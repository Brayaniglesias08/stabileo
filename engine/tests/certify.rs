//! Certified linear static results (docs/verification/CERTIFY.md).
//!
//! Honesty is checked against a reference solution of the same reduced f64
//! system computed by iterative refinement with double-double residuals and a
//! double-double solution vector (accurate to ~κ·ε², far below any bound).

mod common;
mod fuzz_models;

use common::*;
use dedaliano_engine::certify::buckling::{certify_buckling_2d, certify_buckling_3d, BucklingCertificate};
use dedaliano_engine::certify::{
    certify_linear_2d, certify_linear_3d, certify_linear_system, Certificate,
};
use dedaliano_engine::linalg::{lu_apply, lu_factor};
use dedaliano_engine::solver::assembly::{
    assemble_load_vector_2d, assemble_load_vector_3d_dense, assemble_stiffness_2d, assemble_stiffness_3d,
};
use dedaliano_engine::linalg::CscMatrix;
use dedaliano_engine::solver::assembly::assemble_stiffness_sparse_3d;
use dedaliano_engine::solver::dof::DofNumbering;
use dedaliano_engine::solver::linear::{expand_curved_beams_3d, solve_2d, solve_3d, SPARSE_THRESHOLD};
use dedaliano_engine::solver::sparse_assembly::{assemble_load_vector_sparse_3d, assemble_stiffness_sparse_2d};
use dedaliano_engine::types::*;
use std::collections::HashMap;
use std::time::Instant;

// ───────────────────────── double-double reference ─────────────────────────

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    (p, a.mul_add(b, -p))
}

/// Compensated accumulator (Ogita–Rump–Oishi Sum2/Dot2).
#[derive(Default, Clone, Copy)]
struct Acc {
    s: f64,
    c: f64,
}

impl Acc {
    fn add(&mut self, x: f64) {
        let (s, e) = two_sum(self.s, x);
        self.s = s;
        self.c += e;
    }
    fn add_prod(&mut self, a: f64, b: f64) {
        let (p, e) = two_prod(a, b);
        self.add(p);
        self.c += e;
    }
    fn hi_lo(self) -> (f64, f64) {
        two_sum(self.s, self.c)
    }
}

/// Reduced system K_FF u = f_F − K_FR u_R (f as hi+lo), plus the solver's ũ_F.
struct Reduced {
    k: Vec<f64>,
    n: usize,
    f_hi: Vec<f64>,
    f_lo: Vec<f64>,
    u: Vec<f64>,
}

fn reduce(k_full: &[f64], n: usize, nf: usize, f: &[f64], u_full: &[f64]) -> Reduced {
    let mut k = vec![0.0; nf * nf];
    let (mut f_hi, mut f_lo) = (vec![0.0; nf], vec![0.0; nf]);
    for i in 0..nf {
        k[i * nf..(i + 1) * nf].copy_from_slice(&k_full[i * n..i * n + nf]);
        let mut acc = Acc::default();
        acc.add(f[i]);
        for j in nf..n {
            acc.add_prod(-k_full[i * n + j], u_full[j]);
        }
        (f_hi[i], f_lo[i]) = acc.hi_lo();
    }
    Reduced { k, n: nf, f_hi, f_lo, u: u_full[..nf].to_vec() }
}

fn reduced_2d(input: &SolverInput, res: &AnalysisResults) -> Reduced {
    let dof = DofNumbering::build_2d(input);
    let (k, f) = if dof.n_free >= SPARSE_THRESHOLD {
        let st = assemble_stiffness_sparse_2d(input, &dof);
        let f = assemble_load_vector_2d(input, &input.loads, &dof, &st.inclined_transforms_2d);
        (dense_from_lower(&st.k_full), f)
    } else {
        let st = assemble_stiffness_2d(input, &dof);
        let f = assemble_load_vector_2d(input, &input.loads, &dof, &st.inclined_transforms_2d);
        (st.k, f)
    };
    let d: HashMap<usize, &Displacement> = res.displacements.iter().map(|d| (d.node_id, d)).collect();
    let mut u = vec![0.0; dof.n_total];
    for (&(node, l), &g) in &dof.map {
        let x = d[&node];
        u[g] = [x.ux, x.uz, x.ry][l];
    }
    reduce(&k, dof.n_total, dof.n_free, &f, &u)
}

