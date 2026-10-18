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

    Ok(SpectralResult3D {
        displacements, element_forces, base_shear, per_mode, rule: rule.to_string(),
    })
}
