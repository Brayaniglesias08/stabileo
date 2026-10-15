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

use super::forces::{force_bounds_2d, force_bounds_3d, ForceBound2D, ForceBound3D};
use super::interval::Interval;
use super::linear::{certify_sigma_auto, certify_with_certified_sigma, Certificate, MAX_CERTIFY_N};
use super::sparse::{certify_lambda_min_lower, certify_with_certified_mu};
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
    /// Per-member bounds on |force(u*) − force(ũ)| (empty when uncertified).
    pub force_bounds: Vec<ForceBound2D>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertifiedResults3D {
    pub results: AnalysisResults3D,
    pub certificate: Certificate,
    /// Per-member bounds on |force(u*) − force(ũ)| (empty when uncertified).
    pub force_bounds: Vec<ForceBound3D>,
}

/// `certify_linear_2d` plus per-member force bounds.
pub fn certify_linear_2d_full(input: &SolverInput) -> Result<CertifiedResults2D, String> {
    let (results, certificate) = certify_linear_2d(input)?;
    let force_bounds = certificate.abs_bound().map(|b| force_bounds_2d(input, b)).unwrap_or_default();
    Ok(CertifiedResults2D { results, certificate, force_bounds })
}

/// `certify_linear_3d` plus per-member force bounds.
pub fn certify_linear_3d_full(input: &SolverInput3D) -> Result<CertifiedResults3D, String> {
    let (results, certificate) = certify_linear_3d(input)?;
    let force_bounds = certificate
        .abs_bound()
        .map(|b| force_bounds_3d(&expand_curved_beams_3d(input), b))
        .unwrap_or_default();
    Ok(CertifiedResults3D { results, certificate, force_bounds })
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
    certify_load_cases_2d(input, &[(&input.loads, results)]).pop().expect("one case")
}

/// Certify several load cases that share `input`'s structure (its own `loads`
/// are ignored): case `i` is `cases[i].1`, claimed to solve the model with
/// loads `cases[i].0`. The stiffness is assembled and its coercivity certified
/// once; each case then costs one interval residual.
pub fn certify_load_cases_2d(input: &SolverInput, cases: &[(&[SolverLoad], &AnalysisResults)]) -> Vec<Certificate> {
    let all = |c: Certificate| vec![c; cases.len()];
    if !input.constraints.is_empty() {
        return all(Certificate::uncertified("unsupported: constraints (constrained solver path)"));
    }
    if input.supports.values().any(|s| s.support_type == "inclinedRoller") {
        return all(Certificate::uncertified("unsupported: inclined supports"));
    }
    let dof_num = DofNumbering::build_2d(input);
    let prepared = if dof_num.n_free >= SPARSE_THRESHOLD {
        // The solver's sparse path: certify the triplet-assembled K it factors.
        let stiff = assemble_stiffness_sparse_2d(input, &dof_num);
        if !stiff.inclined_transforms_2d.is_empty() {
            return all(Certificate::uncertified("unsupported: inclined supports"));
        }
        PreparedK::sparse(stiff.k_ff, stiff.k_full, dof_num.n_free)
    } else {
        if let Err(c) = size_check(&dof_num) {
            return all(c);
        }
        let stiff = assemble_stiffness_2d(input, &dof_num);
        if !stiff.inclined_transforms_2d.is_empty() {
            return all(Certificate::uncertified("unsupported: inclined supports"));
        }
        PreparedK::dense(stiff.k, dof_num.n_total, dof_num.n_free)
    };
    let prepared = match prepared {
        Ok(p) => p,
        Err(c) => return all(c),
    };
    cases
        .iter()
        .map(|(loads, results)| {
            let f = assemble_load_vector_2d(input, loads, &dof_num, &[]);
            match gather_u_2d(&dof_num, &results.displacements) {
                Ok(u) => prepared.certify(&f, &u),
                Err(c) => c,
            }
        })
        .collect()
}

pub(super) fn gather_u_2d(dof_num: &DofNumbering, displacements: &[Displacement]) -> Result<Vec<f64>, Certificate> {
    let disp: HashMap<usize, &Displacement> = displacements.iter().map(|d| (d.node_id, d)).collect();
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
    certify_load_cases_3d(input, &[(&input.loads, results)]).pop().expect("one case")
}

