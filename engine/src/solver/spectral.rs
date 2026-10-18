use crate::types::*;
use super::dof::DofNumbering;

/// Spectral analysis result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpectralResult {
    pub displacements: Vec<Displacement>,
    pub element_forces: Vec<SpectralElementForce>,
    pub base_shear: f64,
    pub per_mode: Vec<PerModeResult>,
    pub rule: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpectralElementForce {
    pub element_id: usize,
    pub n_max: f64,
    pub v_max: f64,
    pub m_max: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerModeResult {
    pub mode: usize,
    pub period: f64,
    pub sa: f64,
    pub sd: f64,
    pub participation: f64,
    pub modal_force: f64,
}

/// The modal combination rule of a response-spectrum analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombinationRule {
    /// Square root of the sum of squares.
    Srss,
    /// Complete quadratic combination, Der Kiureghian (1981), equal modal damping.
    Cqc,
}

impl CombinationRule {
    /// `None` is CQC. Anything other than SRSS or CQC (any case) is refused: an
    /// unknown rule used to fall through to CQC silently.
    pub fn parse(rule: Option<&str>) -> Result<Self, String> {
        match rule.map(|r| r.trim().to_ascii_uppercase()).as_deref() {
            None | Some("CQC") => Ok(CombinationRule::Cqc),
            Some("SRSS") => Ok(CombinationRule::Srss),
            Some(other) => Err(format!("unknown modal combination rule '{other}' (expected SRSS or CQC)")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CombinationRule::Srss => "SRSS",
            CombinationRule::Cqc => "CQC",
        }
    }
}

/// Der Kiureghian's CQC coefficient for equal damping `xi`, `r = ω_j/ω_i`:
/// ρ = 8ξ²(1+r)r^{3/2} / ((1−r²)² + 4ξ²r(1+r)²). Symmetric in i, j; ρ_ii = 1.
pub fn cqc_coefficient(omega_i: f64, omega_j: f64, xi: f64) -> f64 {
    let r = omega_j / omega_i;
    let num = 8.0 * xi * xi * (1.0 + r) * r.powf(1.5);
    let den = (1.0 - r * r).powi(2) + 4.0 * xi * xi * r * (1.0 + r).powi(2);
    if den > 1e-30 {
        num / den
    } else if (r - 1.0).abs() < 1e-12 {
        // Coincident frequencies without damping: the limit is full correlation.
        1.0
    } else {
        0.0
    }
}

/// Combines per-mode values of one response quantity with the requested rule.
/// Every reported quantity (displacements, end forces, base shear) goes
/// through the same combiner: the base shear used to be SRSS whatever the
/// rule, which under-reported it when CQC was asked for (closely spaced modes
/// add their base shears with the same sign).
pub(crate) struct ModalCombiner {
    /// CQC coefficients, None for SRSS.
    rho: Option<Vec<Vec<f64>>>,
}

impl ModalCombiner {
    pub(crate) fn new(rule: CombinationRule, omegas: &[f64], xi: f64) -> Result<Self, String> {
        let rho = match rule {
            CombinationRule::Srss => None,
            CombinationRule::Cqc => {
                if !(xi.is_finite() && (0.0..1.0).contains(&xi)) {
                    return Err(format!("CQC damping ratio must be in [0, 1), got {xi}"));
                }
                if omegas.iter().any(|w| !(w.is_finite() && *w > 0.0)) {
                    return Err("CQC needs every mode's circular frequency finite and positive".into());
                }
                Some(
                    (0..omegas.len())
                        .map(|i| (0..omegas.len()).map(|j| if i == j { 1.0 } else { cqc_coefficient(omegas[i], omegas[j], xi) }).collect())
                        .collect(),
                )
            }
        };
        Ok(ModalCombiner { rho })
    }

