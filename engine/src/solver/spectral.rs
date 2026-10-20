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

/// Check the spectral data the solver trusts (issue #335): every mode has a
/// finite ω > 0 and period > 0 and finite participation, effective mass and
/// shape; the spectrum has finite ordinates at finite, strictly increasing
/// periods; ξ, the importance and reduction factors are finite, ξ ≥ 0 and the
/// factors > 0. A negative ω made the CQC ratio ω_j/ω_i negative, r^1.5 NaN,
/// and every combined displacement and force NaN; ω ≤ 0 also dropped the
/// mode silently (S_d = 0).
// `!(x > 0.0)` is deliberate: it also rejects NaN.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn validate_spectral(
    modes: &[(f64, f64, Vec<f64>)],
    spectrum: &DesignSpectrum,
    xi: f64,
    importance: f64,
    reduction: f64,
) -> Result<(), String> {
    for (i, (omega, period, rest)) in modes.iter().enumerate() {
        // `!(x > 0.0)` also rejects NaN.
        if !omega.is_finite() || !(*omega > 0.0) {
            return Err(format!("Spectral analysis: mode {} has omega = {omega}; it must be finite and > 0", i + 1));
        }
        if !period.is_finite() || !(*period > 0.0) {
            return Err(format!("Spectral analysis: mode {} has period = {period}; it must be finite and > 0", i + 1));
        }
        if let Some(x) = rest.iter().find(|x| !x.is_finite()) {
            return Err(format!(
                "Spectral analysis: mode {} has a non-finite participation factor, effective mass or shape value ({x})",
                i + 1
            ));
        }
    }
    if let Some(p) = spectrum.points.iter().find(|p| !p.period.is_finite() || !p.sa.is_finite()) {
        return Err(format!("Spectral analysis: spectrum point (T = {}, Sa = {}) is not finite", p.period, p.sa));
    }
    if spectrum.points.windows(2).any(|w| !(w[0].period < w[1].period)) {
        return Err("Spectral analysis: spectrum periods must be strictly increasing".into());
    }
    if !xi.is_finite() || xi < 0.0 {
        return Err(format!("Spectral analysis: damping ratio xi = {xi} must be finite and >= 0"));
    }
    if !importance.is_finite() || !(importance > 0.0) {
        return Err(format!("Spectral analysis: importance factor {importance} must be finite and > 0"));
    }
    if !reduction.is_finite() || !(reduction > 0.0) {
        return Err(format!("Spectral analysis: reduction factor {reduction} must be finite and > 0"));
    }
    Ok(())
}

/// The modal combination rule: absent means CQC; anything other than
/// "SRSS" or "CQC" is refused (it used to fall back to CQC silently).
pub(crate) fn spectral_rule(rule: Option<&str>) -> Result<&'static str, String> {
    match rule {
        None | Some("CQC") => Ok("CQC"),
        Some("SRSS") => Ok("SRSS"),
        Some(r) => Err(format!("Spectral analysis: unknown combination rule '{r}'; expected \"SRSS\" or \"CQC\"")),
    }
}

/// Index of the excitation direction among `accepted` ("X", "Y"[, "Z"]);
/// anything else is refused (it used to fall back to Y in 2D and Z in 3D).
pub(crate) fn spectral_direction(direction: &str, accepted: &[&str]) -> Result<usize, String> {
    accepted.iter().position(|&d| d == direction).ok_or_else(|| {
        format!(
            "Spectral analysis: unknown direction '{direction}'; expected one of {}",
            accepted.iter().map(|d| format!("\"{d}\"")).collect::<Vec<_>>().join(", ")
        )
    })
}

/// Backstop after the combination: a result with a non-finite number (an
/// overflow on huge but finite input) is refused, never returned.
fn check_finite_output(disps: &[f64], forces: &[f64], base_shear: f64, per_mode: &[PerModeResult]) -> Result<(), String> {
    let modal = per_mode.iter().flat_map(|m| [m.period, m.sa, m.sd, m.participation, m.modal_force]);
    if disps.iter().chain(forces).copied().chain(modal).all(f64::is_finite) && base_shear.is_finite() {
        Ok(())
    } else {
        Err("Spectral analysis: the combined response is not finite (overflow; check the modes, spectrum and factors)".into())
    }
}