/// Dense copy of a symmetric matrix stored as its lower triangle.
fn dense_from_lower(a: &CscMatrix) -> Vec<f64> {
    let n = a.n;
    let mut d = vec![0.0; n * n];
    for j in 0..n {
        for p in a.col_ptr[j]..a.col_ptr[j + 1] {
            let i = a.row_idx[p];
            d[i * n + j] = a.values[p];
            d[j * n + i] = a.values[p];
        }
    }
    d
}

fn reduced_3d(input: &SolverInput3D, res: &AnalysisResults3D) -> Reduced {
    let input = &expand_curved_beams_3d(input);
    let dof = DofNumbering::build_3d(input);
    // Same assembly as the certificate: the solver's sparse path above the threshold.
    let (k, f) = if dof.n_free >= SPARSE_THRESHOLD {
        let st = assemble_stiffness_sparse_3d(input, &dof, true);
        let f = assemble_load_vector_sparse_3d(input, &input.loads, &dof, &st.inclined_transforms);
        (dense_from_lower(st.k_full.as_ref().unwrap()), f)
    } else {
        let st = assemble_stiffness_3d(input, &dof);
        let f = assemble_load_vector_3d_dense(input, &input.loads, &dof, &st.inclined_transforms);
        (st.k, f)
    };
    let d: HashMap<usize, &Displacement3D> = res.displacements.iter().map(|d| (d.node_id, d)).collect();
    let mut u = vec![0.0; dof.n_total];
    for (&(node, l), &g) in &dof.map {
        let x = d[&node];
        u[g] = [x.ux, x.uy, x.uz, x.rx, x.ry, x.rz, x.warping.unwrap_or(0.0)][l];
    }
    reduce(&k, dof.n_total, dof.n_free, &f, &u)
}

/// Reference solution (hi, lo) by double-double iterative refinement.
fn reference_solution(r: &Reduced) -> Option<(Vec<f64>, Vec<f64>)> {
    let n = r.n;
    let mut lu = r.k.clone();
    let piv = lu_factor(&mut lu, n)?;
    let mut xh = vec![0.0; n];
    let mut xl = vec![0.0; n];
    for _ in 0..30 {
        let mut res = vec![0.0; n];
        for i in 0..n {
            let mut acc = Acc::default();
            acc.add(r.f_hi[i]);
            acc.add(r.f_lo[i]);
            for j in 0..n {
                let kij = r.k[i * n + j];
                acc.add_prod(-kij, xh[j]);
                acc.add_prod(-kij, xl[j]);
            }
            res[i] = acc.hi_lo().0;
        }
        let d = lu_apply(&lu, &piv, &res, n)?;
        for i in 0..n {
            let (s, e) = two_sum(xh[i], d[i]);
            (xh[i], xl[i]) = two_sum(s, xl[i] + e);
        }
    }
    Some((xh, xl))
}

/// ‖ũ − u_ref‖₂ computed accurately.
fn true_error(r: &Reduced) -> f64 {
    let (xh, xl) = reference_solution(r).expect("reference solve");
    let mut acc = 0.0f64;
    for i in 0..r.n {
        let (d, e) = two_sum(r.u[i], -xh[i]);
        let di = d + (e - xl[i]);
        acc += di * di;
    }
    acc.sqrt()
}

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Assert the certificate's bound dominates the true error. Returns (abs, rel, err).
fn assert_honest(label: &str, cert: &Certificate, r: &Reduced) -> (f64, f64, f64) {
    let Certificate::Certified { abs_bound, rel_bound, .. } = cert else {
        panic!("{label}: expected certified, got {cert:?}");
    };
    let err = true_error(r);
    assert!(err <= *abs_bound, "{label}: true error {err:e} exceeds bound {abs_bound:e}");
    let un = norm(&r.u);
    if un > 0.0 {
        assert!(err / un <= *rel_bound * (1.0 + 1e-12), "{label}: relative bound violated");
    }
    (*abs_bound, *rel_bound, err)
}

// ───────────────────────── fixtures ─────────────────────────

fn fixture_paths(prefix: &str, filter: impl Fn(&str) -> bool) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().unwrap().to_str().unwrap();
            n.starts_with(prefix) && n.ends_with("-input.json") && filter(n)
        })
        .collect();
    v.sort();
    v
}

