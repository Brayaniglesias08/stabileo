//! Certified P-Delta, spectral and harmonic results (docs/verification/CERTIFY.md).
//!
//! Honesty references are independent high-accuracy solutions of the same model:
//! P-Delta by Newton's method with the residual evaluated in double-double
//! arithmetic (so the reference fixed point is accurate to ~κ·ε²).

mod common;

use common::*;
use dedaliano_engine::certify::pdelta::{
    certify_pdelta_2d, certify_pdelta_3d, certify_pdelta_results_2d, PDeltaCertificate,
};
use dedaliano_engine::linalg::{lu_apply, lu_factor, CscMatrix};
use dedaliano_engine::solver::assembly::{assemble_load_vector_2d, assemble_sparse_3d};
use dedaliano_engine::solver::dof::DofNumbering;
use dedaliano_engine::solver::geometric_stiffness::{
    geometric_stiffness_terms_2d, geometric_stiffness_terms_3d, KgAffineTerm,
};
use dedaliano_engine::solver::buckling::solve_buckling_2d;
use dedaliano_engine::solver::pdelta::solve_pdelta_2d;
use dedaliano_engine::solver::sparse_assembly::assemble_stiffness_sparse_2d;
use dedaliano_engine::types::*;
use std::collections::HashMap;

// ───────────────────────── double-double ─────────────────────────

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    (p, a.mul_add(b, -p))
}

#[derive(Clone, Copy, Debug, Default)]
struct DD {
    hi: f64,
    lo: f64,
}

impl DD {
    fn new(x: f64) -> DD {
        DD { hi: x, lo: 0.0 }
    }
    fn norm(s: f64, e: f64) -> DD {
        let (hi, lo) = two_sum(s, e);
        DD { hi, lo }
    }
    fn add(self, b: DD) -> DD {
        let (s, e) = two_sum(self.hi, b.hi);
        DD::norm(s, e + self.lo + b.lo)
    }
    fn neg(self) -> DD {
        DD { hi: -self.hi, lo: -self.lo }
    }
    fn sub(self, b: DD) -> DD {
        self.add(b.neg())
    }
    fn mul(self, b: DD) -> DD {
        let (p, e) = two_prod(self.hi, b.hi);
        DD::norm(p, e + self.hi * b.lo + self.lo * b.hi)
    }
    fn mulf(self, b: f64) -> DD {
        self.mul(DD::new(b))
    }
    fn divf(self, d: f64) -> DD {
        let q1 = self.hi / d;
        let r = self.sub(DD::new(q1).mulf(d));
        DD::norm(q1, r.hi / d)
    }
}

// ───────────────────────── P-Delta reference ─────────────────────────

/// The certificate's model: lower-triangle full K, f, affine K_G terms.
struct PdModel {
    k: CscMatrix,
    nf: usize,
    f: Vec<f64>,
    terms: Vec<KgAffineTerm>,
}

fn pd_model_2d(input: &SolverInput) -> (PdModel, DofNumbering) {
    let dof = DofNumbering::build_2d(input);
    let k = assemble_stiffness_sparse_2d(input, &dof).k_full;
    let f = assemble_load_vector_2d(input, &input.loads, &dof, &[]);
    let terms = geometric_stiffness_terms_2d(input, &dof);
    (PdModel { k, nf: dof.n_free, f, terms }, dof)
}

fn pd_model_3d(input: &SolverInput3D) -> (PdModel, DofNumbering) {
    let dof = DofNumbering::build_3d(input);
    let asm = assemble_sparse_3d(input, &dof, true);
    let terms = geometric_stiffness_terms_3d(input, &dof);
    (PdModel { k: asm.k_full.unwrap(), nf: dof.n_free, f: asm.f, terms }, dof)
}

impl PdModel {
    fn axial(&self, t: &KgAffineTerm, x: &[DD]) -> DD {
        let d = t.elongation.iter().fold(DD::default(), |s, &(i, c)| s.add(x[i].mulf(c)));
        let th = DD::new(t.alpha).mulf(t.dt);
        DD::new(t.e).mulf(t.a).mul(d.divf(t.l).sub(th))
    }

    fn gx(t: &KgAffineTerm, x: &[DD]) -> Vec<DD> {
        let nd = t.dofs.len();
        (0..nd).map(|a| (0..nd).fold(DD::default(), |s, b| s.add(x[t.dofs[b]].mulf(t.g[a * nd + b])))).collect()
    }

