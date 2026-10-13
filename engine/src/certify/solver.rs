//! Certify the displacements returned by the linear static solvers.
//!
//! The certified system is the reduced free-DOF system `K_FF u_F = f_F − K_FR u_R`
//! of the dense assembly (`assemble_stiffness_{2d,3d}`, the same assembly the
//! solver's dense path factors), with `ũ_F` read back from the solver's own
//! returned displacements (no re-solve). The effective load `f_F − K_FR u_R`
//! is evaluated in interval arithmetic, so prescribed displacements add no
//! unaccounted rounding. Springs, hinges' artificial stiffness and connectors
//! are part of `K` and need nothing special.
//!
//! Unsupported (returns `Uncertified`): constraints (the constrained solver
//! transforms the system), inclined supports (returned displacements are
//! rotated back to global axes).
//!
//! Small models (n_free < `SPARSE_THRESHOLD`) use the dense assembly and the
//! dense interval-Cholesky certificate (`linear.rs`). Larger ones use the
//! triplet sparse assembly the solver's sparse path factors, and the Gram
//! residual + Gershgorin certificate (`sparse.rs`), which has no size cap.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::interval::Interval;
use super::linear::{certify_linear_system_auto_interval_rhs, Certificate, MAX_CERTIFY_N};
use super::sparse::certify_sparse_spd_system;
use crate::linalg::sparse::CscMatrix;
use crate::solver::assembly::{
    assemble_load_vector_2d, assemble_load_vector_3d_dense, assemble_stiffness_2d, assemble_stiffness_3d,
    assemble_stiffness_sparse_3d,
};
use crate::solver::sparse_assembly::{assemble_load_vector_sparse_3d, assemble_stiffness_sparse_2d};
use crate::solver::dof::DofNumbering;
use crate::solver::linear::{expand_curved_beams_3d, solve_2d, solve_3d, SPARSE_THRESHOLD};
use crate::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertifiedResults2D {
    pub results: AnalysisResults,
    pub certificate: Certificate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertifiedResults3D {
    pub results: AnalysisResults3D,
    pub certificate: Certificate,
}

/// Solve a 2D linear static model with `solve_2d` and certify its displacements.
pub fn certify_linear_2d(input: &SolverInput) -> Result<(AnalysisResults, Certificate), String> {
    let results = solve_2d(input)?;
    let cert = certify_results_2d(input, &results);
    Ok((results, cert))
}

/// Solve a 3D linear static model with `solve_3d` and certify its displacements.
pub fn certify_linear_3d(input: &SolverInput3D) -> Result<(AnalysisResults3D, Certificate), String> {
    let results = solve_3d(input)?;
    let cert = certify_results_3d(input, &results);
    Ok((results, cert))
}

fn size_check(dof_num: &DofNumbering) -> Result<(), Certificate> {
    let nf = dof_num.n_free;
    if nf > MAX_CERTIFY_N || dof_num.n_total > 4 * MAX_CERTIFY_N {
        return Err(Certificate::uncertified(format!(
            "too large: {nf} free DOFs exceeds the dense certification cap {MAX_CERTIFY_N}"
        )));
    }
    Ok(())
}

/// Certify given results of `solve_2d(input)` (the input the results came from).
pub fn certify_results_2d(input: &SolverInput, results: &AnalysisResults) -> Certificate {
    if !input.constraints.is_empty() {
        return Certificate::uncertified("unsupported: constraints (constrained solver path)");
    }
    if input.supports.values().any(|s| s.support_type == "inclinedRoller") {
        return Certificate::uncertified("unsupported: inclined supports");
    }
    let dof_num = DofNumbering::build_2d(input);
    if dof_num.n_free >= SPARSE_THRESHOLD {
        // The solver's sparse path: certify the triplet-assembled K it factors.
        let stiff = assemble_stiffness_sparse_2d(input, &dof_num);
        if !stiff.inclined_transforms_2d.is_empty() {
            return Certificate::uncertified("unsupported: inclined supports");
        }
        let f = assemble_load_vector_2d(input, &input.loads, &dof_num, &stiff.inclined_transforms_2d);
        return match gather_u_2d(&dof_num, results) {
            Ok(u) => certify_reduced_sparse(&stiff.k_ff, &stiff.k_full, dof_num.n_free, &f, &u),
            Err(c) => c,
        };
    }
    if let Err(c) = size_check(&dof_num) {
        return c;
    }
    let stiff = assemble_stiffness_2d(input, &dof_num);
    if !stiff.inclined_transforms_2d.is_empty() {
        return Certificate::uncertified("unsupported: inclined supports");
    }
    let f = assemble_load_vector_2d(input, &input.loads, &dof_num, &stiff.inclined_transforms_2d);
    match gather_u_2d(&dof_num, results) {
        Ok(u) => certify_reduced(&stiff.k, dof_num.n_total, dof_num.n_free, &f, &u),
        Err(c) => c,
    }
}

fn gather_u_2d(dof_num: &DofNumbering, results: &AnalysisResults) -> Result<Vec<f64>, Certificate> {
    let disp: HashMap<usize, &Displacement> = results.displacements.iter().map(|d| (d.node_id, d)).collect();
    let mut u = vec![0.0; dof_num.n_total];
    for (&(node, local), &g) in &dof_num.map {
        let Some(d) = disp.get(&node) else {
            return Err(Certificate::uncertified(format!("no displacement returned for node {node}")));
        };
        u[g] = match local {
            0 => d.ux,
            1 => d.uz,
            2 => d.ry,
            _ => return Err(Certificate::uncertified(format!("unexpected 2D local DOF {local}"))),
        };
    }
    Ok(u)
}

/// Certify given results of `solve_3d(input)` (the input the results came from).
pub fn certify_results_3d(input: &SolverInput3D, results: &AnalysisResults3D) -> Certificate {
    let input = &expand_curved_beams_3d(input);
    if !input.constraints.is_empty() {
        return Certificate::uncertified("unsupported: constraints (constrained solver path)");
    }
    if input.supports.values().any(|s| s.is_inclined.unwrap_or(false)) {
        return Certificate::uncertified("unsupported: inclined supports");
    }
    let dof_num = DofNumbering::build_3d(input);
    if dof_num.n_free >= SPARSE_THRESHOLD {
        // The solver's sparse path: certify the triplet-assembled K it factors.
        let stiff = assemble_stiffness_sparse_3d(input, &dof_num, true);
        if !stiff.inclined_transforms.is_empty() {
            return Certificate::uncertified("unsupported: inclined supports");
        }
        let Some(k_full) = stiff.k_full.as_ref() else {
            return Certificate::uncertified("sparse assembly did not build the full K");
        };
        let f = assemble_load_vector_sparse_3d(input, &input.loads, &dof_num, &stiff.inclined_transforms);
        return match gather_u_3d(&dof_num, results) {
            Ok(u) => certify_reduced_sparse(&stiff.k_ff, k_full, dof_num.n_free, &f, &u),
            Err(c) => c,
        };
    }
    if let Err(c) = size_check(&dof_num) {
        return c;
    }
    let stiff = assemble_stiffness_3d(input, &dof_num);
    if !stiff.inclined_transforms.is_empty() {
        return Certificate::uncertified("unsupported: inclined supports");
    }
    let f = assemble_load_vector_3d_dense(input, &input.loads, &dof_num, &stiff.inclined_transforms);
    match gather_u_3d(&dof_num, results) {
        Ok(u) => certify_reduced(&stiff.k, dof_num.n_total, dof_num.n_free, &f, &u),
        Err(c) => c,
    }
}

fn gather_u_3d(dof_num: &DofNumbering, results: &AnalysisResults3D) -> Result<Vec<f64>, Certificate> {
    let disp: HashMap<usize, &Displacement3D> = results.displacements.iter().map(|d| (d.node_id, d)).collect();
    let mut u = vec![0.0; dof_num.n_total];
    for (&(node, local), &g) in &dof_num.map {
        let Some(d) = disp.get(&node) else {
            return Err(Certificate::uncertified(format!("no displacement returned for node {node}")));
        };
        u[g] = match local {
            0 => d.ux,
            1 => d.uy,
            2 => d.uz,
            3 => d.rx,
            4 => d.ry,
            5 => d.rz,
            6 => match d.warping {
                Some(w) => w,
                None => return Err(Certificate::uncertified(format!("missing warping DOF at node {node}"))),
            },
            _ => return Err(Certificate::uncertified(format!("unexpected 3D local DOF {local}"))),
        };
    }
    Ok(u)
}

/// Reduced system from the full sparse K (lower-triangle CSC, free DOFs first):
/// K_FF ũ_F = f_F − K_FR u_R, the right-hand side enclosed in intervals.
fn certify_reduced_sparse(k_ff: &CscMatrix, k_full: &CscMatrix, nf: usize, f: &[f64], u: &[f64]) -> Certificate {
    if !u.iter().all(|x| x.is_finite()) {
        return Certificate::uncertified("non-finite displacement in solver output");
    }
    let mut f_eff: Vec<Interval> = f[..nf].iter().map(|&x| Interval::point(x)).collect();
    for j in 0..k_full.n {
        for p in k_full.col_ptr[j]..k_full.col_ptr[j + 1] {
            let i = k_full.row_idx[p];
            let v = k_full.values[p];
            // Stored (i, j) with i ≥ j: the free×restrained block has i ≥ nf > j.
            if i >= nf && j < nf && u[i] != 0.0 && v != 0.0 {
                f_eff[j] = f_eff[j] - Interval::point(v) * Interval::point(u[i]);
            }
        }
    }
    certify_sparse_spd_system(k_ff, &f_eff, &u[..nf])
}

/// Reduced system from the full dense K (n×n, free DOFs first) and full u:
/// K_FF ũ_F = f_F − K_FR u_R, the right-hand side enclosed in intervals.
fn certify_reduced(k: &[f64], n: usize, nf: usize, f: &[f64], u: &[f64]) -> Certificate {
    if !u.iter().all(|x| x.is_finite()) {
        return Certificate::uncertified("non-finite displacement in solver output");
    }
    let mut k_ff = vec![0.0; nf * nf];
    let mut f_eff = Vec::with_capacity(nf);
    for i in 0..nf {
        let row = &k[i * n..(i + 1) * n];
        k_ff[i * nf..(i + 1) * nf].copy_from_slice(&row[..nf]);
        let mut acc = Interval::point(f[i]);
        for j in nf..n {
            if u[j] != 0.0 && row[j] != 0.0 {
                acc = acc - Interval::point(row[j]) * Interval::point(u[j]);
            }
        }
        f_eff.push(acc);
    }
    certify_linear_system_auto_interval_rhs(&k_ff, nf, &f_eff, &u[..nf])
}