#[test]
fn certify_2d_fixtures() {
    let mut paths = fixture_paths("ex-", |n| !n.starts_with("ex-3d-"));
    paths.extend(fixture_paths("random-", |_| true));
    let (mut certified, mut unsupported) = (0, 0);
    for p in &paths {
        let name = p.file_name().unwrap().to_str().unwrap();
        let json = std::fs::read_to_string(p).unwrap();
        let Ok(input) = serde_json::from_str::<SolverInput>(&json) else {
            eprintln!("{name}: not a 2D SolverInput, skipped");
            continue;
        };
        let (res, cert) = match certify_linear_2d(&input) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("{name}: solver error {e}");
                continue;
            }
        };
        // same displacements as plain solve_2d
        let plain = solve_2d(&input).unwrap();
        for (a, b) in res.displacements.iter().zip(&plain.displacements) {
            assert_eq!((a.ux, a.uz, a.ry), (b.ux, b.uz, b.ry));
        }
        match &cert {
            Certificate::Uncertified { reason } => {
                assert!(reason.starts_with("unsupported"), "{name}: uncertified: {reason}");
                eprintln!("{name}: {reason}");
                unsupported += 1;
            }
            Certificate::Certified { .. } => {
                let r = reduced_2d(&input, &res);
                let (abs, rel, err) = assert_honest(name, &cert, &r);
                eprintln!("{name}: nf={} abs={abs:.2e} rel={rel:.2e} true_err={err:.2e}", r.n);
                certified += 1;
            }
        }
    }
    eprintln!("2D fixtures: {certified} certified, {unsupported} unsupported");
    assert!(certified >= 20, "only {certified} 2D fixtures certified");
}

fn certify_3d_fixture_set(max_nf: usize, min_nf: usize, strict: bool) -> usize {
    let mut certified = 0;
    for p in fixture_paths("ex-3d-", |_| true) {
        let name = p.file_name().unwrap().to_str().unwrap().to_string();
        let input: SolverInput3D = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        let nf = DofNumbering::build_3d(&expand_curved_beams_3d(&input)).n_free;
        if nf > max_nf || nf < min_nf {
            continue;
        }
        let (res, cert) = certify_linear_3d(&input).unwrap();
        match &cert {
            Certificate::Uncertified { reason } => {
                assert!(!strict || reason.starts_with("unsupported"), "{name}: uncertified: {reason}");
                eprintln!("{name}: nf={nf} {reason}");
            }
            Certificate::Certified { .. } => {
                let r = reduced_3d(&input, &res);
                let (abs, rel, err) = assert_honest(&name, &cert, &r);
                eprintln!("{name}: nf={nf} abs={abs:.2e} rel={rel:.2e} true_err={err:.2e}");
                certified += 1;
            }
        }
    }
    certified
}

#[test]
fn certify_3d_fixtures_small() {
    let c = certify_3d_fixture_set(400, 0, true);
    assert!(c >= 4, "only {c} 3D fixtures certified");
}

/// Larger 3D fixtures (slow in debug): `cargo test --release --test certify -- --ignored`.
/// Reports only: the naive interval Cholesky of CERTIFY.md may legitimately
/// fail (Uncertified) on larger models through interval overestimation.
#[test]
#[ignore]
fn certify_3d_fixtures_large() {
    let t = Instant::now();
    certify_3d_fixture_set(usize::MAX, 401, false);
    eprintln!("large 3D fixtures: {:.1} s", t.elapsed().as_secs_f64());
}

// ───────────────────────── synthetic models ─────────────────────────

fn portal() -> SolverInput {
    make_portal_frame(4.0, 6.0, 200_000.0, 0.01, 1e-4, 10.0, -20.0)
}

