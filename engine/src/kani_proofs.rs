//! Kani bounded-model-checking harnesses. Run with `cargo kani`.
//!
//! See `docs/verification/KANI.md` for what each harness proves, under which
//! input assumptions, and the counterexamples Kani found.
//!
//! Conventions:
//! - Float inputs come from `kani::any()` constrained by `kani::assume` to
//!   physically meaningful, finite ranges (a range check also excludes NaN).
//! - Every harness implicitly proves absence of panics, out-of-bounds indexing
//!   and integer (usize) overflow on all paths reachable under the assumptions;
//!   Kani adds those checks automatically.
//! - "Bitwise" equalities compare `f64::to_bits`, i.e. exact IEEE-754 equality
//!   (stronger than `==`, which conflates +0.0/-0.0).
//! - Harnesses marked `#[kani::should_panic]` encode a property that does NOT
//!   hold; they pass only because Kani finds a counterexample. They document
//!   findings and guard against silently "fixing" them in the docs.

use crate::element::*;
use crate::linalg::dense;

// ─── helpers ────────────────────────────────────────────────────────────────

/// Symbolic f64 in the closed interval [lo, hi] (hence finite, not NaN).
fn any_in(lo: f64, hi: f64) -> f64 {
    let x: f64 = kani::any();
    kani::assume(x >= lo && x <= hi);
    x
}

/// Symbolic f64 with |x| in [lo, hi], either sign.
fn any_mag(lo: f64, hi: f64) -> f64 {
    let x: f64 = kani::any();
    kani::assume(x.abs() >= lo && x.abs() <= hi);
    x
}

/// Symbolic f64 in [-hi, hi] (zero allowed).
fn any_sym(hi: f64) -> f64 {
    any_in(-hi, hi)
}

// Physical ranges (SI-ish: kN, m).
fn any_len() -> f64 { any_in(1e-3, 1e3) }
fn any_e() -> f64 { any_in(1e3, 1e12) }
fn any_area() -> f64 { any_in(1e-8, 10.0) }
fn any_inertia() -> f64 { any_in(1e-12, 10.0) }
fn any_phi() -> f64 { any_in(0.0, 100.0) }

fn all_finite(v: &[f64]) -> bool {
    v.iter().all(|x| x.is_finite())
}

fn bitwise_symmetric(k: &[f64], n: usize) -> bool {
    let mut ok = true;
    for i in 0..n {
        for j in (i + 1)..n {
            ok &= k[i * n + j].to_bits() == k[j * n + i].to_bits();
        }
    }
    ok
}

fn diag_nonneg(k: &[f64], n: usize) -> bool {
    (0..n).all(|i| k[i * n + i] >= 0.0)
}

// ─── truss.rs ───────────────────────────────────────────────────────────────

#[kani::proof]
fn truss_local_stiffness_2d_ok() {
    let (e, a, l) = (any_e(), any_area(), any_len());
    let k = truss_local_stiffness_2d(e, a, l);
    assert!(k.len() == 16);
    assert!(all_finite(&k));
    assert!(bitwise_symmetric(&k, 4));
    assert!(k[0] > 0.0 && k[2 * 4 + 2] > 0.0);
    assert!(diag_nonneg(&k, 4));
    // Row sums are exactly zero (rigid translation produces no force).
    assert!(k[0] + k[2] == 0.0);
}

#[kani::proof]
fn truss_local_stiffness_3d_ok() {
    let (e, a, l) = (any_e(), any_area(), any_len());
    let k = truss_local_stiffness_3d(e, a, l);
    assert!(k.len() == 36);
    assert!(all_finite(&k));
    assert!(bitwise_symmetric(&k, 6));
    assert!(k[0] > 0.0 && k[3 * 6 + 3] > 0.0);
    assert!(diag_nonneg(&k, 6));
}

#[kani::proof]
fn truss_global_stiffness_2d_ok() {
    let (e, a, l) = (any_e(), any_area(), any_len());
    let c = any_sym(1.0);
    let s = any_sym(1.0);
    let k = truss_global_stiffness_2d(e, a, l, c, s);
    assert!(k.len() == 16);
    assert!(all_finite(&k));
    assert!(bitwise_symmetric(&k, 4));
    assert!(diag_nonneg(&k, 4));
}