/// 3D version of `certify_load_cases_2d`.
pub fn certify_load_cases_3d(
    input: &SolverInput3D,
    cases: &[(&[SolverLoad3D], &AnalysisResults3D)],
) -> Vec<Certificate> {
    let all = |c: Certificate| vec![c; cases.len()];
    let input = &expand_curved_beams_3d(input);
    if !input.constraints.is_empty() {
        return all(Certificate::uncertified("unsupported: constraints (constrained solver path)"));
    }
    if input.supports.values().any(|s| s.is_inclined.unwrap_or(false)) {
        return all(Certificate::uncertified("unsupported: inclined supports"));
    }
    let dof_num = DofNumbering::build_3d(input);
    let sparse = dof_num.n_free >= SPARSE_THRESHOLD;
    let prepared = if sparse {
        // The solver's sparse path: certify the triplet-assembled K it factors.
        let stiff = assemble_stiffness_sparse_3d(input, &dof_num, true);
        if !stiff.inclined_transforms.is_empty() {
            return all(Certificate::uncertified("unsupported: inclined supports"));
        }
        let Some(k_full) = stiff.k_full else {
            return all(Certificate::uncertified("sparse assembly did not build the full K"));
        };
        PreparedK::sparse(stiff.k_ff, k_full, dof_num.n_free)
    } else {
        if let Err(c) = size_check(&dof_num) {
            return all(c);
        }
        let stiff = assemble_stiffness_3d(input, &dof_num);
        if !stiff.inclined_transforms.is_empty() {
            return all(Certificate::uncertified("unsupported: inclined supports"));
        }
        PreparedK::dense(stiff.k, dof_num.n_total, dof_num.n_free)
    };
    let prepared = match prepared {
        Ok(p) => p,
        Err(c) => return all(c),
    };
    cases
        .iter()
        .map(|(loads, results)| {
            let f = if sparse {
                assemble_load_vector_sparse_3d(input, loads, &dof_num, &[])
            } else {
                assemble_load_vector_3d_dense(input, loads, &dof_num, &[])
            };
            match gather_u_3d(&dof_num, &results.displacements) {
                Ok(u) => prepared.certify(&f, &u),
                Err(c) => c,
            }
        })
        .collect()
}

pub(super) fn gather_u_3d(dof_num: &DofNumbering, displacements: &[Displacement3D]) -> Result<Vec<f64>, Certificate> {
    let disp: HashMap<usize, &Displacement3D> = displacements.iter().map(|d| (d.node_id, d)).collect();
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

/// A stiffness whose coercivity has been certified once, ready to certify any
/// number of right-hand sides. Both variants hold the full K (free DOFs first)
/// so prescribed displacements can be moved to the right-hand side.
enum PreparedK {
    Dense { k: Vec<f64>, k_ff: Vec<f64>, n: usize, nf: usize, sigma: f64 },
    Sparse { k_ff: CscMatrix, k_full: CscMatrix, nf: usize, mu: f64 },
}

impl PreparedK {
    fn dense(k: Vec<f64>, n: usize, nf: usize) -> Result<Self, Certificate> {
        if k.len() != n * n || nf > n {
            return Err(Certificate::uncertified("dimension mismatch in assembled K"));
        }
        let mut k_ff = vec![0.0; nf * nf];
        for i in 0..nf {
            k_ff[i * nf..(i + 1) * nf].copy_from_slice(&k[i * n..i * n + nf]);
        }
        if !k_ff.iter().all(|x| x.is_finite()) {
            return Err(Certificate::uncertified("non-finite entry in K"));
        }
        let sigma = if nf == 0 { 0.0 } else { certify_sigma_auto(&k_ff, nf).map_err(Certificate::uncertified)? };
        Ok(PreparedK::Dense { k, k_ff, n, nf, sigma })
    }

    fn sparse(k_ff: CscMatrix, k_full: CscMatrix, nf: usize) -> Result<Self, Certificate> {
        if k_ff.n != nf {
            return Err(Certificate::uncertified("dimension mismatch in assembled K"));
        }
        if !k_ff.values.iter().all(|x| x.is_finite()) {
            return Err(Certificate::uncertified("non-finite entry in K"));
        }
        let mu = if nf == 0 {
            0.0
        } else {
            let vals: Vec<Interval> = k_ff.values.iter().map(|&x| Interval::point(x)).collect();
            certify_lambda_min_lower(&k_ff, &vals).map_err(Certificate::uncertified)?
        };
        Ok(PreparedK::Sparse { k_ff, k_full, nf, mu })
    }

    /// Certify the full displacement vector `u` (free DOFs first) against the
    /// full load vector `f`.
    fn certify(&self, f: &[f64], u: &[f64]) -> Certificate {
        if !u.iter().all(|x| x.is_finite()) {
            return Certificate::uncertified("non-finite displacement in solver output");
        }
        match self {
            PreparedK::Dense { k, k_ff, n, nf, sigma } => {
                let (n, nf) = (*n, *nf);
                if f.len() != n || u.len() != n {
                    return Certificate::uncertified("dimension mismatch in load or displacement vector");
                }
                let mut f_eff = Vec::with_capacity(nf);
                for i in 0..nf {
                    let row = &k[i * n..(i + 1) * n];
                    let mut acc = Interval::point(f[i]);
                    for j in nf..n {
                        if u[j] != 0.0 && row[j] != 0.0 {
                            acc = acc - Interval::point(row[j]) * Interval::point(u[j]);
                        }
                    }
                    f_eff.push(acc);
                }
                certify_with_certified_sigma(k_ff, nf, &f_eff, &u[..nf], *sigma)
            }
            PreparedK::Sparse { k_ff, k_full, nf, mu } => {
                let nf = *nf;
                if f.len() < nf || u.len() < k_full.n {
                    return Certificate::uncertified("dimension mismatch in load or displacement vector");
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
                certify_with_certified_mu(k_ff, &f_eff, &u[..nf], *mu)
            }
        }
    }
}