#[test]
fn certify_basic_models_2d() {
    let models = vec![
        ("portal", portal()),
        (
            "continuous",
            make_continuous_beam(
                &[5.0, 7.0, 5.0],
                8,
                30_000.0,
                0.12,
                1.6e-3,
                (1..=24)
                    .map(|i| SolverLoad::Distributed(SolverDistributedLoad {
                        element_id: i, q_i: -10.0, q_j: -10.0, a: None, b: None,
                    }))
                    .collect(),
            ),
        ),
    ];
    for (name, input) in models {
        let (res, cert) = certify_linear_2d(&input).unwrap();
        let r = reduced_2d(&input, &res);
        let (abs, rel, err) = assert_honest(name, &cert, &r);
        eprintln!("{name}: abs={abs:.2e} rel={rel:.2e} err={err:.2e}");
        assert!(rel < 1e-8, "{name}: rel bound unexpectedly loose: {rel:e}");
    }
}

/// A deliberately perturbed ũ: the bound must cover the perturbation.
#[test]
fn bound_covers_deliberate_perturbation() {
    let input = portal();
    let res = solve_2d(&input).unwrap();
    let mut r = reduced_2d(&input, &res);
    let un = norm(&r.u);
    for (i, x) in r.u.iter_mut().enumerate() {
        *x += un * 1e-5 * if i % 2 == 0 { 1.0 } else { -0.7 };
    }
    let f: Vec<f64> = r.f_hi.iter().zip(&r.f_lo).map(|(h, l)| h + l).collect();
    // σ from the auto path on the unperturbed system
    let sigma = match certify_linear_2d(&input).unwrap().1 {
        Certificate::Certified { sigma, .. } => sigma,
        c => panic!("{c:?}"),
    };
    let cert = certify_linear_system(&r.k, r.n, &f, &r.u, sigma);
    let Certificate::Certified { abs_bound, .. } = cert else { panic!("{cert:?}") };
    let err = true_error(&r);
    assert!(err > 1e-6 * un, "perturbation should dominate");
    assert!(err <= abs_bound, "err {err:e} > bound {abs_bound:e}");
    // σ larger than λ_min must be refused
    let refused = certify_linear_system(&r.k, r.n, &f, &r.u, 1e30);
    assert!(!refused.is_certified());
}

/// Very stiff and very flexible members together: either an honest (larger)
/// bound or Uncertified — never a wrong bound.
#[test]
fn ill_conditioned_models() {
    for (stiff_e, flex_e) in [(2e8, 2e-2), (2e11, 2e-5), (2e14, 2e-8)] {
        let input = make_input(
            vec![(1, 0.0, 0.0), (2, 3.0, 0.0), (3, 6.0, 0.0), (4, 9.0, 0.0), (5, 9.0, 3.0)],
            vec![(1, stiff_e, 0.3), (2, flex_e, 0.3)],
            vec![(1, 0.05, 1e-3), (2, 1e-4, 1e-8)],
            vec![
                (1, "frame", 1, 2, 1, 1, false, false),
                (2, "frame", 2, 3, 2, 2, false, false),
                (3, "frame", 3, 4, 1, 1, false, false),
                (4, "frame", 4, 5, 2, 2, false, false),
            ],
            vec![(1, 1, "fixed"), (2, 5, "pinned")],
            vec![SolverLoad::Nodal(SolverNodalLoad { node_id: 3, fx: 1.0, fz: -5.0, my: 0.3 })],
        );
        let label = format!("E ratio {:.0e}", stiff_e / flex_e);
        match certify_linear_2d(&input) {
            Err(e) => eprintln!("{label}: solver error {e}"),
            Ok((res, cert)) => match &cert {
                Certificate::Uncertified { reason } => eprintln!("{label}: uncertified ({reason})"),
                Certificate::Certified { .. } => {
                    let r = reduced_2d(&input, &res);
                    let (abs, rel, err) = assert_honest(&label, &cert, &r);
                    eprintln!("{label}: abs={abs:.2e} rel={rel:.2e} err={err:.2e}");
                }
            },
        }
    }
}

fn assert_never_certified(label: &str, out: Result<(AnalysisResults, Certificate), String>) {
    match out {
        Err(e) => eprintln!("{label}: solver refused ({e})"),
        Ok((_, Certificate::Uncertified { reason })) => eprintln!("{label}: uncertified ({reason})"),
        Ok((_, c)) => panic!("{label}: mechanism certified: {c:?}"),
    }
}