/// Solve 2D spectral analysis using pre-computed modal data.
pub fn solve_spectral_2d(input: &SpectralInput) -> Result<SpectralResult, String> {
    super::linear::validate_input_2d(&input.solver)?;
    // The certificate re-assembles M from these masses.
    super::dynamic_validation::validate_mass_input_2d(&input.solver)?;
    if input.modes.is_empty() {
        return Err("Spectral analysis requires at least one mode (run modal analysis first)".to_string());
    }
    let solver_input = &input.solver;
    let modes = &input.modes;
    let spectrum = &input.spectrum;
    let direction = &input.direction;
    let rule = spectral_rule(input.rule.as_deref())?;
    let xi = input.xi.unwrap_or(0.05);
    let importance_factor = input.importance_factor.unwrap_or(1.0);
    let reduction_factor = input.reduction_factor.unwrap_or(1.0);
    let dir = spectral_direction(direction, &["X", "Y"])?;
    let checked: Vec<(f64, f64, Vec<f64>)> = modes
        .iter()
        .map(|m| {
            let mut rest = vec![m.participation_x, m.participation_y, m.effective_mass_x, m.effective_mass_y];
            rest.extend(m.displacements.iter().flat_map(|d| [d.ux, d.uz, d.ry]));
            (m.omega, m.period, rest)
        })
        .collect();
    validate_spectral(&checked, spectrum, xi, importance_factor, reduction_factor)?;

    let dof_num = DofNumbering::build_2d(solver_input);
    let n = dof_num.n_total;

    // For each mode, compute peak response
    let mut modal_disps: Vec<Vec<f64>> = Vec::new();
    let mut modal_forces: Vec<Vec<f64>> = Vec::new();
    let mut per_mode = Vec::new();
    let mut elem_ids: Vec<usize> = Vec::new();

    for (mode_idx, mode) in modes.iter().enumerate() {
        // Get Sa from spectrum
        let mut sa = interpolate_spectrum(spectrum, mode.period);
        if spectrum.in_g.unwrap_or(true) {
            sa *= 9.81;
        }
        sa *= importance_factor / reduction_factor;

        // ω > 0 (validated).
        let sd = sa / (mode.omega * mode.omega);

        let (participation, meff) = if dir == 0 {
            (mode.participation_x, mode.effective_mass_x)
        } else {
            (mode.participation_y, mode.effective_mass_y)
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
        let mut ef = super::linear::compute_internal_forces_2d_with_loads(solver_input, &[], &dof_num, &u_full);
        ef.sort_by_key(|f| f.element_id);
        if elem_ids.is_empty() {
            elem_ids = ef.iter().map(|f| f.element_id).collect();
        }
        // Signed end values: the modal combination must see the signs (two
        // modes of opposite sign at an end partly cancel under CQC).
        let forces: Vec<f64> = ef.iter()
            .flat_map(|f| [f.n_start, f.n_end, f.v_start, f.v_end, f.m_start, f.m_end])
            .collect();
        modal_forces.push(forces);

        per_mode.push(PerModeResult {
            mode: mode_idx + 1,
            period: mode.period,
            sa,
            sd,
            participation,
            modal_force,
        });
    }

    // Combine modal responses (signed), then envelope each member's two ends.
    let rho = (rule != "SRSS").then(|| cqc_rho(&modes.iter().map(|m| m.omega).collect::<Vec<_>>(), xi));
    let combined_disps = combine_modal(&modal_disps, rho.as_deref());
    let combined_forces = combine_modal(&modal_forces, rho.as_deref());

    // Build combined displacement results
    let displacements = super::linear::build_displacements_2d(&dof_num, &combined_disps);

    // Element ids from the force records themselves (the HashMap iteration
    // order they used to be paired with only matched by coincidence).
    let element_forces: Vec<SpectralElementForce> = elem_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let c = &combined_forces[i * 6..i * 6 + 6];
            SpectralElementForce { element_id: id, n_max: c[0].max(c[1]), v_max: c[2].max(c[3]), m_max: c[4].max(c[5]) }
        })
        .collect();

    // Base shear combined by the same rule as every other response: SRSS of
    // the modal base shears under CQC ignored the correlation of closely
    // spaced modes (and depended on the arbitrary basis of a repeated one).
    let base_shear = combine_modal(&per_mode.iter().map(|pm| vec![pm.modal_force]).collect::<Vec<_>>(), rho.as_deref())[0];
    check_finite_output(&combined_disps, &combined_forces, base_shear, &per_mode)?;

    Ok(SpectralResult {
        displacements,
        element_forces,
        base_shear,
        per_mode,
        rule: rule.to_string(),
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

/// CQC correlation coefficients (Der Kiureghian, equal damping ξ).
fn cqc_rho(omegas: &[f64], xi: f64) -> Vec<Vec<f64>> {
    let n = omegas.len();
    let mut rho = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            let r = omegas[j] / omegas[i];
            let num = 8.0 * xi * xi * (1.0 + r) * r.powf(1.5);
            let den = (1.0 - r * r).powi(2) + 4.0 * xi * xi * r * (1.0 + r).powi(2);
            rho[i][j] = if den.abs() > 1e-30 { num / den } else if i == j { 1.0 } else { 0.0 };
        }
    }
    rho
}

