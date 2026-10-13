//! Certified enclosure of the critical load factor of linear buckling,
//! `K φ = λ N φ` with `N = −K_G` (the matrices `solve_buckling_{2d,3d}` build).
//!
//! The stability matrix `A(λ) = K − λ N` is affine in λ. The critical load factor
//! is where it stops being positive definite:
//! `λ_cr = sup { λ ≥ 0 : A(λ') ≻ 0 for all 0 ≤ λ' ≤ λ }`.
//!
//! * Lower bound: if `A(0) ≻ 0` and `A(λ̲) ≻ 0` (both certified by the sparse Gram
//!   residual + Gershgorin check, `sparse.rs`), then every `A(λ)` on `[0, λ̲]` is a
//!   convex combination of the two and is positive definite, so `λ_cr ≥ λ̲`.
//! * Upper bound: for the computed mode φ, `φᵀ A(λ̄) φ = φᵀKφ − λ̄ φᵀNφ ≤ 0` with
//!   `λ̄ = φᵀKφ / φᵀNφ` (interval-evaluated, rounded up), so `A(λ̄)` is not positive
//!   definite and `λ_cr ≤ λ̄`.
//!
//! The certificate is about the f64 `K` and `K_G` as assembled (with `K_G` from
//! the computed linear axial forces), not about rounding-free versions of them.
//! Constraints are not supported (the solver reduces both matrices first).

use serde::{Deserialize, Serialize};

use super::interval::Interval;
use super::solver::{gather_u_2d, gather_u_3d};
use super::sparse::certify_lambda_min_lower;
use crate::linalg::sparse::CscMatrix;
use crate::solver::buckling::{
    buckling_system_2d, buckling_system_3d, solve_buckling_2d, solve_buckling_3d, BucklingResult,
    BucklingResult3D,
};
use crate::types::*;

/// Outcome of certifying the critical load factor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum BucklingCertificate {
    Certified {
        /// The structure is certified stable (K − λN ≻ 0) for all 0 ≤ λ ≤ lambda_lower.
        lambda_lower: f64,
        /// K − λN is certified not positive definite at λ = lambda_upper
        /// (None when the computed mode gave no usable Rayleigh quotient).
        lambda_upper: Option<f64>,
        /// The solver's computed critical load factor.
        lambda_computed: f64,
    },
    Uncertified { reason: String },
}

impl BucklingCertificate {
    fn uncertified(reason: impl Into<String>) -> Self {
        BucklingCertificate::Uncertified { reason: reason.into() }
    }