#[test]
fn mechanisms_never_certified() {
    // Square truss with no diagonal: sway mechanism.
    let square = make_input(
        vec![(1, 0.0, 0.0), (2, 4.0, 0.0), (3, 4.0, 3.0), (4, 0.0, 3.0)],
        vec![(1, 200_000.0, 0.3)],
        vec![(1, 0.01, 0.0)],
        vec![
            (1, "truss", 1, 4, 1, 1, false, false),
            (2, "truss", 4, 3, 1, 1, false, false),
            (3, "truss", 3, 2, 1, 1, false, false),
        ],
        vec![(1, 1, "pinned"), (2, 2, "pinned")],
        vec![SolverLoad::Nodal(SolverNodalLoad { node_id: 3, fx: 0.0, fz: -10.0, my: 0.0 })],
    );
    assert_never_certified("truss sway", certify_linear_2d(&square));

    // Beam on two rollers: rigid horizontal translation unrestrained.
    let rollers = make_beam(4, 8.0, 200_000.0, 0.01, 1e-4, "rollerX", Some("rollerX"), vec![
        SolverLoad::Nodal(SolverNodalLoad { node_id: 3, fx: 0.0, fz: -10.0, my: 0.0 }),
    ]);
    assert_never_certified("rollers", certify_linear_2d(&rollers));

    // Three collinear hinges in a frame span: local mechanism.
    let hinged = make_input(
        vec![(1, 0.0, 0.0), (2, 3.0, 0.0), (3, 6.0, 0.0)],
        vec![(1, 200_000.0, 0.3)],
        vec![(1, 0.01, 1e-4)],
        vec![
            (1, "frame", 1, 2, 1, 1, false, true),
            (2, "frame", 2, 3, 1, 1, true, false),
        ],
        vec![(1, 1, "pinned"), (2, 3, "pinned")],
        vec![SolverLoad::Nodal(SolverNodalLoad { node_id: 2, fx: 0.0, fz: -10.0, my: 0.0 })],
    );
    assert_never_certified("hinge chain", certify_linear_2d(&hinged));

    // Near-singular system directly: S ⪰ 0 with a zero eigenvalue.
    let k = vec![1.0, -1.0, 0.0, -1.0, 2.0, -1.0, 0.0, -1.0, 1.0];
    let c = dedaliano_engine::certify::certify_linear_system_auto(&k, 3, &[1.0, 0.0, -1.0], &[1.0, 0.0, -1.0]);
    assert!(!c.is_certified(), "{c:?}");
}

#[test]
fn constraints_and_inclined_are_unsupported() {
    let mut input = portal();
    let sup = input.supports.values_mut().next().unwrap();
    sup.support_type = "inclinedRoller".into();
    sup.angle = Some(0.3);
    match certify_linear_2d(&input).expect("inclined portal solves").1 {
        Certificate::Uncertified { reason } => assert!(reason.contains("inclined"), "{reason}"),
        c => panic!("{c:?}"),
    }
}

#[test]
fn certify_3d_beam() {
    let mut input = make_3d_beam(
        10, 6.0, 200_000.0, 0.3, 0.01, 1e-4, 2e-4, 1e-5,
        vec![true; 6], None,
        vec![SolverLoad3D::Nodal(SolverNodalLoad3D {
            node_id: 11, fx: 1.0, fy: -3.0, fz: 2.0, mx: 0.1, my: 0.0, mz: 0.0, bw: None,
        })],
    );
    pin_legacy_local_axes(&mut input);
    let (res, cert) = certify_linear_3d(&input).unwrap();
    let r = reduced_3d(&input, &res);
    let (abs, rel, err) = assert_honest("3d cantilever", &cert, &r);
    eprintln!("3d cantilever: abs={abs:.2e} rel={rel:.2e} err={err:.2e}");
    let plain = solve_3d(&input).unwrap();
    assert_eq!(plain.displacements[3].uy, res.displacements[3].uy);
}

// ───────────────────────── overhead ─────────────────────────

