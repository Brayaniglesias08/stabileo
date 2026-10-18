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
    if input.modes.is_empty() {
        return Err("Spectral analysis requires at least one mode (run modal analysis first)".to_string());
    }
    let solver_input = &input.solver;
    let modes = &input.modes;
    let spectrum = &input.spectrum;
    let direction = &input.direction;
    let rule = input.rule.as_deref().unwrap_or("CQC");
    let xi = input.xi.unwrap_or(0.05);
    let importance_factor = input.importance_factor.unwrap_or(1.0);
    let reduction_factor = input.reduction_factor.unwrap_or(1.0);
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

    for (mode_idx, mode) in modes.iter().enumerate() {
        // Get Sa from spectrum
        let mut sa = interpolate_spectrum(spectrum, mode.period);
        if spectrum.in_g.unwrap_or(true) {
            sa *= 9.81;
        }
        sa *= importance_factor / reduction_factor;

        // ω > 0 (validated).
        let sd = sa / (mode.omega * mode.omega);

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
        let forces: Vec<f64> = ef.iter()
            .flat_map(|f| vec![f.n_start.abs().max(f.n_end.abs()),
                              f.v_start.abs().max(f.v_end.abs()),
                              f.m_start.abs().max(f.m_end.abs())])
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

    // Combine modal responses
    let n_elems = if modal_forces.is_empty() { 0 } else { modal_forces[0].len() / 3 };

    let (combined_disps, combined_forces) = match rule {
        "SRSS" => combine_srss(&modal_disps, &modal_forces, n, n_elems),
        _ => combine_cqc(&modal_disps, &modal_forces, modes, xi, n, n_elems),
    };

    // Build combined displacement results
    let displacements = super::linear::build_displacements_2d(&dof_num, &combined_disps);

    // Build element force envelopes
    let elem_ids: Vec<usize> = solver_input.elements.values().map(|e| e.id).collect();
    let element_forces: Vec<SpectralElementForce> = (0..n_elems)
        .map(|i| SpectralElementForce {
            element_id: if i < elem_ids.len() { elem_ids[i] } else { 0 },
            n_max: combined_forces[i * 3],
            v_max: combined_forces[i * 3 + 1],
            m_max: combined_forces[i * 3 + 2],
        })
        .collect();

    let base_shear: f64 = per_mode.iter()
        .map(|pm| pm.modal_force * pm.modal_force)
        .sum::<f64>()
        .sqrt();
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

fn combine_srss(
    modal_disps: &[Vec<f64>],
    modal_forces: &[Vec<f64>],
    n: usize,
    n_elems: usize,
) -> (Vec<f64>, Vec<f64>) {
    let mut disps = vec![0.0; n];
    for i in 0..n {
        let sum: f64 = modal_disps.iter().map(|md| md[i] * md[i]).sum();
        disps[i] = sum.sqrt();
    }
    let nf_forces = n_elems * 3;
    let mut forces = vec![0.0; nf_forces];
    for i in 0..nf_forces {
        let sum: f64 = modal_forces.iter().map(|mf| mf[i] * mf[i]).sum();
        forces[i] = sum.sqrt();
    }
    (disps, forces)
}

fn combine_cqc(
    modal_disps: &[Vec<f64>],
    modal_forces: &[Vec<f64>],
    modes: &[SpectralModeInput],
    xi: f64,
    n: usize,
    n_elems: usize,
) -> (Vec<f64>, Vec<f64>) {
    let n_modes = modal_disps.len();

    // CQC correlation coefficients
    let mut rho = vec![vec![0.0; n_modes]; n_modes];
    for i in 0..n_modes {
        for j in 0..n_modes {
            let r = modes[j].omega / modes[i].omega;
            let num = 8.0 * xi * xi * (1.0 + r) * r.powf(1.5);
            let den = (1.0 - r * r).powi(2) + 4.0 * xi * xi * r * (1.0 + r).powi(2);
            rho[i][j] = if den.abs() > 1e-30 { num / den } else { if i == j { 1.0 } else { 0.0 } };
        }
    }

    let mut disps = vec![0.0; n];
    for k in 0..n {
        let mut sum = 0.0;
        for i in 0..n_modes {
            for j in 0..n_modes {
                sum += modal_disps[i][k] * rho[i][j] * modal_disps[j][k];
            }
        }
        disps[k] = sum.abs().sqrt();
    }

    let nf_forces = n_elems * 3;
    let mut forces = vec![0.0; nf_forces];
    for k in 0..nf_forces {
        let mut sum = 0.0;
        for i in 0..n_modes {
            for j in 0..n_modes {
                sum += modal_forces[i][k] * rho[i][j] * modal_forces[j][k];
            }
        }
        forces[k] = sum.abs().sqrt();
    }

    (disps, forces)
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
    let rule = input.rule.as_deref().unwrap_or("CQC");
    let xi = input.xi.unwrap_or(0.05);
    let importance_factor = input.importance_factor.unwrap_or(1.0);
    let reduction_factor = input.reduction_factor.unwrap_or(1.0);
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

    for (mode_idx, mode) in modes.iter().enumerate() {
        let mut sa = interpolate_spectrum(spectrum, mode.period);
        if spectrum.in_g.unwrap_or(true) { sa *= 9.81; }
        sa *= importance_factor / reduction_factor;

        // ω > 0 (validated).
        let sd = sa / (mode.omega * mode.omega);

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
        let forces: Vec<f64> = ef.iter()
            .flat_map(|f| vec![
                f.n_start.abs().max(f.n_end.abs()),
                f.vy_start.abs().max(f.vy_end.abs()),
                f.vz_start.abs().max(f.vz_end.abs()),
                f.mx_start.abs().max(f.mx_end.abs()),
                f.my_start.abs().max(f.my_end.abs()),
                f.mz_start.abs().max(f.mz_end.abs()),
            ])
            .collect();
        modal_forces.push(forces);

        per_mode.push(PerModeResult {
            mode: mode_idx + 1, period: mode.period, sa, sd, participation, modal_force,
        });
    }

    let n_elems = if modal_forces.is_empty() { 0 } else { modal_forces[0].len() / 6 };

    let n_force_components = n_elems * 6;
    let (combined_disps, combined_forces) = match rule {
        "SRSS" => {
            let mut disps = vec![0.0; n];
            for i in 0..n {
                disps[i] = modal_disps.iter().map(|md| md[i] * md[i]).sum::<f64>().sqrt();
            }
            let mut forces = vec![0.0; n_force_components];
            for i in 0..n_force_components {
                forces[i] = modal_forces.iter().map(|mf| mf[i] * mf[i]).sum::<f64>().sqrt();
            }
            (disps, forces)
        }
        _ => {
            let omegas: Vec<f64> = modes.iter().map(|m| m.omega).collect();
            let n_modes = modal_disps.len();
            let mut rho = vec![vec![0.0; n_modes]; n_modes];
            for i in 0..n_modes {
                for j in 0..n_modes {
                    let r = omegas[j] / omegas[i];
                    let num = 8.0 * xi * xi * (1.0 + r) * r.powf(1.5);
                    let den = (1.0 - r * r).powi(2) + 4.0 * xi * xi * r * (1.0 + r).powi(2);
                    rho[i][j] = if den.abs() > 1e-30 { num / den } else { if i == j { 1.0 } else { 0.0 } };
                }
            }
            let mut disps = vec![0.0; n];
            for k in 0..n {
                let mut sum = 0.0;
                for i in 0..n_modes { for j in 0..n_modes { sum += modal_disps[i][k] * rho[i][j] * modal_disps[j][k]; } }
                disps[k] = sum.abs().sqrt();
            }
            let mut forces = vec![0.0; n_force_components];
            for k in 0..n_force_components {
                let mut sum = 0.0;
                for i in 0..n_modes { for j in 0..n_modes { sum += modal_forces[i][k] * rho[i][j] * modal_forces[j][k]; } }
                forces[k] = sum.abs().sqrt();
            }
            (disps, forces)
        }
    };

    let displacements = super::linear::build_displacements_3d(&dof_num, &combined_disps);

    let elem_ids: Vec<usize> = solver_input.elements.values().map(|e| e.id).collect();
    let element_forces: Vec<SpectralElementForce3D> = (0..n_elems)
        .map(|i| SpectralElementForce3D {
            element_id: if i < elem_ids.len() { elem_ids[i] } else { 0 },
            n_max: combined_forces[i * 6],
            vy_max: combined_forces[i * 6 + 1],
            vz_max: combined_forces[i * 6 + 2],
            mx_max: combined_forces[i * 6 + 3],
            my_max: combined_forces[i * 6 + 4],
            mz_max: combined_forces[i * 6 + 5],
        })
        .collect();

    let base_shear: f64 = per_mode.iter()
        .map(|pm| pm.modal_force * pm.modal_force).sum::<f64>().sqrt();
    check_finite_output(&combined_disps, &combined_forces, base_shear, &per_mode)?;

    Ok(SpectralResult3D {
        displacements, element_forces, base_shear, per_mode, rule: rule.to_string(),
    })
}