    pub fn is_certified(&self) -> bool {
        matches!(self, BucklingCertificate::Certified { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertifiedBuckling2D {
    pub results: BucklingResult,
    pub certificate: BucklingCertificate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertifiedBuckling3D {
    pub results: BucklingResult3D,
    pub certificate: BucklingCertificate,
}

/// Solve 2D linear buckling and certify the first critical load factor.
pub fn certify_buckling_2d(input: &SolverInput, num_modes: usize) -> Result<CertifiedBuckling2D, String> {
    let results = solve_buckling_2d(input, num_modes.max(1))?;
    let certificate = if !input.constraints.is_empty() {
        BucklingCertificate::uncertified("unsupported: constraints")
    } else {
        let sys = buckling_system_2d(input)?;
        let mode = &results.modes[0];
        match gather_u_2d(&sys.dof_num, &mode.displacements) {
            Ok(phi) => certify_critical_factor(
                &sys.k_ff,
                &sys.neg_kg_csc,
                mode.load_factor,
                &phi[..sys.dof_num.n_free],
            ),
            Err(c) => BucklingCertificate::uncertified(format!("mode shape: {c:?}")),
        }
    };
    Ok(CertifiedBuckling2D { results, certificate })
}

/// Solve 3D linear buckling and certify the first critical load factor.
pub fn certify_buckling_3d(input: &SolverInput3D, num_modes: usize) -> Result<CertifiedBuckling3D, String> {
    let results = solve_buckling_3d(input, num_modes.max(1))?;
    let certificate = if !input.constraints.is_empty() {
        BucklingCertificate::uncertified("unsupported: constraints")
    } else {
        let sys = buckling_system_3d(input)?;
        let mode = &results.modes[0];
        match gather_u_3d(&sys.dof_num, &mode.displacements) {
            Ok(phi) => certify_critical_factor(
                &sys.k_ff,
                &sys.neg_kg_csc,
                mode.load_factor,
                &phi[..sys.dof_num.n_free],
            ),
            Err(c) => BucklingCertificate::uncertified(format!("mode shape: {c:?}")),
        }
    };
    Ok(CertifiedBuckling3D { results, certificate })
}

/// Enclose the critical load factor of `K φ = λ N φ` (both symmetric, lower
/// triangle CSC of the same dimension) given the computed `lambda_hat` and
/// mode `phi`.
pub fn certify_critical_factor(k: &CscMatrix, n_mat: &CscMatrix, lambda_hat: f64, phi: &[f64]) -> BucklingCertificate {
    if k.n != n_mat.n || phi.len() != k.n {
        return BucklingCertificate::uncertified("dimension mismatch");
    }
    if !(lambda_hat > 0.0 && lambda_hat.is_finite()) {
        return BucklingCertificate::uncertified(format!("computed load factor not positive: {lambda_hat}"));
    }
    if !phi.iter().all(|x| x.is_finite()) {
        return BucklingCertificate::uncertified("non-finite mode shape");
    }
    let Some(pattern) = union_pattern(k, n_mat) else {
        return BucklingCertificate::uncertified("unsorted CSC column");
    };

    // A(0) = K ≻ 0: needed for the lower bound and for the upper bound's meaning.
    let (a0, v0) = stability_matrix(k, n_mat, &pattern, 0.0);
    if let Err(reason) = certify_lambda_min_lower(&a0, &v0) {
        return BucklingCertificate::uncertified(format!("K not certified positive definite: {reason}"));
    }

    // Tightest λ̲ = λ̂(1 − δ) that certifies.
    let mut lambda_lower = None;
    let mut last = String::new();
    for delta in [1e-6, 1e-4, 1e-3, 1e-2, 0.05, 0.2, 0.5] {
        let lam = lambda_hat * (1.0 - delta);
        let (a, v) = stability_matrix(k, n_mat, &pattern, lam);
        match certify_lambda_min_lower(&a, &v) {
            Ok(_) => {
                lambda_lower = Some(lam);
                break;
            }
            Err(reason) => last = reason,
        }
    }
    let Some(lambda_lower) = lambda_lower else {
        return BucklingCertificate::uncertified(format!(
            "could not certify K − λN ≻ 0 even at λ = λ̂/2 (λ̂ = {lambda_hat:.6e}): {last}"
        ));
    };

    BucklingCertificate::Certified {
        lambda_lower,
        lambda_upper: rayleigh_upper(k, n_mat, phi),
        lambda_computed: lambda_hat,
    }
}

/// `λ̄ = φᵀKφ / φᵀNφ` rounded up, if φᵀNφ is certified positive.
fn rayleigh_upper(k: &CscMatrix, n_mat: &CscMatrix, phi: &[f64]) -> Option<f64> {
    let num = quad_form(k, phi)?;
    let den = quad_form(n_mat, phi)?;
    if !(den.lo > 0.0) {
        return None;
    }
    let ub = (num.hi / den.lo).next_up();
    (ub.is_finite() && ub > 0.0).then_some(ub)
}

/// Interval enclosure of φᵀAφ for symmetric A stored as its lower triangle.
fn quad_form(a: &CscMatrix, phi: &[f64]) -> Option<Interval> {
    let mut acc = Interval::ZERO;
    for j in 0..a.n {
        for p in a.col_ptr[j]..a.col_ptr[j + 1] {
            let i = a.row_idx[p];
            let t = Interval::point(a.values[p]) * Interval::point(phi[i]) * Interval::point(phi[j]);
            acc = if i == j { acc + t } else { acc + t + t };
        }
    }
    acc.is_finite().then_some(acc)
}

/// Lower-triangle union pattern of K and N: per column, sorted
/// (row, index into K or None, index into N or None).
type Pattern = Vec<Vec<(usize, Option<usize>, Option<usize>)>>;

fn union_pattern(k: &CscMatrix, n_mat: &CscMatrix) -> Option<Pattern> {
    let sorted = |m: &CscMatrix| {
        (0..m.n).all(|j| m.row_idx[m.col_ptr[j]..m.col_ptr[j + 1]].windows(2).all(|w| w[0] < w[1]))
    };
    if !sorted(k) || !sorted(n_mat) {
        return None;
    }
    let mut cols = Vec::with_capacity(k.n);
    for j in 0..k.n {
        let (mut p, pe) = (k.col_ptr[j], k.col_ptr[j + 1]);
        let (mut q, qe) = (n_mat.col_ptr[j], n_mat.col_ptr[j + 1]);
        let mut col = Vec::with_capacity(pe - p + qe - q);
        while p < pe || q < qe {
            let rk = if p < pe { k.row_idx[p] } else { usize::MAX };
            let rn = if q < qe { n_mat.row_idx[q] } else { usize::MAX };
            if rk == rn {
                col.push((rk, Some(p), Some(q)));
                p += 1;
                q += 1;
            } else if rk < rn {
                col.push((rk, Some(p), None));
                p += 1;
            } else {
                col.push((rn, None, Some(q)));
                q += 1;
            }
        }
        cols.push(col);
    }
    Some(cols)
}

/// `A = K − λN` on the union pattern: f64 approximation (for factoring) and
/// exact-entry enclosures.
fn stability_matrix(k: &CscMatrix, n_mat: &CscMatrix, pattern: &Pattern, lam: f64) -> (CscMatrix, Vec<Interval>) {
    let n = k.n;
    let mut col_ptr = Vec::with_capacity(n + 1);
    let mut row_idx = Vec::new();
    let mut values = Vec::new();
    let mut vals = Vec::new();
    col_ptr.push(0);
    for col in pattern {
        for &(r, pk, pn) in col {
            let kv = pk.map_or(0.0, |p| k.values[p]);
            let nv = pn.map_or(0.0, |q| n_mat.values[q]);
            row_idx.push(r);
            values.push(kv - lam * nv);
            vals.push(Interval::point(kv) - Interval::point(lam) * Interval::point(nv));
        }
        col_ptr.push(row_idx.len());
    }
    (CscMatrix { n, col_ptr, row_idx, values }, vals)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag(d: &[f64]) -> CscMatrix {
        let idx: Vec<usize> = (0..d.len()).collect();
        CscMatrix::from_triplets(d.len(), &idx, &idx, d)
    }

    #[test]
    fn diagonal_pencil_encloses_smallest_ratio() {
        // λ_i = k_i / n_i = [4, 2, 10] → λ_cr = 2
        let k = diag(&[4.0, 6.0, 10.0]);
        let n = diag(&[1.0, 3.0, 1.0]);
        let phi = [0.0, 1.0, 0.0];
        let c = certify_critical_factor(&k, &n, 2.0, &phi);
        let BucklingCertificate::Certified { lambda_lower, lambda_upper, .. } = c else { panic!("{c:?}") };
        let ub = lambda_upper.unwrap();
        assert!(lambda_lower <= 2.0 && 2.0 <= ub, "[{lambda_lower}, {ub}]");
        assert!(lambda_lower > 1.99 && ub < 2.0 + 1e-12, "[{lambda_lower}, {ub}]");
    }

    #[test]
    fn overestimated_factor_is_not_certified_as_lower_bound() {
        // A wrong λ̂ = 3 (true λ_cr = 2): the lower bound must stay ≤ 2.
        let k = diag(&[4.0, 6.0, 10.0]);
        let n = diag(&[1.0, 3.0, 1.0]);
        let c = certify_critical_factor(&k, &n, 3.0, &[0.0, 1.0, 0.0]);
        if let BucklingCertificate::Certified { lambda_lower, .. } = c {
            assert!(lambda_lower <= 2.0, "λ̲ = {lambda_lower}");
        }
    }
}
