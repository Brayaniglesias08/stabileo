//! Certified per-element bounds on member end forces, from a certified
//! displacement bound.
//!
//! In linear statics a member's local end forces are `f = k_local · T · u_e − fef`:
//! affine in its end displacements `u_e`, with `fef` independent of `u`. So the
//! forces evaluated at the exact solution `u*` and at the computed `ũ` differ, in
//! local component `i`, by `|(k_local T (u*_e − ũ_e))_i| ≤ ‖row_i(k_local)‖₂ ·
//! ‖T‖₂ · ‖u* − ũ‖₂`, and `‖T‖₂ ≤ 1 + 4ε` (an orthogonal matrix up to rounding of
//! its entries). With `‖u* − ũ‖₂ ≤ abs_bound` from the displacement certificate,
//! each bound below holds for both end values of that force component.
//!
//! The bound covers the displacement error only; evaluating the force formulas
//! in f64 adds rounding of order ε·|terms|, which is not included.
//! Elements other than 2D/3D frames and trusses (plates, shells, warping
//! frames, ...) get no bound.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::element::{frame_local_stiffness_2d, frame_local_stiffness_3d, Hinge3D};
use crate::types::*;

/// Bounds on |force(u*) − force(ũ)| for one 2D member (kN, kN·m).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForceBound2D {
    pub element_id: usize,
    pub n: f64,
    pub v: f64,
    pub m: f64,
}

/// Bounds on |force(u*) − force(ũ)| for one 3D member (local axes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForceBound3D {
    pub element_id: usize,
    pub n: f64,
    pub vy: f64,
    pub vz: f64,
    pub t: f64,
    pub my: f64,
    pub mz: f64,
}

#[inline]
fn up(x: f64) -> f64 {
    x.next_up()
}

/// ‖T‖₂ ≤ 1 + 4ε for a rotation whose entries are rounded.
const T_NORM: f64 = 1.0 + 4.0 * f64::EPSILON;

/// Upper bound on the 2-norm of row `i` of an n×n row-major matrix.
fn row_norm(k: &[f64], n: usize, i: usize) -> f64 {
    let mut s = 0.0f64;
    for &x in &k[i * n..(i + 1) * n] {
        s = up(s + up(x * x));
    }
    up(s.sqrt())
}

/// `‖row‖ · ‖T‖ · abs_bound`, rounded up, over the given rows (max).
fn bound(k: &[f64], n: usize, rows: &[usize], abs_bound: f64) -> f64 {
    let r = rows.iter().map(|&i| row_norm(k, n, i)).fold(0.0, f64::max);
    up(up(r * T_NORM) * abs_bound)
}

/// Per-member force bounds for a 2D model, given `‖u* − ũ‖₂ ≤ abs_bound`.
/// Uses the element stiffness exactly as `assemble_stiffness_2d` builds it.
pub fn force_bounds_2d(input: &SolverInput, abs_bound: f64) -> Vec<ForceBound2D> {
    let nodes: HashMap<usize, &SolverNode> = input.nodes.values().map(|n| (n.id, n)).collect();
    let mats: HashMap<usize, &SolverMaterial> = input.materials.values().map(|m| (m.id, m)).collect();
    let secs: HashMap<usize, &SolverSection> = input.sections.values().map(|s| (s.id, s)).collect();
    let mut out: Vec<ForceBound2D> = input
        .elements
        .values()
        .map(|elem| {
            let (ni, nj) = (nodes[&elem.node_i], nodes[&elem.node_j]);
            let (mat, sec) = (mats[&elem.material_id], secs[&elem.section_id]);
            let (dx, dy) = (nj.x - ni.x, nj.z - ni.z);
            let l = (dx * dx + dy * dy).sqrt();
            let e = mat.e * 1000.0;
            if elem.elem_type == "truss" || elem.elem_type == "cable" {
                // local axial stiffness EA/L·[[1, −1], [−1, 1]]: row norm √2·EA/L
                let k = [e * sec.a / l, -(e * sec.a / l)];
                return ForceBound2D { element_id: elem.id, n: bound(&k, 2, &[0], abs_bound), v: 0.0, m: 0.0 };
            }
            let phi = match sec.as_y {
                Some(as_y) => {
                    let g = e / (2.0 * (1.0 + mat.nu));
                    12.0 * e * sec.iz / (g * as_y * l * l)
                }
                None => 0.0,
            };
            let k = frame_local_stiffness_2d(e, sec.a, sec.iz, l, elem.hinge_start, elem.hinge_end, phi);
            ForceBound2D {
                element_id: elem.id,
                n: bound(&k, 6, &[0, 3], abs_bound),
                v: bound(&k, 6, &[1, 4], abs_bound),
                m: bound(&k, 6, &[2, 5], abs_bound),
            }
        })
        .collect();
    out.sort_by_key(|b| b.element_id);
    out
}