/// Grid frame with `bays × stories` bays, fixed bases, lateral + gravity loads.
fn grid_frame(bays: usize, stories: usize) -> SolverInput {
    let mut nodes = vec![];
    let id = |c: usize, s: usize| s * (bays + 1) + c + 1;
    for s in 0..=stories {
        for c in 0..=bays {
            nodes.push((id(c, s), c as f64 * 6.0, s as f64 * 3.5));
        }
    }
    let mut elems = vec![];
    let mut e = 1;
    for s in 0..stories {
        for c in 0..=bays {
            elems.push((e, "frame", id(c, s), id(c, s + 1), 1, 1, false, false));
            e += 1;
        }
    }
    for s in 1..=stories {
        for c in 0..bays {
            elems.push((e, "frame", id(c, s), id(c + 1, s), 1, 2, false, false));
            e += 1;
        }
    }
    let sups = (0..=bays).map(|c| (c + 1, id(c, 0), "fixed")).collect();
    let loads = (1..=stories)
        .flat_map(|s| (0..=bays).map(move |c| (c, s)))
        .map(|(c, s)| SolverLoad::Nodal(SolverNodalLoad { node_id: id(c, s), fx: 5.0, fz: -20.0, my: 0.0 }))
        .collect();
    make_input(nodes, vec![(1, 30_000.0, 0.2)], vec![(1, 0.16, 2.1e-3), (2, 0.12, 1.6e-3)], elems, sups, loads)
}

/// Overhead of certification vs plain solve. Run with
/// `cargo test --release --test certify -- --ignored --nocapture overhead`.
#[test]
#[ignore]
fn overhead() {
    for (bays, stories) in [(2, 3), (4, 8), (6, 15), (8, 15), (6, 30), (10, 20), (20, 40), (30, 80), (50, 100)] {
        let input = grid_frame(bays, stories);
        let nf = DofNumbering::build_2d(&input).n_free;
        let t = Instant::now();
        let res = solve_2d(&input).unwrap();
        let t_solve = t.elapsed();
        let t = Instant::now();
        let (_, cert) = certify_linear_2d(&input).unwrap();
        let t_cert = t.elapsed();
        let outcome = match &cert {
            Certificate::Certified { abs_bound, rel_bound, .. } => {
                assert_honest("grid", &cert, &reduced_2d(&input, &res));
                format!("abs={abs_bound:.2e} rel={rel_bound:.2e}")
            }
            Certificate::Uncertified { reason } => format!("UNCERTIFIED: {reason}"),
        };
        eprintln!(
            "nf={nf:5}: solve {:9.3} ms, solve+certify {:10.1} ms ({:.0}x), {outcome}",
            t_solve.as_secs_f64() * 1e3,
            t_cert.as_secs_f64() * 1e3,
            t_cert.as_secs_f64() / t_solve.as_secs_f64()
        );
    }
}

// ───────────────────────── buckling ─────────────────────────

fn assert_buckling_enclosure(name: &str, cert: &BucklingCertificate) -> (f64, Option<f64>) {
    let BucklingCertificate::Certified { lambda_lower, lambda_upper, lambda_computed } = cert else {
        panic!("{name}: {cert:?}");
    };
    assert!(*lambda_lower > 0.0 && lambda_lower <= lambda_computed, "{name}: {cert:?}");
    if let Some(ub) = lambda_upper {
        assert!(lambda_computed <= ub, "{name}: {cert:?}");
    }
    (*lambda_lower, *lambda_upper)
}

#[test]
fn certify_buckling_euler_column() {
    // Pinned–pinned column: FE λ_cr converges to π²EI/(PL²) from above.
    let (l, e, a, iz, p) = (5.0, 200_000.0, 0.01, 1e-4, 100.0);
    let euler = std::f64::consts::PI.powi(2) * e * 1000.0 * iz / (l * l) / p;
    for n_elem in [4, 16, 64] {
        let input = make_column(n_elem, l, e, a, iz, "pinned", "rollerX", -p);
        let out = certify_buckling_2d(&input, 1).unwrap();
        let (lo, hi) = assert_buckling_enclosure("column", &out.certificate);
        let hi = hi.expect("Rayleigh upper bound");
        eprintln!("column n={n_elem}: λ_cr ∈ [{lo:.9e}, {hi:.9e}], Euler {euler:.9e}");
        assert!((hi - lo) / lo < 1e-3, "enclosure too wide: [{lo}, {hi}]");
        assert!(lo > 0.99 * euler && lo < 1.05 * euler, "λ̲ {lo} vs Euler {euler}");
    }
}