    /// F(x) on the free rows, in double-double.
    fn residual(&self, x: &[DD]) -> Vec<DD> {
        let nf = self.nf;
        let mut r: Vec<DD> = self.f[..nf].iter().map(|&v| DD::new(-v)).collect();
        for j in 0..self.k.n {
            for p in self.k.col_ptr[j]..self.k.col_ptr[j + 1] {
                let i = self.k.row_idx[p];
                let v = self.k.values[p];
                if i < nf {
                    r[i] = r[i].add(x[j].mulf(v));
                }
                if i != j && j < nf {
                    r[j] = r[j].add(x[i].mulf(v));
                }
            }
        }
        for t in &self.terms {
            let n = self.axial(t, x);
            let g = Self::gx(t, x);
            for (a, &d) in t.dofs.iter().enumerate() {
                if d < nf {
                    r[d] = r[d].add(n.mul(g[a]));
                }
            }
        }
        r
    }

    /// Dense f64 Jacobian J(x) on the free DOFs.
    fn jacobian(&self, x: &[DD]) -> Vec<f64> {
        let nf = self.nf;
        let mut j = vec![0.0; nf * nf];
        for c in 0..self.k.n {
            for p in self.k.col_ptr[c]..self.k.col_ptr[c + 1] {
                let i = self.k.row_idx[p];
                if i < nf && c < nf {
                    j[i * nf + c] += self.k.values[p];
                    if i != c {
                        j[c * nf + i] += self.k.values[p];
                    }
                }
            }
        }
        for t in &self.terms {
            let nd = t.dofs.len();
            let n = self.axial(t, x).hi;
            let g = Self::gx(t, x);
            let c = t.e * t.a / t.l;
            for (a, &da) in t.dofs.iter().enumerate() {
                if da >= nf {
                    continue;
                }
                for (b, &db) in t.dofs.iter().enumerate() {
                    if db < nf {
                        j[da * nf + db] += n * t.g[a * nd + b];
                    }
                }
                for &(db, tc) in &t.elongation {
                    if db < nf {
                        j[da * nf + db] += c * g[a].hi * tc;
                    }
                }
            }
        }
        j
    }

    /// Newton with double-double residuals from the start `u0` (full vector).
    fn reference(&self, u0: &[f64]) -> Vec<DD> {
        let nf = self.nf;
        let mut x: Vec<DD> = u0.iter().map(|&v| DD::new(v)).collect();
        for _ in 0..30 {
            let r = self.residual(&x);
            let mut jm = self.jacobian(&x);
            let piv = lu_factor(&mut jm, nf).expect("reference Jacobian singular");
            let rhs: Vec<f64> = r.iter().map(|v| v.hi).collect();
            let dx = lu_apply(&jm, &piv, &rhs, nf).unwrap();
            for i in 0..nf {
                x[i] = x[i].sub(DD::new(dx[i]));
            }
            let s: f64 = dx.iter().map(|v| v * v).sum::<f64>().sqrt();
            let xs: f64 = x[..nf].iter().map(|v| v.hi * v.hi).sum::<f64>().sqrt();
            if s <= 1e-30 * xs.max(1e-300) {
                break;
            }
        }
        x
    }
}

fn dd_error(x: &[DD], u: &[f64], nf: usize) -> f64 {
    (0..nf).map(|i| x[i].sub(DD::new(u[i])).hi.powi(2)).sum::<f64>().sqrt()
}

fn gather_2d(dof: &DofNumbering, disp: &[Displacement]) -> Vec<f64> {
    let by: HashMap<usize, &Displacement> = disp.iter().map(|d| (d.node_id, d)).collect();
    let mut u = vec![0.0; dof.n_total];
    for (&(node, local), &g) in &dof.map {
        let d = by[&node];
        u[g] = [d.ux, d.uz, d.ry][local];
    }
    u
}

fn gather_3d(dof: &DofNumbering, disp: &[Displacement3D]) -> Vec<f64> {
    let by: HashMap<usize, &Displacement3D> = disp.iter().map(|d| (d.node_id, d)).collect();
    let mut u = vec![0.0; dof.n_total];
    for (&(node, local), &g) in &dof.map {
        let d = by[&node];
        u[g] = [d.ux, d.uy, d.uz, d.rx, d.ry, d.rz, d.warping.unwrap_or(0.0)][local];
    }
    u
}

fn assert_pd_honest(name: &str, cert: &PDeltaCertificate, model: &PdModel, u: &[f64]) -> (f64, f64) {
    let PDeltaCertificate::Certified { abs_bound, .. } = cert else { panic!("{name}: {cert:?}") };
    let x = model.reference(u);
    let err = dd_error(&x, u, model.nf);
    assert!(err <= *abs_bound, "{name}: true error {err:e} exceeds bound {abs_bound:e}");
    (err, *abs_bound)
}