/// Finiteness, non-negative diagonal and the exact block structure
/// K = [[B, -B], [-B, B]] of the direct 3D truss global stiffness.
#[kani::proof]
fn truss_global_stiffness_3d_finite_and_blocks() {
    let (e, a, l) = (any_e(), any_area(), any_len());
    // Direction components with |d| <= L (they are projections of L).
    let dx = any_in(-l, l);
    let dy = any_in(-l, l);
    let dz = any_in(-l, l);
    let k = truss_global_stiffness_3d(e, a, l, dx, dy, dz);
    assert!(k.len() == 36);
    assert!(all_finite(&k));
    assert!(diag_nonneg(&k, 6));
    for i in 0..3 {
        for j in 0..3 {
            let b = k[i * 6 + j];
            assert!(k[i * 6 + j + 3] == -b);
            assert!(k[(i + 3) * 6 + j] == -b);
            assert!(k[(i + 3) * 6 + j + 3].to_bits() == b.to_bits());
        }
    }
}

/// FINDING: the direct 3D truss global stiffness is NOT bitwise symmetric.
/// k[i][j] = (ea_l*dir[i])*dir[j] and k[j][i] = (ea_l*dir[j])*dir[i] round
/// differently. This harness asserts symmetry and is expected to FAIL.
#[kani::proof]
#[kani::should_panic]
fn truss_global_stiffness_3d_symmetry_counterexample() {
    let (e, a, l) = (any_e(), any_area(), any_len());
    let dx = any_in(-l, l);
    let dy = any_in(-l, l);
    let k = truss_global_stiffness_3d(e, a, l, dx, dy, 0.0);
    assert!(k[0 * 6 + 1].to_bits() == k[1 * 6 + 0].to_bits());
}

// ─── frame.rs ───────────────────────────────────────────────────────────────

#[kani::proof]
fn frame_local_stiffness_2d_ok() {
    let (e, a, iz, l) = (any_e(), any_area(), any_inertia(), any_len());
    let phi = any_phi();
    let hs: bool = kani::any();
    let he: bool = kani::any();
    let k = frame_local_stiffness_2d(e, a, iz, l, hs, he, phi);
    assert!(k.len() == 36);
    assert!(all_finite(&k));
    assert!(bitwise_symmetric(&k, 6));
    assert!(diag_nonneg(&k, 6));
    // Axial diagonal always strictly positive.
    assert!(k[0] > 0.0 && k[3 * 6 + 3] > 0.0);
    // Transverse translational stiffness strictly positive unless both ends hinged.
    if !(hs && he) {
        assert!(k[1 * 6 + 1] > 0.0 && k[4 * 6 + 4] > 0.0);
    }
    // Released rotations have exactly zero row/column.
    if hs {
        for j in 0..6 {
            assert!(k[2 * 6 + j] == 0.0 && k[j * 6 + 2] == 0.0);
        }
    } else {
        assert!(k[2 * 6 + 2] > 0.0);
    }
    if he {
        for j in 0..6 {
            assert!(k[5 * 6 + j] == 0.0 && k[j * 6 + 5] == 0.0);
        }
    } else {
        assert!(k[5 * 6 + 5] > 0.0);
    }
}

fn any_hinge3d() -> Hinge3D {
    Hinge3D {
        release_my_start: kani::any(),
        release_mz_start: kani::any(),
        release_t_start: kani::any(),
        release_my_end: kani::any(),
        release_mz_end: kani::any(),
        release_t_end: kani::any(),
    }
}

// The 3D frame stiffness is verified compositionally. CBMC bit-blasts every f64
// operation, so the monolithic harness (10 symbolic floats x 64 hinge
// combinations) needed > 80 GB. Instead:
//   1. `bending_block_*` prove each release case of `add_bending_block` alone:
//      finite, bitwise symmetric, non-negative diagonal, positive v1 diagonal.
//   2. `frame_local_stiffness_3d_ok` stubs `add_bending_block` with
//      `add_bending_block_stub`, which may write any finite symmetric values with
//      a non-negative diagonal to any subset of the symmetric pairs of its four
//      DOFs. By (1) that over-approximates the real block, so the assembly
//      properties proven with the stub hold for the real function.
//   3. `frame_local_stiffness_3d_warping_ok` likewise stubs the 12x12 matrix with
//      `frame_local_stiffness_3d_stub` (justified by (2)) and checks the
//      torsion-warping block it adds.