/// Per-member force bounds for a 3D model (frames and trusses), given
/// `‖u* − ũ‖₂ ≤ abs_bound`. Uses the element stiffness exactly as
/// `assemble_stiffness_3d` builds it; warping frames are skipped.
pub fn force_bounds_3d(input: &SolverInput3D, abs_bound: f64) -> Vec<ForceBound3D> {
    let nodes: HashMap<usize, &SolverNode3D> = input.nodes.values().map(|n| (n.id, n)).collect();
    let mats: HashMap<usize, &SolverMaterial> = input.materials.values().map(|m| (m.id, m)).collect();
    let secs: HashMap<usize, &SolverSection3D> = input.sections.values().map(|s| (s.id, s)).collect();
    let mut out: Vec<ForceBound3D> = input
        .elements
        .values()
        .filter_map(|elem| {
            let (ni, nj) = (nodes[&elem.node_i], nodes[&elem.node_j]);
            let (mat, sec) = (mats[&elem.material_id], secs[&elem.section_id]);
            let (dx, dy, dz) = (nj.x - ni.x, nj.y - ni.y, nj.z - ni.z);
            let l = (dx * dx + dy * dy + dz * dz).sqrt();
            let e = mat.e * 1000.0;
            let g = e / (2.0 * (1.0 + mat.nu));
            if elem.elem_type == "truss" || elem.elem_type == "cable" {
                let k = [e * sec.a / l, -(e * sec.a / l)];
                let n = bound(&k, 2, &[0], abs_bound);
                return Some(ForceBound3D { element_id: elem.id, n, vy: 0.0, vz: 0.0, t: 0.0, my: 0.0, mz: 0.0 });
            }
            if sec.cw.is_some_and(|cw| cw > 0.0) {
                return None;
            }
            let (phi_y, phi_z) = if sec.as_y.is_some() || sec.as_z.is_some() {
                let l2 = l * l;
                (
                    sec.as_y.map(|ay| 12.0 * e * sec.iy / (g * ay * l2)).unwrap_or(0.0),
                    sec.as_z.map(|az| 12.0 * e * sec.iz / (g * az * l2)).unwrap_or(0.0),
                )
            } else {
                (0.0, 0.0)
            };
            let k = frame_local_stiffness_3d(
                e, sec.a, sec.iy, sec.iz, sec.j, l, g, Hinge3D::from_elem(elem), phi_y, phi_z,
            );
            Some(ForceBound3D {
                element_id: elem.id,
                n: bound(&k, 12, &[0, 6], abs_bound),
                vy: bound(&k, 12, &[1, 7], abs_bound),
                vz: bound(&k, 12, &[2, 8], abs_bound),
                t: bound(&k, 12, &[3, 9], abs_bound),
                my: bound(&k, 12, &[4, 10], abs_bound),
                mz: bound(&k, 12, &[5, 11], abs_bound),
            })
        })
        .collect();
    out.sort_by_key(|b| b.element_id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::dof::DofNumbering;
    use crate::solver::linear::{compute_internal_forces_2d, compute_internal_forces_3d, solve_2d, solve_3d};

    /// Deterministic pseudo-random direction in the free DOFs, scaled to norm `b`.
    fn perturbation(n_total: usize, nf: usize, b: f64, seed: usize) -> Vec<f64> {
        let mut d: Vec<f64> = (0..n_total)
            .map(|i| if i < nf { (((i + seed) * 7919) % 211) as f64 / 105.5 - 1.0 } else { 0.0 })
            .collect();
        let nrm = d.iter().map(|x| x * x).sum::<f64>().sqrt();
        d.iter_mut().for_each(|x| *x *= b / nrm);
        d
    }

    #[test]
    fn force_bounds_2d_cover_displacement_perturbations() {
        let input: SolverInput =
            serde_json::from_str(include_str!("../../tests/fixtures/ex-building-3story-dlw-case1-input.json")).unwrap();
        let res = solve_2d(&input).unwrap();
        let dof = DofNumbering::build_2d(&input);
        let u = super::super::solver::gather_u_2d(&dof, &res.displacements).unwrap();
        let b = 1e-3;
        let bounds: HashMap<usize, ForceBound2D> =
            force_bounds_2d(&input, b).into_iter().map(|f| (f.element_id, f)).collect();
        let f0 = compute_internal_forces_2d(&input, &dof, &u);
        let mut worst = 0.0f64;
        for seed in 0..20 {
            let d = perturbation(dof.n_total, dof.n_free, b, seed);
            let u1: Vec<f64> = u.iter().zip(&d).map(|(a, b)| a + b).collect();
            for (a, c) in f0.iter().zip(compute_internal_forces_2d(&input, &dof, &u1)) {
                let fb = &bounds[&a.element_id];
                let slack = 1e-9 * (1.0 + fb.n + fb.v + fb.m); // f64 evaluation rounding
                assert!((a.n_start - c.n_start).abs() <= fb.n + slack, "N {} {fb:?}", a.element_id);
                assert!((a.n_end - c.n_end).abs() <= fb.n + slack, "N {}", a.element_id);
                assert!((a.v_start - c.v_start).abs() <= fb.v + slack, "V {}", a.element_id);
                assert!((a.v_end - c.v_end).abs() <= fb.v + slack, "V {}", a.element_id);
                assert!((a.m_start - c.m_start).abs() <= fb.m + slack, "M {}", a.element_id);
                assert!((a.m_end - c.m_end).abs() <= fb.m + slack, "M {}", a.element_id);
                worst = worst.max((a.m_start - c.m_start).abs() / fb.m).max((a.n_start - c.n_start).abs() / fb.n);
            }
        }
        // Not vacuous: random perturbations reach a real fraction of the bound.
        assert!(worst > 0.05, "bounds look vacuous: max ratio {worst}");
    }

    #[test]
    fn force_bounds_3d_cover_displacement_perturbations() {
        let input: SolverInput3D =
            serde_json::from_str(include_str!("../../tests/fixtures/ex-3d-portal-frame-input.json")).unwrap();
        let res = solve_3d(&input).unwrap();
        let dof = DofNumbering::build_3d(&input);
        let u = super::super::solver::gather_u_3d(&dof, &res.displacements).unwrap();
        let b = 1e-4;
        let bounds: HashMap<usize, ForceBound3D> =
            force_bounds_3d(&input, b).into_iter().map(|f| (f.element_id, f)).collect();
        let f0 = compute_internal_forces_3d(&input, &dof, &u);
        let mut worst = 0.0f64;
        for seed in 0..20 {
            let d = perturbation(dof.n_total, dof.n_free, b, seed);
            let u1: Vec<f64> = u.iter().zip(&d).map(|(a, b)| a + b).collect();
            for (a, c) in f0.iter().zip(compute_internal_forces_3d(&input, &dof, &u1)) {
                let Some(fb) = bounds.get(&a.element_id) else { continue };
                let s = 1e-9 * (1.0 + fb.n + fb.vy + fb.vz + fb.t + fb.my + fb.mz);
                let pairs = [
                    (a.n_start - c.n_start, fb.n), (a.n_end - c.n_end, fb.n),
                    (a.vy_start - c.vy_start, fb.vy), (a.vy_end - c.vy_end, fb.vy),
                    (a.vz_start - c.vz_start, fb.vz), (a.vz_end - c.vz_end, fb.vz),
                    (a.mx_start - c.mx_start, fb.t), (a.mx_end - c.mx_end, fb.t),
                    (a.my_start - c.my_start, fb.my), (a.my_end - c.my_end, fb.my),
                    (a.mz_start - c.mz_start, fb.mz), (a.mz_end - c.mz_end, fb.mz),
                ];
                for (k, (diff, bnd)) in pairs.iter().enumerate() {
                    assert!(diff.abs() <= bnd + s, "elem {} component {k}: {diff} > {bnd}", a.element_id);
                    if *bnd > 0.0 {
                        worst = worst.max(diff.abs() / bnd);
                    }
                }
            }
        }
        assert!(worst > 0.05, "bounds look vacuous: max ratio {worst}");
    }
}