/// Combine signed modal values entry by entry: √(Σ ρ_ij r_i r_j) (CQC) or
/// √(Σ r_i²) (SRSS, `rho = None`).
fn combine_modal(modal: &[Vec<f64>], rho: Option<&[Vec<f64>]>) -> Vec<f64> {
    let len = modal.first().map_or(0, |m| m.len());
    (0..len)
        .map(|k| {
            let sum: f64 = match rho {
                None => modal.iter().map(|m| m[k] * m[k]).sum(),
                Some(rho) => (0..modal.len()).map(|i| (0..modal.len()).map(|j| modal[i][k] * rho[i][j] * modal[j][k]).sum::<f64>()).sum(),
            };
            sum.abs().sqrt()
        })
        .collect()
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
    // The certificate re-assembles M from these masses.
    super::dynamic_validation::validate_mass_input_3d(&input.solver)?;
    if input.modes.is_empty() {
        return Err("Spectral analysis requires at least one mode (run modal analysis first)".to_string());
    }
    let solver_input = &input.solver;
    let modes = &input.modes;
    let spectrum = &input.spectrum;
    let direction = &input.direction;
    let rule = spectral_rule(input.rule.as_deref())?;
    let xi = input.xi.unwrap_or(0.05);
    let importance_factor = input.importance_factor.unwrap_or(1.0);
    let reduction_factor = input.reduction_factor.unwrap_or(1.0);
    let dir = spectral_direction(direction, &["X", "Y", "Z"])?;
    let checked: Vec<(f64, f64, Vec<f64>)> = modes
        .iter()
        .map(|m| {
            let mut rest = vec![
                m.participation_x, m.participation_y, m.participation_z,
                m.effective_mass_x, m.effective_mass_y, m.effective_mass_z,
            ];
            rest.extend(m.displacements.iter().flat_map(|d| [d.ux, d.uy, d.uz, d.rx, d.ry, d.rz]));
            (m.omega, m.period, rest)
        })
        .collect();
    validate_spectral(&checked, spectrum, xi, importance_factor, reduction_factor)?;

    let dof_num = DofNumbering::build_3d(solver_input);
    let n = dof_num.n_total;

    let mut modal_disps: Vec<Vec<f64>> = Vec::new();
    let mut modal_forces: Vec<Vec<f64>> = Vec::new();
    let mut per_mode = Vec::new();
    let mut elem_ids: Vec<usize> = Vec::new();

    for (mode_idx, mode) in modes.iter().enumerate() {
        let mut sa = interpolate_spectrum(spectrum, mode.period);
        if spectrum.in_g.unwrap_or(true) { sa *= 9.81; }
        sa *= importance_factor / reduction_factor;

        // ω > 0 (validated).
        let sd = sa / (mode.omega * mode.omega);

        let (participation, meff) = match dir {
            0 => (mode.participation_x, mode.effective_mass_x),
            1 => (mode.participation_y, mode.effective_mass_y),
            _ => (mode.participation_z, mode.effective_mass_z),
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
        let mut ef = super::linear::compute_internal_forces_3d_with_loads(solver_input, &[], &dof_num, &u_full);
        ef.sort_by_key(|f| f.element_id);
        if elem_ids.is_empty() {
            elem_ids = ef.iter().map(|f| f.element_id).collect();
        }
        // Signed end values (see the 2D solver).
        let forces: Vec<f64> = ef.iter()
            .flat_map(|f| [
                f.n_start, f.n_end, f.vy_start, f.vy_end, f.vz_start, f.vz_end,
                f.mx_start, f.mx_end, f.my_start, f.my_end, f.mz_start, f.mz_end,
            ])
            .collect();
        modal_forces.push(forces);

        per_mode.push(PerModeResult {
            mode: mode_idx + 1, period: mode.period, sa, sd, participation, modal_force,
        });
    }

    let rho = (rule != "SRSS").then(|| cqc_rho(&modes.iter().map(|m| m.omega).collect::<Vec<_>>(), xi));
    let combined_disps = combine_modal(&modal_disps, rho.as_deref());
    let combined_forces = combine_modal(&modal_forces, rho.as_deref());

    let displacements = super::linear::build_displacements_3d(&dof_num, &combined_disps);

    let element_forces: Vec<SpectralElementForce3D> = elem_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let c = &combined_forces[i * 12..i * 12 + 12];
            SpectralElementForce3D {
                element_id: id,
                n_max: c[0].max(c[1]),
                vy_max: c[2].max(c[3]),
                vz_max: c[4].max(c[5]),
                mx_max: c[6].max(c[7]),
                my_max: c[8].max(c[9]),
                mz_max: c[10].max(c[11]),
            }
        })
        .collect();

    let base_shear = combine_modal(&per_mode.iter().map(|pm| vec![pm.modal_force]).collect::<Vec<_>>(), rho.as_deref())[0];
    check_finite_output(&combined_disps, &combined_forces, base_shear, &per_mode)?;

    Ok(SpectralResult3D {
        displacements, element_forces, base_shear, per_mode, rule: rule.to_string(),
    })
}