fn grid_frame(bays: usize, stories: usize, gravity: f64) -> SolverInput {
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
        .map(|(c, s)| SolverLoad::Nodal(SolverNodalLoad { node_id: id(c, s), fx: 5.0, fz: gravity, my: 0.0 }))
        .collect();
    make_input(nodes, vec![(1, 30_000.0, 0.2)], vec![(1, 0.16, 2.1e-3), (2, 0.12, 1.6e-3)], elems, sups, loads)
}

/// Portal with a pinned diagonal brace (truss) and a heated beam.
fn braced_portal() -> SolverInput {
    let mut input = make_portal_frame(4.0, 6.0, 200_000.0, 0.01, 1e-4, 30.0, -600.0);
    input.elements.insert(
        "4".into(),
        SolverElement {
            id: 4,
            elem_type: "truss".into(),
            node_i: 1,
            node_j: 3,
            material_id: 1,
            section_id: 1,
            hinge_start: false,
            hinge_end: false,
        },
    );
    input.loads.push(SolverLoad::Thermal(SolverThermalLoad { element_id: 2, dt_uniform: 30.0, dt_gradient: 0.0 }));
    input
}

fn building_3d(bays: usize, storeys: usize, gravity: f64) -> SolverInput3D {
    let (span, h) = (6.0, 3.5);
    let id = |i: usize, j: usize, k: usize| 1 + k * (bays + 1) * (bays + 1) + j * (bays + 1) + i;
    let mut nodes = vec![];
    for k in 0..=storeys {
        for j in 0..=bays {
            for i in 0..=bays {
                nodes.push((id(i, j, k), span * i as f64, span * j as f64, h * k as f64));
            }
        }
    }
    let mut elems = vec![];
    let mut member = |a: usize, b: usize, s: usize| {
        let e = elems.len() + 1;
        elems.push((e, "frame", a, b, 1, s));
    };
    for k in 0..storeys {
        for j in 0..=bays {
            for i in 0..=bays {
                member(id(i, j, k), id(i, j, k + 1), 1);
            }
        }
    }
    for k in 1..=storeys {
        for j in 0..=bays {
            for i in 0..=bays {
                if i < bays {
                    member(id(i, j, k), id(i + 1, j, k), 2);
                }
                if j < bays {
                    member(id(i, j, k), id(i, j + 1, k), 2);
                }
            }
        }
    }
    let sups = (0..=bays).flat_map(|j| (0..=bays).map(move |i| (id(i, j, 0), vec![true; 6]))).collect();
    let mut loads = vec![];
    for k in 1..=storeys {
        for j in 0..=bays {
            for i in 0..=bays {
                let fx = if i == 0 { 15.0 } else { 0.0 };
                loads.push(SolverLoad3D::Nodal(SolverNodalLoad3D {
                    node_id: id(i, j, k),
                    fx,
                    fy: 4.0,
                    fz: gravity,
                    mx: 0.0,
                    my: 0.0,
                    mz: 0.0,
                    bw: None,
                }));
            }
        }
    }
    make_3d_input(
        nodes,
        vec![(1, 200_000.0, 0.3)],
        vec![(1, 0.012, 1.5e-4, 1.5e-4, 2.0e-6), (2, 0.008, 2.5e-4, 1.0e-5, 1.0e-6)],
        elems,
        sups,
        vec![],
    )
    .with_loads(loads)
}

trait WithLoads {
    fn with_loads(self, loads: Vec<SolverLoad3D>) -> Self;
}

impl WithLoads for SolverInput3D {
    fn with_loads(mut self, loads: Vec<SolverLoad3D>) -> Self {
        self.loads = loads;
        self
    }
}

#[test]
fn certify_pdelta_2d_models() {
    let models = vec![
        ("portal", make_portal_frame(4.0, 6.0, 200_000.0, 0.01, 1e-4, 10.0, -800.0)),
        ("braced portal + thermal", braced_portal()),
        ("grid 3x6 (sparse path)", grid_frame(3, 6, -900.0)),
    ];
    for (name, input) in models {
        let out = certify_pdelta_2d(&input, 50, 1e-10).unwrap();
        assert!(out.results.converged, "{name}");
        let (model, dof) = pd_model_2d(&input);
        let u = gather_2d(&dof, &out.results.results.displacements);
        let (err, bound) = assert_pd_honest(name, &out.certificate, &model, &u);
        let PDeltaCertificate::Certified { rel_bound, stability_margin, mu, beta, .. } = &out.certificate else {
            unreachable!()
        };
        eprintln!(
            "{name}: n_f={} err={err:.2e} bound={bound:.2e} rel={rel_bound:.2e} μ={mu:.2e} β={beta:.2e} B2={:.3}",
            model.nf, out.results.b2_factor
        );
        assert!(*rel_bound < 1e-6, "{name}: bound unexpectedly loose {rel_bound:e}");
        assert!(stability_margin.is_some(), "{name}: stability not certified");
    }
}