fn bending_block_case(release_start: bool, release_end: bool) {
    let e = any_e();
    let i = any_inertia();
    let l = any_len();
    let phi = any_phi();
    let s = if kani::any() { 1.0 } else { -1.0 };
    let mut k = [0.0f64; 16];
    add_bending_block(&mut k, 4, 0, 1, 2, 3, e, i, l, l * l, l * l * l, phi, s, release_start, release_end);
    assert!(all_finite(&k));
    assert!(bitwise_symmetric(&k, 4));
    assert!(diag_nonneg(&k, 4));
    if !(release_start && release_end) {
        assert!(k[0] > 0.0 && k[2 * 4 + 2] > 0.0);
    }
}

#[kani::proof]
fn bending_block_fixed_ok() { bending_block_case(false, false) }

#[kani::proof]
fn bending_block_release_start_ok() { bending_block_case(true, false) }

#[kani::proof]
fn bending_block_release_end_ok() { bending_block_case(false, true) }

#[kani::proof]
fn bending_block_released_both_ok() { bending_block_case(true, true) }

/// Over-approximation of `add_bending_block` (see the note above).
#[allow(clippy::too_many_arguments)]
fn add_bending_block_stub(
    k: &mut [f64], n: usize,
    v1: usize, r1: usize, v2: usize, r2: usize,
    _e: f64, _i: f64, _l: f64, _l2: f64, _l3: f64, _phi: f64, _s: f64,
    _release_start: bool, _release_end: bool,
) {
    let idx = [v1, r1, v2, r2];
    for p in 0..4 {
        for q in p..4 {
            if kani::any() {
                let x: f64 = kani::any();
                kani::assume(x.is_finite() && (p != q || x >= 0.0));
                k[idx[p] * n + idx[q]] = x;
                k[idx[q] * n + idx[p]] = x;
            }
        }
    }
}

#[kani::proof]
#[kani::stub(add_bending_block, add_bending_block_stub)]
fn frame_local_stiffness_3d_ok() {
    let e = any_e();
    let a = any_area();
    let iy = any_inertia();
    let iz = any_inertia();
    let j = any_inertia();
    let l = any_len();
    let g = any_in(1e2, 1e12);
    let phi_y = any_phi();
    let phi_z = any_phi();
    let h = any_hinge3d();
    let k = frame_local_stiffness_3d(e, a, iy, iz, j, l, g, h, phi_y, phi_z);
    assert!(k.len() == 144);
    assert!(all_finite(&k));
    assert!(bitwise_symmetric(&k, 12));
    assert!(diag_nonneg(&k, 12));
    assert!(k[0] > 0.0 && k[6 * 12 + 6] > 0.0);
    if !h.release_t_start && !h.release_t_end {
        assert!(k[3 * 12 + 3] > 0.0 && k[9 * 12 + 9] > 0.0);
    }
}

/// Over-approximation of `frame_local_stiffness_3d`, justified by
/// `frame_local_stiffness_3d_ok`: any finite symmetric 12x12 with a
/// non-negative diagonal.
#[allow(clippy::too_many_arguments)]
fn frame_local_stiffness_3d_stub(
    _e: f64, _a: f64, _iy: f64, _iz: f64, _j: f64, _l: f64, _g: f64,
    _hinge: Hinge3D, _phi_y: f64, _phi_z: f64,
) -> Vec<f64> {
    let mut k = vec![0.0; 144];
    for p in 0..12 {
        for q in p..12 {
            let x: f64 = kani::any();
            kani::assume(x.is_finite() && (p != q || x >= 0.0));
            k[p * 12 + q] = x;
            k[q * 12 + p] = x;
        }
    }
    k
}

#[kani::proof]
#[kani::stub(frame_local_stiffness_3d, frame_local_stiffness_3d_stub)]
fn frame_local_stiffness_3d_warping_ok() {
    let e = any_e();
    let a = any_area();
    let iy = any_inertia();
    let iz = any_inertia();
    let j = any_inertia();
    let cw = any_in(0.0, 1.0);
    let l = any_len();
    let g = any_in(1e2, 1e12);
    let phi_y = any_phi();
    let phi_z = any_phi();
    let h = any_hinge3d();
    let k = frame_local_stiffness_3d_warping(e, a, iy, iz, j, cw, l, g, h, phi_y, phi_z);
    assert!(k.len() == 196);
    assert!(all_finite(&k));
    assert!(bitwise_symmetric(&k, 14));
    assert!(diag_nonneg(&k, 14));
}