#[test]
fn certify_buckling_3d_fixtures() {
    let mut certified = 0;
    for p in fixture_paths("ex-3d-", |_| true) {
        let name = p.file_name().unwrap().to_str().unwrap().to_string();
        let input: SolverInput3D = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        if DofNumbering::build_3d(&expand_curved_beams_3d(&input)).n_free > 600 {
            continue;
        }
        let Ok(out) = certify_buckling_3d(&input, 1) else { continue }; // no compression etc.
        match &out.certificate {
            BucklingCertificate::Uncertified { reason } => eprintln!("{name}: {reason}"),
            c => {
                let (lo, hi) = assert_buckling_enclosure(&name, c);
                eprintln!("{name}: λ_cr ∈ [{lo:.6e}, {hi:?}]");
                certified += 1;
            }
        }
    }
    assert!(certified >= 1, "no 3D buckling fixture certified");
}

// ───────────────────────── modal ─────────────────────────

#[test]
fn certify_modal_3d_fixtures() {
    use dedaliano_engine::certify::modal::{certify_modal_3d, ModalCertificate};
    let mut complete = 0;
    for p in fixture_paths("ex-3d-", |_| true) {
        let name = p.file_name().unwrap().to_str().unwrap().to_string();
        let input: SolverInput3D = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        if !input.constraints.is_empty() || DofNumbering::build_3d(&input).n_free > 600 {
            continue;
        }
        let densities: HashMap<String, f64> =
            input.materials.values().map(|m| (m.id.to_string(), 7.85)).collect();
        let Ok(out) = certify_modal_3d(&input, &densities, 6) else { continue };
        match &out.certificate {
            ModalCertificate::Uncertified { reason } => eprintln!("{name}: {reason}"),
            ModalCertificate::Certified { modes, complete_modes, cutoff_omega2, count_failure } => {
                for (e, m) in modes.iter().zip(&out.results.modes) {
                    let w2 = m.omega * m.omega;
                    assert!(e.omega2_lower <= w2 && w2 <= e.omega2_upper, "{name}: {e:?}");
                }
                let w = modes.iter().map(|e| (e.omega2_upper - e.omega2_lower) / e.omega2_computed).fold(0.0, f64::max);
                eprintln!(
                    "{name}: max rel width {w:.1e}, {complete_modes}/{} complete below ω² = {cutoff_omega2:.4e} {count_failure:?}",
                    modes.len()
                );
                if *complete_modes == modes.len() {
                    complete += 1;
                }
            }
        }
    }
    assert!(complete >= 3, "only {complete} fixtures certified complete");
}

// ───────────────────────── fuzz with an oracle ─────────────────────────

/// Random models (the crash-free fuzz generators): whenever the solver returns
/// results, the certificate is either honest (true error ≤ bound, checked
/// against a double-double reference) or Uncertified with a reason.
#[test]
fn certify_fuzz_oracle() {
    use dedaliano_engine::certify::{certify_results_2d, certify_results_3d};
    let (mut cert, mut unc, mut solved) = (0, 0, 0);
    let mut reasons: HashMap<String, usize> = HashMap::new();
    for seed in 0..400u64 {
        let input = fuzz_models::random_2d_model(seed);
        if let Ok(res) = solve_2d(&input) {
            solved += 1;
            match certify_results_2d(&input, &res) {
                c @ Certificate::Certified { .. } => {
                    assert_honest(&format!("2d seed {seed}"), &c, &reduced_2d(&input, &res));
                    cert += 1;
                }
                Certificate::Uncertified { reason } => {
                    assert!(!reason.is_empty());
                    *reasons.entry(reason.chars().take(40).collect()).or_default() += 1;
                    unc += 1;
                }
            }
        }
        let input = fuzz_models::random_3d_model(100_000 + seed);
        if let Ok(res) = solve_3d(&input) {
            solved += 1;
            match certify_results_3d(&input, &res) {
                c @ Certificate::Certified { .. } => {
                    assert_honest(&format!("3d seed {seed}"), &c, &reduced_3d(&input, &res));
                    cert += 1;
                }
                Certificate::Uncertified { reason } => {
                    assert!(!reason.is_empty());
                    *reasons.entry(reason.chars().take(40).collect()).or_default() += 1;
                    unc += 1;
                }
            }
        }
    }
    eprintln!("fuzz: {solved} solved, {cert} certified, {unc} uncertified: {reasons:?}");
    assert!(cert > 0);
}