#[test]
fn certify_pdelta_3d_building() {
    let input = building_3d(2, 3, -300.0);
    let out = certify_pdelta_3d(&input, 50, 1e-10).unwrap();
    assert!(out.results.converged);
    let (model, dof) = pd_model_3d(&input);
    let u = gather_3d(&dof, &out.results.results.displacements);
    let (err, bound) = assert_pd_honest("building", &out.certificate, &model, &u);
    eprintln!("3D building n_f={}: err={err:.2e} bound={bound:.2e} B2={:.3}", model.nf, out.results.b2_factor);
    let PDeltaCertificate::Certified { rel_bound, stability_margin, .. } = out.certificate else { unreachable!() };
    assert!(rel_bound < 1e-6 && stability_margin.is_some(), "{rel_bound:e}");
}

/// Critical gravity load per column of the test portal (linear buckling).
fn portal_pcr() -> f64 {
    let input = make_portal_frame(4.0, 6.0, 200_000.0, 0.01, 1e-4, 0.0, -1000.0);
    1000.0 * solve_buckling_2d(&input, 1).unwrap().modes[0].load_factor
}

/// An iteration-limited answer is still certified, with a bound on its
/// distance to the true fixed point that covers the true error.
#[test]
fn certify_pdelta_unconverged_and_perturbed() {
    let input = make_portal_frame(4.0, 6.0, 200_000.0, 0.01, 1e-4, 10.0, -0.8 * portal_pcr());
    let (model, dof) = pd_model_2d(&input);
    let few = solve_pdelta_2d(&input, 2, 1e-12).unwrap();
    let u = gather_2d(&dof, &few.results.displacements);
    let cert = certify_pdelta_results_2d(&input, &few.results);
    let (err, bound) = assert_pd_honest("2 iterations", &cert, &model, &u);
    assert!(err > 1e-9, "2 iterations should not have converged: {err:e}");
    eprintln!("unconverged: err={err:.2e} bound={bound:.2e}");

    // Perturb a converged answer: the bound must grow to cover it (small
    // perturbation), or the certificate is refused (one beyond the reach
    // μ/(2β) of the quadratic bound).
    let full = solve_pdelta_2d(&input, 50, 1e-12).unwrap();
    for (delta, must_certify) in [(1e-7, true), (1e-3, false)] {
        let mut res = full.results.clone();
        for d in res.displacements.iter_mut().filter(|d| d.node_id == 2) {
            d.ux += delta;
        }
        let u = gather_2d(&dof, &res.displacements);
        let cert = certify_pdelta_results_2d(&input, &res);
        if must_certify || cert.is_certified() {
            let (err, bound) = assert_pd_honest("perturbed", &cert, &model, &u);
            assert!(err > 0.9 * delta && bound >= delta, "{err:e} {bound:e}");
        }
    }
}

/// Past the critical load the solver's LU answer is an unstable equilibrium:
/// stability is never certified, and the linear answer is not certified as
/// the P-Delta one.
#[test]
fn certify_pdelta_refusals() {
    let input = make_portal_frame(4.0, 6.0, 200_000.0, 0.01, 1e-4, 10.0, -1.5 * portal_pcr());
    let out = certify_pdelta_2d(&input, 50, 1e-10).unwrap();
    if let PDeltaCertificate::Certified { stability_margin, .. } = &out.certificate {
        assert!(stability_margin.is_none(), "unstable state certified stable: {:?}", out.certificate);
    }
    // The linear solution offered as the second-order one (moderate load).
    let input = make_portal_frame(4.0, 6.0, 200_000.0, 0.01, 1e-4, 10.0, -1500.0);
    let (model, dof) = pd_model_2d(&input);
    let out = certify_pdelta_2d(&input, 50, 1e-12).unwrap();
    let lin = &out.results.linear_results;
    let u = gather_2d(&dof, &lin.displacements);
    let cert = certify_pdelta_results_2d(&input, lin);
    if cert.is_certified() {
        assert_pd_honest("linear as P-Delta", &cert, &model, &u);
    }
    // Missing node in the displacements.
    let mut res = out.results.results.clone();
    res.displacements.pop();
    assert!(!certify_pdelta_results_2d(&input, &res).is_certified());
    // Constraints are refused (not silently ignored).
    let mut c = input.clone();
    c.constraints.push(Constraint::EqualDOF(EqualDOFConstraint { master_node: 2, slave_node: 3, dofs: vec![0] }));
    assert!(!certify_pdelta_results_2d(&c, &out.results.results).is_certified());
}