// ─── transform.rs ───────────────────────────────────────────────────────────

#[kani::proof]
fn frame_transform_2d_structure() {
    let c = any_sym(1.0);
    let s = any_sym(1.0);
    let t = frame_transform_2d(c, s);
    assert!(t.len() == 36);
    assert!(all_finite(&t));
    // Exact block structure: two identical rotation blocks + unit rotation DOFs.
    for b in [0usize, 3] {
        assert!(t[b * 6 + b] == c && t[b * 6 + b + 1] == s);
        assert!(t[(b + 1) * 6 + b] == -s && t[(b + 1) * 6 + b + 1] == c);
        assert!(t[(b + 2) * 6 + b + 2] == 1.0);
    }
}

#[kani::proof]
fn truss_transform_2d_structure() {
    let c = any_sym(1.0);
    let s = any_sym(1.0);
    let t = truss_transform_2d(c, s);
    assert!(t.len() == 16);
    assert!(all_finite(&t));
    assert!(t[0] == t[2 * 4 + 2] && t[1] == t[2 * 4 + 3]);
    assert!(t[4] == -s && t[3 * 4 + 2] == -s);
}

#[kani::proof]
fn frame_transform_3d_structure() {
    let ex = [any_sym(1.0), any_sym(1.0), any_sym(1.0)];
    let ey = [any_sym(1.0), any_sym(1.0), any_sym(1.0)];
    let ez = [any_sym(1.0), any_sym(1.0), any_sym(1.0)];
    let t = frame_transform_3d(&ex, &ey, &ez);
    assert!(t.len() == 144);
    assert!(all_finite(&t));
    let r: usize = kani::any();
    let c: usize = kani::any();
    kani::assume(r < 12 && c < 12);
    let v = t[r * 12 + c];
    if r / 3 != c / 3 {
        assert!(v == 0.0);
    } else {
        let row = [ex, ey, ez][r % 3];
        assert!(v == row[c % 3]);
    }
}

#[kani::proof]
fn frame_transform_3d_warping_structure() {
    let ex = [any_sym(1.0), any_sym(1.0), any_sym(1.0)];
    let ey = [any_sym(1.0), any_sym(1.0), any_sym(1.0)];
    let ez = [any_sym(1.0), any_sym(1.0), any_sym(1.0)];
    let t = frame_transform_3d_warping(&ex, &ey, &ez);
    assert!(t.len() == 196);
    assert!(all_finite(&t));
    assert!(t[6 * 14 + 6] == 1.0 && t[13 * 14 + 13] == 1.0);
    let r: usize = kani::any();
    let c: usize = kani::any();
    kani::assume(r < 14 && c < 14);
    // Warping rows/cols are identity.
    if (r == 6 || r == 13 || c == 6 || c == 13) && r != c {
        assert!(t[r * 14 + c] == 0.0);
    }
}

/// compute_local_axes_3d with the default (Z-up) convention, no explicit local
/// Y, no roll: no panic and all outputs finite for members with L >= 1e-3.
#[kani::proof]
fn compute_local_axes_3d_default_finite() {
    let dx = any_sym(1e3);
    let dy = any_sym(1e3);
    let dz = any_sym(1e3);
    kani::assume(dx * dx + dy * dy + dz * dz >= 1e-6);
    let lh: bool = kani::any();
    let (ex, ey, ez) =
        compute_local_axes_3d(0.0, 0.0, 0.0, dx, dy, dz, None, None, None, None, lh);
    assert!(all_finite(&ex) && all_finite(&ey) && all_finite(&ez));
}

// ─── fef.rs ─────────────────────────────────────────────────────────────────

fn any_q() -> f64 { any_sym(1e6) }

#[kani::proof]
fn fef_distributed_2d_ok() {
    let (qi, qj, l) = (any_q(), any_q(), any_len());
    let f = fef_distributed_2d(qi, qj, l);
    assert!(all_finite(&f));
    assert!(f[0] == 0.0 && f[3] == 0.0);
}