    /// The combined peak of a quantity whose signed value in mode `i` is `r(i)`.
    pub(crate) fn combine(&self, n_modes: usize, r: impl Fn(usize) -> f64) -> f64 {
        let vals: Vec<f64> = (0..n_modes).map(r).collect();
        let s: f64 = match &self.rho {
            None => vals.iter().map(|v| v * v).sum(),
            Some(rho) => {
                let mut s = 0.0;
                for i in 0..n_modes {
                    for j in 0..n_modes {
                        s += vals[i] * rho[i][j] * vals[j];
                    }
                }
                s
            }
        };
        s.abs().sqrt()
    }
}

/// Signed per-mode end forces of every member, `(element id, values)`: `values` is
/// `[start components…, end components…]`. Each end component is a response
/// quantity linear in the modal displacement, and it is that signed value the
/// rule combines; the envelope of the two ends is taken after combining.
/// (Combining per-mode max(|start|, |end|) made every CQC cross term
/// non-negative and mixed the two ends across modes.)
fn combine_end_forces(
    combiner: &ModalCombiner,
    per_mode: &[Vec<(usize, Vec<f64>)>],
    n_comp: usize,
) -> Vec<(usize, Vec<f64>)> {
    let n_modes = per_mode.len();
    let Some(first) = per_mode.first() else { return Vec::new() };
    first
        .iter()
        .enumerate()
        .map(|(e, (id, _))| {
            let vals = (0..n_comp)
                .map(|c| {
                    let start = combiner.combine(n_modes, |m| per_mode[m][e].1[c]);
                    let end = combiner.combine(n_modes, |m| per_mode[m][e].1[n_comp + c]);
                    start.max(end)
                })
                .collect();
            (*id, vals)
        })
        .collect()
}

/// Solve 2D spectral analysis using pre-computed modal data.
pub fn solve_spectral_2d(input: &SpectralInput) -> Result<SpectralResult, String> {
    super::linear::validate_input_2d(&input.solver)?;
    if input.modes.is_empty() {
        return Err("Spectral analysis requires at least one mode (run modal analysis first)".to_string());
    }
    let solver_input = &input.solver;
    let modes = &input.modes;
    let spectrum = &input.spectrum;
    let direction = &input.direction;
    let rule = CombinationRule::parse(input.rule.as_deref())?;
    let xi = input.xi.unwrap_or(0.05);
    let importance_factor = input.importance_factor.unwrap_or(1.0);
    let reduction_factor = input.reduction_factor.unwrap_or(1.0);
    let omegas: Vec<f64> = modes.iter().map(|m| m.omega).collect();
    let combiner = ModalCombiner::new(rule, &omegas, xi)?;

    let dof_num = DofNumbering::build_2d(solver_input);
    let n = dof_num.n_total;

    // For each mode, compute peak response
    let mut modal_disps: Vec<Vec<f64>> = Vec::new();
    let mut modal_forces: Vec<Vec<(usize, Vec<f64>)>> = Vec::new();
    let mut per_mode = Vec::new();

    for (mode_idx, mode) in modes.iter().enumerate() {
        // Get Sa from spectrum
        let mut sa = interpolate_spectrum(spectrum, mode.period);
        if spectrum.in_g.unwrap_or(true) {
            sa *= 9.81;
        }
        sa *= importance_factor / reduction_factor;

        let sd = if mode.omega > 1e-10 { sa / (mode.omega * mode.omega) } else { 0.0 };

        let participation = match direction.as_str() {
            "X" => mode.participation_x,
            _ => mode.participation_y,
        };
        let meff = match direction.as_str() {
            "X" => mode.effective_mass_x,
            _ => mode.effective_mass_y,
        };
        let modal_force = meff * sa;

        // Modal displacement vector over all DOFs, in global axes like the
        // mode shapes: at an inclined roller both components move although
        // only the tangential DOF is free, so restrained slots are kept too
        // (they are zero everywhere else).
        let mut u_full = vec![0.0; n];
        for d in &mode.displacements {
            if let Some(&idx) = dof_num.map.get(&(d.node_id, 0)) {
                u_full[idx] = d.ux * participation * sd;
            }
            if let Some(&idx) = dof_num.map.get(&(d.node_id, 1)) {
                u_full[idx] = d.uz * participation * sd;
            }
            if dof_num.dofs_per_node >= 3 {
                if let Some(&idx) = dof_num.map.get(&(d.node_id, 2)) {
                    u_full[idx] = d.ry * participation * sd;
                }
            }
        }
        modal_disps.push(u_full.clone());

        // Compute element forces for this mode
        // The mode's response to its spectral inertia forces only: no element
        // loads, whose fixed-end forces are static and must not be combined.
        let ef = super::linear::compute_internal_forces_2d_with_loads(solver_input, &[], &dof_num, &u_full);
        modal_forces.push(
            ef.iter()
                .map(|f| (f.element_id, vec![f.n_start, f.v_start, f.m_start, f.n_end, f.v_end, f.m_end]))
                .collect(),
        );

        per_mode.push(PerModeResult {
            mode: mode_idx + 1,
            period: mode.period,
            sa,
            sd,
            participation,
            modal_force,
        });
    }

    let n_modes = modes.len();
    let combined_disps: Vec<f64> = (0..n).map(|k| combiner.combine(n_modes, |m| modal_disps[m][k])).collect();
    let displacements = super::linear::build_displacements_2d(&dof_num, &combined_disps);

    let element_forces: Vec<SpectralElementForce> = combine_end_forces(&combiner, &modal_forces, 3)
        .into_iter()
        .map(|(element_id, v)| SpectralElementForce { element_id, n_max: v[0], v_max: v[1], m_max: v[2] })
        .collect();

    // Modal base shears V_n = M*_n·Sa_n, combined with the requested rule.
    let base_shear = combiner.combine(n_modes, |m| per_mode[m].modal_force);

    Ok(SpectralResult {
        displacements,
        element_forces,
        base_shear,
        per_mode,
        rule: rule.as_str().to_string(),
    })
}

fn interpolate_spectrum(spectrum: &DesignSpectrum, period: f64) -> f64 {
    if spectrum.points.is_empty() {
        return 0.0;
    }
    if period <= spectrum.points[0].period {
        return spectrum.points[0].sa;
    }
    let last = spectrum.points.last().unwrap();
    if period >= last.period {
        return last.sa;
    }
    for i in 0..spectrum.points.len() - 1 {
        let p1 = &spectrum.points[i];
        let p2 = &spectrum.points[i + 1];
        if period >= p1.period && period <= p2.period {
            let t = (period - p1.period) / (p2.period - p1.period);
            return p1.sa + t * (p2.sa - p1.sa);
        }
    }
    last.sa
}

/// 3D spectral analysis result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpectralResult3D {
    pub displacements: Vec<Displacement3D>,
    pub element_forces: Vec<SpectralElementForce3D>,
    pub base_shear: f64,
    pub per_mode: Vec<PerModeResult>,
    pub rule: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpectralElementForce3D {
    pub element_id: usize,
    pub n_max: f64,
    pub vy_max: f64,
    pub vz_max: f64,
    pub mx_max: f64,
    pub my_max: f64,
    pub mz_max: f64,
}

/// Solve 3D spectral analysis using pre-computed modal data.
pub fn solve_spectral_3d(input: &SpectralInput3D) -> Result<SpectralResult3D, String> {
    super::linear::validate_input_3d(&input.solver)?;
    if input.modes.is_empty() {
        return Err("Spectral analysis requires at least one mode (run modal analysis first)".to_string());
    }
    let solver_input = &input.solver;
    let modes = &input.modes;
    let spectrum = &input.spectrum;
    let direction = &input.direction;
    let rule = CombinationRule::parse(input.rule.as_deref())?;
    let xi = input.xi.unwrap_or(0.05);
    let importance_factor = input.importance_factor.unwrap_or(1.0);
    let reduction_factor = input.reduction_factor.unwrap_or(1.0);
    let omegas: Vec<f64> = modes.iter().map(|m| m.omega).collect();
    let combiner = ModalCombiner::new(rule, &omegas, xi)?;

    let dof_num = DofNumbering::build_3d(solver_input);
    let n = dof_num.n_total;

    let mut modal_disps: Vec<Vec<f64>> = Vec::new();
    let mut modal_forces: Vec<Vec<(usize, Vec<f64>)>> = Vec::new();
    let mut per_mode = Vec::new();

    for (mode_idx, mode) in modes.iter().enumerate() {
        let mut sa = interpolate_spectrum(spectrum, mode.period);
        if spectrum.in_g.unwrap_or(true) { sa *= 9.81; }
        sa *= importance_factor / reduction_factor;

        let sd = if mode.omega > 1e-10 { sa / (mode.omega * mode.omega) } else { 0.0 };

        let participation = match direction.as_str() {
            "X" => mode.participation_x,
            "Y" => mode.participation_y,
            _ => mode.participation_z,
        };
        let meff = match direction.as_str() {
            "X" => mode.effective_mass_x,
            "Y" => mode.effective_mass_y,
            _ => mode.effective_mass_z,
        };
        let modal_force = meff * sa;

        // Over all DOFs in global axes (see the 2D solver).
        let mut u_full = vec![0.0; n];
        for d in &mode.displacements {
            let vals = [d.ux, d.uy, d.uz, d.rx, d.ry, d.rz];
            for (i, &val) in vals.iter().enumerate() {
                if let Some(&idx) = dof_num.map.get(&(d.node_id, i)) {
                    u_full[idx] = val * participation * sd;
                }
            }
        }
        modal_disps.push(u_full.clone());
        // Spectral inertia forces only: no element loads (see the 2D solver).
        let ef = super::linear::compute_internal_forces_3d_with_loads(solver_input, &[], &dof_num, &u_full);
        modal_forces.push(
            ef.iter()
                .map(|f| {
                    (f.element_id, vec![
                        f.n_start, f.vy_start, f.vz_start, f.mx_start, f.my_start, f.mz_start,
                        f.n_end, f.vy_end, f.vz_end, f.mx_end, f.my_end, f.mz_end,
                    ])
                })
                .collect(),
        );

        per_mode.push(PerModeResult {
            mode: mode_idx + 1, period: mode.period, sa, sd, participation, modal_force,
        });
    }

    let n_modes = modes.len();
    let combined_disps: Vec<f64> = (0..n).map(|k| combiner.combine(n_modes, |m| modal_disps[m][k])).collect();
    let displacements = super::linear::build_displacements_3d(&dof_num, &combined_disps);

    let element_forces: Vec<SpectralElementForce3D> = combine_end_forces(&combiner, &modal_forces, 6)
        .into_iter()
        .map(|(element_id, v)| SpectralElementForce3D {
            element_id,
            n_max: v[0],
            vy_max: v[1],
            vz_max: v[2],
            mx_max: v[3],
            my_max: v[4],
            mz_max: v[5],
        })
        .collect();

    // Modal base shears combined with the requested rule (see the 2D solver).
    let base_shear = combiner.combine(n_modes, |m| per_mode[m].modal_force);

    Ok(SpectralResult3D {
        displacements, element_forces, base_shear, per_mode, rule: rule.as_str().to_string(),
    })
}