#[kani::proof]
fn fef_point_load_2d_ok() {
    let l = any_len();
    let a = any_in(0.0, l);
    let (p, px, m) = (any_q(), any_q(), any_q());
    let f = fef_point_load_2d(p, px, m, a, l);
    assert!(all_finite(&f));
}

#[kani::proof]
fn fef_thermal_2d_ok() {
    let (e, a, iz, l) = (any_e(), any_area(), any_inertia(), any_len());
    let dtu = any_sym(1e3);
    let dtg = any_sym(1e3);
    let alpha = any_in(0.0, 1e-4);
    let h = any_in(0.0, 10.0);
    let f = fef_thermal_2d(e, a, iz, l, dtu, dtg, alpha, h);
    assert!(all_finite(&f));
    // Self-equilibrated: exact negation between ends.
    assert!(f[0] == -f[3] && f[2] == -f[5]);
}

#[kani::proof]
fn adjust_fef_for_hinges_ok() {
    let l = any_len();
    let phi = any_phi();
    let mut f = [0.0, any_q(), any_q(), 0.0, any_q(), any_q()];
    let hs: bool = kani::any();
    let he: bool = kani::any();
    let before = f;
    adjust_fef_for_hinges(&mut f, l, hs, he, phi);
    assert!(all_finite(&f));
    if hs { assert!(f[2] == 0.0); }
    if he { assert!(f[5] == 0.0); }
    if !hs && !he {
        for i in 0..6 { assert!(f[i].to_bits() == before[i].to_bits()); }
    }
    // Axial entries never touched.
    assert!(f[0] == 0.0 && f[3] == 0.0);
}

#[kani::proof]
fn adjust_fef_for_hinges_3d_ok() {
    let l = any_len();
    let (py, pz) = (any_phi(), any_phi());
    let mut f = [0.0; 12];
    for i in [1usize, 2, 4, 5, 7, 8, 10, 11] {
        f[i] = any_q();
    }
    let h = any_hinge3d();
    adjust_fef_for_hinges_3d(&mut f, l, h, py, pz);
    assert!(all_finite(&f));
    if h.release_mz_start { assert!(f[5] == 0.0); }
    if h.release_mz_end { assert!(f[11] == 0.0); }
    if h.release_my_start { assert!(f[4] == 0.0); }
    if h.release_my_end { assert!(f[10] == 0.0); }
}

#[kani::proof]
fn expand_fef_12_to_14_ok() {
    let f12: [f64; 12] = kani::any();
    let f14 = expand_fef_12_to_14(&f12);
    for i in 0..6 {
        assert!(f14[i].to_bits() == f12[i].to_bits());
        assert!(f14[7 + i].to_bits() == f12[6 + i].to_bits());
    }
    assert!(f14[6] == 0.0 && f14[13] == 0.0);
}

#[kani::proof]
fn axial_distributed_fef_ok() {
    let l = any_len();
    let a = any_in(0.0, l);
    let b = any_in(0.0, l);
    let (qi, qj) = (any_q(), any_q());
    let f = axial_distributed_fef(qi, qj, a, b, l);
    assert!(all_finite(&f));
}

// ─── linalg/dense.rs ────────────────────────────────────────────────────────

const NMAX: usize = 3;

#[kani::proof]
#[kani::unwind(4)]
fn dense_mat_vec_bounds() {
    let n: usize = kani::any();
    kani::assume(n <= NMAX);
    let a: [f64; NMAX * NMAX] = kani::any();
    let x: [f64; NMAX] = kani::any();
    let y = dense::mat_vec(&a[..n * n], &x[..n], n);
    assert!(y.len() == n);
}

#[kani::proof]
#[kani::unwind(4)]
fn dense_mat_vec_rect_bounds() {
    let nr: usize = kani::any();
    let nc: usize = kani::any();
    kani::assume(nr <= NMAX && nc <= NMAX);
    let a: [f64; NMAX * NMAX] = kani::any();
    let x: [f64; NMAX] = kani::any();
    let y = dense::mat_vec_rect(&a[..nr * nc], &x[..nc], nr, nc);
    assert!(y.len() == nr);
}

#[kani::proof]
#[kani::unwind(4)]
fn dense_mat_mul_bounds() {
    let n: usize = kani::any();
    kani::assume(n <= NMAX);
    let a: [f64; NMAX * NMAX] = kani::any();
    let b: [f64; NMAX * NMAX] = kani::any();
    let c = dense::mat_mul(&a[..n * n], &b[..n * n], n);
    assert!(c.len() == n * n);
}

/// Transpose is an exact (bitwise) involution and moves (i,j) to (j,i).
#[kani::proof]
#[kani::unwind(17)]
fn dense_transpose_involution() {
    let n: usize = kani::any();
    kani::assume(n <= 4);
    let orig: [f64; 16] = kani::any();
    let mut a = orig;
    dense::transpose_inplace(&mut a[..n * n], n);
    let i: usize = kani::any();
    let j: usize = kani::any();
    kani::assume(i < n && j < n);
    assert!(a[i * n + j].to_bits() == orig[j * n + i].to_bits());
    dense::transpose_inplace(&mut a[..n * n], n);
    for k in 0..16 {
        assert!(a[k].to_bits() == orig[k].to_bits());
    }
}

#[kani::proof]
#[kani::unwind(4)]
fn dense_transform_force_displacement_bounds() {
    let n: usize = kani::any();
    kani::assume(n <= NMAX);
    let t: [f64; NMAX * NMAX] = kani::any();
    let f: [f64; NMAX] = kani::any();
    let g = dense::transform_force(&f[..n], &t[..n * n], n);
    let u = dense::transform_displacement(&f[..n], &t[..n * n], n);
    assert!(g.len() == n && u.len() == n);
}

#[kani::proof]
#[kani::unwind(4)]
fn dense_transform_stiffness_bounds() {
    let n: usize = kani::any();
    kani::assume(n <= 2);
    let k: [f64; 4] = kani::any();
    let t: [f64; 4] = kani::any();
    let r = dense::transform_stiffness(&k[..n * n], &t[..n * n], n);
    assert!(r.len() == n * n);
}

/// FINDING candidate: does Tᵀ K T preserve bitwise symmetry? Uses the real
/// 2D truss local stiffness and transform (n = 4). Expected to FAIL.
#[kani::proof]
#[kani::should_panic]
fn transform_stiffness_truss_2d_symmetry_counterexample() {
    let ea = any_in(1e-3, 1e9);
    let c = any_sym(1.0);
    let s = any_sym(1.0);
    let k = truss_local_stiffness_2d(ea, 1.0, 1.0);
    let t = truss_transform_2d(c, s);
    let kg = dense::transform_stiffness(&k, &t, 4);
    assert!(bitwise_symmetric(&kg, 4));
}

#[kani::proof]
#[kani::unwind(4)]
fn dense_extract_submatrix_exact() {
    let n: usize = kani::any();
    kani::assume(n >= 1 && n <= NMAX);
    let a: [f64; NMAX * NMAX] = kani::any();
    let nr: usize = kani::any();
    let nc: usize = kani::any();
    kani::assume(nr <= 2 && nc <= 2);
    let rows: [usize; 2] = kani::any();
    let cols: [usize; 2] = kani::any();
    kani::assume(rows.iter().all(|&r| r < n) && cols.iter().all(|&c| c < n));
    let sub = dense::extract_submatrix(&a[..n * n], n, &rows[..nr], &cols[..nc]);
    assert!(sub.len() == nr * nc);
    let i: usize = kani::any();
    let j: usize = kani::any();
    kani::assume(i < nr && j < nc);
    assert!(sub[i * nc + j].to_bits() == a[rows[i] * n + cols[j]].to_bits());
}

#[kani::proof]
#[kani::unwind(4)]
fn dense_extract_scatter_subvec_roundtrip() {
    let v: [f64; NMAX] = kani::any();
    let m: usize = kani::any();
    kani::assume(m <= NMAX);
    let idx: [usize; NMAX] = kani::any();
    kani::assume(idx.iter().all(|&i| i < NMAX));
    let sub = dense::extract_subvec(&v, &idx[..m]);
    assert!(sub.len() == m);
    let mut full = [0.0f64; NMAX];
    dense::scatter_subvec(&mut full, &idx[..m], &sub);
    for k in 0..m {
        assert!(full[idx[k]].to_bits() == v[idx[k]].to_bits());
    }
}
