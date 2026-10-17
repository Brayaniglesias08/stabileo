//! Feature × analysis support matrix.
//!
//! Every analysis must either honour every model feature it is handed or
//! refuse it by name. The failure this file exists to catch is the third
//! option: an analysis that returns a plausible answer for a model it has
//! quietly simplified — connectors dropped, an inclined support read as a flat
//! one, a spring's angle ignored. Each of those has happened.
//!
//! For each feature there is one or more small model in which the feature
//! *matters* (removing it changes the answer), built three ways:
//!
//!   * `with`    — the model using the feature;
//!   * `without` — the same model with the feature stripped, which is what an
//!                 analysis that ignores the feature actually solves;
//!   * `equiv`   — where it exists, the same physical structure expressed
//!                 without the feature (a rotated model for an inclined
//!                 support, a split member with a nodal load for a point load
//!                 on a member, a stub member for a spring, …).
//!
//! Each analysis is run on all three, and the pair is classified:
//!
//!   * SUPPORTED — `with` matches the reference: the same analysis on `equiv`,
//!                 or, when no equivalent exists, the linear solver in the
//!                 linear limit (static analyses) or an analytical value;
//!   * REFUSED   — the analysis returns an error that names the feature;
//!   * IGNORED   — `with` matches `without`: the feature was silently dropped;
//!   * WRONG     — `with` matches neither;
//!   * N/A       — the feature does not change this analysis' answer (e.g. a
//!                 member load in a modal analysis), so nothing is checked.
//!
//! The matrix is rendered into `docs/verification/FEATURE_MATRIX.md`, and the
//! test fails if the file is out of date (regenerate with
//! `UPDATE_FEATURE_MATRIX=1 cargo test --release --test feature_matrix`) or if
//! any IGNORED/WRONG cell is not listed in `KNOWN` with the branch fixing it.

use dedaliano_engine::element::fiber_beam::{Fiber, FiberMaterial, FiberSectionDef};
use dedaliano_engine::postprocess::influence;
use dedaliano_engine::solver::{
    arc_length, buckling, cable, constraints, contact, corotational, creep_shrinkage, fiber_nonlinear,
    harmonic, kinematic, linear, load_cases, material_nonlinear, modal, moving_loads, pdelta, plastic,
    reduction, spectral, ssi, staged, time_integration, winkler,
};
use dedaliano_engine::types::*;
use std::collections::{BTreeMap, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};

// ==================== constants ====================

const E: f64 = 200_000.0; // MPa
const NU: f64 = 0.3;
const RHO: f64 = 7_850.0; // kg/m³
/// Material id of massless, stiff "equivalent" stubs (springs expressed as members).
const STUB_MAT: usize = 9;
/// Material id of massless, very stiff members standing in for rigid links.
const RIGID_MAT: usize = 8;

// ==================== responses and comparison ====================

/// A response, reduced to numbers that can be compared between models.
#[derive(Clone, Debug)]
enum Out {
    /// Per probe (or per time sample) `[ux, uz, ry]` in global axes; rotatable.
    D2(Vec<[f64; 3]>),
    /// Per probe `[ux, uy, uz, rx, ry, rz]` in global axes; rotatable.
    D3(Vec<[f64; 6]>),
    /// Scalars. `true` = invariant under a rigid rotation of the model.
    S(Vec<f64>, bool),
}

fn rot2(v: [f64; 3], phi: f64) -> [f64; 3] {
    let (s, c) = phi.sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]]
}

fn rot3(v: [f64; 6], r: &[[f64; 3]; 3]) -> [f64; 6] {
    let mut o = [0.0; 6];
    for i in 0..3 {
        for k in 0..3 {
            o[i] += r[i][k] * v[k];
            o[i + 3] += r[i][k] * v[k + 3];
        }
    }
    o
}

/// Relative difference of `a` from reference `b`; `None` when not comparable.
fn rel(a: &Out, b: &Out) -> Option<f64> {
    // A NaN in the reference marks a component it does not pin down.
    fn vec_rel(a: &[f64], b: &[f64]) -> f64 {
        let scale = b.iter().filter(|x| !x.is_nan()).fold(0.0f64, |m, x| m.max(x.abs()));
        let diff = a.iter().zip(b).filter(|(_, y)| !y.is_nan()).fold(0.0f64, |m, (x, y)| m.max((x - y).abs()));
        if scale < 1e-300 {
            if diff < 1e-300 { 0.0 } else { f64::INFINITY }
        } else {
            diff / scale
        }
    }
    match (a, b) {
        (Out::D2(x), Out::D2(y)) if x.len() == y.len() => {
            let fx: Vec<f64> = x.iter().flat_map(|v| v.iter().copied()).collect();
            let fy: Vec<f64> = y.iter().flat_map(|v| v.iter().copied()).collect();
            Some(vec_rel(&fx, &fy))
        }
        (Out::D3(x), Out::D3(y)) if x.len() == y.len() => {
            let fx: Vec<f64> = x.iter().flat_map(|v| v.iter().copied()).collect();
            let fy: Vec<f64> = y.iter().flat_map(|v| v.iter().copied()).collect();
            Some(vec_rel(&fx, &fy))
        }
        (Out::S(x, _), Out::S(y, _)) if x.len() == y.len() => {
            // Each scalar against its own magnitude: frequencies, load factors.
            Some(x.iter().zip(y).fold(0.0f64, |m, (p, q)| {
                let s = q.abs().max(1e-12);
                m.max((p - q).abs() / s)
            }))
        }
        _ => None,
    }
}

fn any_nonfinite(o: &Out) -> bool {
    match o {
        Out::D2(v) => v.iter().flatten().any(|x| !x.is_finite()),
        Out::D3(v) => v.iter().flatten().any(|x| !x.is_finite()),
        Out::S(v, _) => v.iter().any(|x| !x.is_finite()),
    }
}

/// Run an analysis, turning panics into errors.
fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            Err(format!("PANIC: {msg}"))
        }
    }
}

// ==================== verdicts ====================

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Verdict {
    NotApplicable,
    Supported,
    Refused,
    /// The feature changes the answer, but there is no independent reference.
    Responds,
    /// Error that does not name the feature, or no way to compare.
    Unclear,
    Ignored,
    Wrong,
}

impl Verdict {
    fn symbol(self) -> &'static str {
        match self {
            Verdict::Supported => "✓",
            Verdict::Refused => "R",
            Verdict::NotApplicable => "·",
            Verdict::Responds => "~",
            Verdict::Unclear => "E",
            Verdict::Ignored => "IGN",
            Verdict::Wrong => "BAD",
        }
    }
}

#[derive(Clone, Debug)]
struct Cell {
    verdict: Verdict,
    note: String,
}

// ==================== 2D model builder ====================

#[derive(Clone)]
struct M2 {
    s: SolverInput,
    /// section id → (b, h) of the rectangle it was built from (fiber/plastic data).
    rects: HashMap<usize, (f64, f64)>,
}

impl M2 {
    fn new() -> Self {
        let mut m = M2 {
            s: SolverInput {
                nodes: HashMap::new(),
                materials: HashMap::new(),
                sections: HashMap::new(),
                elements: HashMap::new(),
                supports: HashMap::new(),
                loads: vec![],
                constraints: vec![],
                connectors: HashMap::new(),
                solver_options: None,
            },
            rects: HashMap::new(),
        };
        m.mat(1, E);
        m.mat(STUB_MAT, E);
        m.sec(1, 0.2, 0.4);
        m
    }
    fn mat(&mut self, id: usize, e: f64) -> &mut Self {
        self.s.materials.insert(id.to_string(), SolverMaterial { id, e, nu: NU });
        self
    }
    fn sec(&mut self, id: usize, b: f64, h: f64) -> &mut Self {
        self.s.sections.insert(id.to_string(), SolverSection { id, a: b * h, iz: b * h.powi(3) / 12.0, as_y: None });
        self.rects.insert(id, (b, h));
        self
    }
    fn node(&mut self, id: usize, x: f64, z: f64) -> &mut Self {
        self.s.nodes.insert(id.to_string(), SolverNode { id, x, z });
        self
    }
    fn elem(&mut self, id: usize, ty: &str, ni: usize, nj: usize, mat: usize, sec: usize) -> &mut Self {
        self.s.elements.insert(
            id.to_string(),
            SolverElement {
                id, elem_type: ty.into(), node_i: ni, node_j: nj, material_id: mat, section_id: sec,
                hinge_start: false, hinge_end: false,
            },
        );
        self
    }
    fn frame(&mut self, id: usize, ni: usize, nj: usize) -> &mut Self {
        self.elem(id, "frame", ni, nj, 1, 1)
    }
    /// Nodes `ids` evenly from `a` to `b`, and frames between consecutive ones
    /// numbered from `first_elem`.
    fn line(&mut self, ids: &[usize], a: (f64, f64), b: (f64, f64), first_elem: usize) -> &mut Self {
        let n = ids.len() - 1;
        for (k, &id) in ids.iter().enumerate() {
            let t = k as f64 / n as f64;
            self.node(id, a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
        }
        for k in 0..n {
            self.frame(first_elem + k, ids[k], ids[k + 1]);
        }
        self
    }
    fn sup(&mut self, id: usize, node: usize, ty: &str) -> &mut Self {
        self.s.supports.insert(
            id.to_string(),
            SolverSupport {
                id, node_id: node, support_type: ty.into(),
                kx: None, ky: None, kz: None, dx: None, dz: None, dry: None, angle: None,
            },
        );
        self
    }
    fn sup_mut(&mut self, id: usize) -> &mut SolverSupport {
        self.s.supports.get_mut(&id.to_string()).unwrap()
    }
    fn elem_mut(&mut self, id: usize) -> &mut SolverElement {
        self.s.elements.get_mut(&id.to_string()).unwrap()
    }
    fn nodal(&mut self, node: usize, fx: f64, fz: f64, my: f64) -> &mut Self {
        self.s.loads.push(SolverLoad::Nodal(SolverNodalLoad { node_id: node, fx, fz, my }));
        self
    }
    fn load(&mut self, l: SolverLoad) -> &mut Self {
        self.s.loads.push(l);
        self
    }
    fn densities(&self) -> HashMap<String, f64> {
        self.s
            .materials
            .values()
            .map(|m| (m.id.to_string(), if m.id >= RIGID_MAT { 0.0 } else { RHO }))
            .collect()
    }
    /// Rigidly rotate the model by `phi` in the x–z plane (nodes and nodal loads).
    fn rotated(&self, phi: f64) -> M2 {
        let mut m = self.clone();
        for n in m.s.nodes.values_mut() {
            let r = rot2([n.x, n.z, 0.0], phi);
            n.x = r[0];
            n.z = r[1];
        }
        for l in m.s.loads.iter_mut() {
            match l {
                SolverLoad::Nodal(nl) => {
                    let r = rot2([nl.fx, nl.fz, 0.0], phi);
                    nl.fx = r[0];
                    nl.fz = r[1];
                }
                SolverLoad::Distributed(_) | SolverLoad::PointOnElement(_) | SolverLoad::Thermal(_) => {}
            }
        }
        m
    }
    /// A copy with an independent, fixed cantilever added far away, so the
    /// model crosses `SPARSE_THRESHOLD` and takes the sparse path. It shares no
    /// node with the structure, so the structure's answer is unchanged.
    fn padded(&self) -> M2 {
        let mut m = self.clone();
        let ids: Vec<usize> = (5001..=5030).collect();
        m.line(&ids, (100.0, 0.0), (100.0, 6.0), 5001);
        let sid = 5001;
        m.sup(sid, 5001, "fixed");
        m
    }
}

fn disp2(r: &AnalysisResults, probes: &[usize]) -> Result<Out, String> {
    let mut v = Vec::new();
    for &p in probes {
        let d = r
            .displacements
            .iter()
            .find(|d| d.node_id == p)
            .ok_or_else(|| format!("no displacement for probe node {p}"))?;
        v.push([d.ux, d.uz, d.ry]);
    }
    Ok(Out::D2(v))
}

// ==================== 2D cases ====================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Static,
    Modal,
    Buckling,
    Dynamic,
    Reduction,
    Plastic,
    Kinematic,
    Moving,
}

struct Case2 {
    feature: &'static str,
    variant: &'static str,
    /// Lower-case words an error message must contain to count as a refusal
    /// naming this feature.
    keywords: &'static [&'static str],
    /// A load (or imposed deformation): only analyses that read model loads see it.
    is_load: bool,
    with: M2,
    without: M2,
    equiv: Option<M2>,
    /// Rotation that takes `equiv` results back to `with` axes.
    equiv_rot: f64,
    probes: Vec<usize>,
    tol: f64,
    /// Kinds this variant is meant for; other kinds are covered by another variant.
    kinds: &'static [Kind],
    /// Analytical references for kinds without an equivalent model.
    analytic: Vec<(Kind, Out)>,
    /// Horizontal load path (element ids, in order) for moving loads / influence lines.
    path: Option<Vec<usize>>,
}

const ALL_KINDS: &[Kind] = &[
    Kind::Static, Kind::Modal, Kind::Buckling, Kind::Dynamic, Kind::Reduction, Kind::Plastic,
    Kind::Kinematic, Kind::Moving,
];
const STATIC_ONLY: &[Kind] = &[Kind::Static, Kind::Moving, Kind::Plastic];
/// Imposed deformations (temperature, settlement) do not change a plastic
/// collapse load, and their displacement-equivalent forces would.
const IMPOSED: &[Kind] = &[Kind::Static, Kind::Moving, Kind::Modal, Kind::Buckling, Kind::Dynamic, Kind::Reduction, Kind::Kinematic];

fn ei() -> f64 {
    E * 1000.0 * 0.2 * 0.4f64.powi(3) / 12.0
}

impl Case2 {
    fn base(feature: &'static str, variant: &'static str, keywords: &'static [&'static str], with: M2, without: M2, probes: Vec<usize>) -> Self {
        Case2 {
            feature, variant, keywords, is_load: false, with, without, equiv: None, equiv_rot: 0.0, probes,
            tol: 2e-3, kinds: ALL_KINDS, analytic: vec![], path: None,
        }
    }
}

/// Horizontal beam of `n` frames from node 1 at x = 0 to node n+1 at x = l.
fn beam(n: usize, l: f64) -> M2 {
    let mut m = M2::new();
    let ids: Vec<usize> = (1..=n + 1).collect();
    m.line(&ids, (0.0, 0.0), (l, 0.0), 1);
    m
}

fn cases_2d() -> Vec<Case2> {
    let mut cases = Vec::new();

    // ---- baseline: the plain frame every equivalent model is built from ----
    {
        let mut m = M2::new();
        let ids: Vec<usize> = (1..=9).collect();
        m.line(&ids, (0.0, 0.0), (0.0, 4.0), 1);
        m.sup(1, 1, "fixed");
        m.nodal(9, 10.0, -100.0, 0.0);
        let l: f64 = 4.0;
        let f1 = 1.875104f64.powi(2) / (2.0 * std::f64::consts::PI) * (ei() * 1000.0 / (RHO * 0.08 * l.powi(4))).sqrt();
        let pcr = std::f64::consts::PI.powi(2) * ei() / (4.0 * l * l);
        let mut c = Case2::base("baseline frame", "cantilever column", &[], m.clone(), m, vec![9, 5]);
        let ea = E * 1000.0 * 0.08;
        let f = [l.powi(3) / (3.0 * ei()), l / ea, l / ei(), l * l / (2.0 * ei())];
        let det = f[0] * f[2] - f[3] * f[3];
        let k = [f[2] / det, 0.0, -f[3] / det, 0.0, 1.0 / f[1], 0.0, -f[3] / det, 0.0, f[0] / det];
        let mp = 250e3 * 0.2 * 0.4 * 0.4 / 4.0;
        c.analytic = vec![
            (Kind::Modal, Out::S(vec![f1], true)),
            (Kind::Buckling, Out::S(vec![pcr / 100.0], true)),
            (Kind::Reduction, Out::S(invariants(&k, 3), true)),
            (Kind::Plastic, Out::S(vec![mp / (10.0 * l)], true)),
            (Kind::Kinematic, Out::S(vec![0.0, 1.0], true)),
            (Kind::Static, Out::D2(vec![
                [10.0 * l.powi(3) / (3.0 * ei()), -100.0 * l / ea, f64::NAN],
                [10.0 * 4.0 * (3.0 * l - 2.0) / (6.0 * ei()), -50.0 * l / ea, f64::NAN],
            ])),
        ];
        c.tol = 5e-3;
        cases.push(c);
    }

    // ---- truss members ----
    {
        // Column pinned at the base, braced at the top by a fat horizontal
        // member: as a truss it only props the top; as a frame it also
        // restrains the top's rotation. Equivalent: a frame brace released at
        // both ends (its far node fixed so that node's rotation is held).
        let mk = |ty: &str, hinged: bool| {
            let mut m = M2::new();
            m.sec(2, 0.3, 0.6);
            let ids: Vec<usize> = (1..=9).collect();
            m.line(&ids, (0.0, 0.0), (0.0, 4.0), 1);
            m.node(20, 3.0, 4.0);
            m.elem(20, ty, 9, 20, 1, 2);
            if hinged {
                m.elem_mut(20).hinge_start = true;
                m.elem_mut(20).hinge_end = true;
            }
            m.sup(1, 1, "pinned").sup(2, 20, if hinged { "fixed" } else { "pinned" });
            m.nodal(9, 20.0, -300.0, 0.0).nodal(5, 5.0, 0.0, 0.0);
            m
        };
        let mut c = Case2::base("truss element", "braced column", &["truss"], mk("truss", false), mk("frame", false), vec![9, 5]);
        c.equiv = Some(mk("frame", true));
        cases.push(c);
    }

    // ---- Timoshenko shear area ----
    {
        // Deep, short cantilever: shear deformation is ~25% of the tip deflection.
        let canti = |as_y: Option<f64>| {
            let mut m = M2::new();
            m.sec(1, 0.3, 1.0);
            m.s.sections.get_mut("1").unwrap().as_y = as_y;
            let ids: Vec<usize> = (1..=9).collect();
            m.line(&ids, (0.0, 0.0), (0.0, 3.0), 1);
            m.sup(1, 1, "fixed");
            m.nodal(9, 100.0, -50.0, 0.0);
            m
        };
        // Analytical tip and mid-height translations (rotations not pinned down:
        // NaN), and the tip's 3×3 condensed stiffness for the reductions.
        let (l, a, iz, asy) = (3.0f64, 0.3, 0.3 / 12.0, 0.25 * 0.3);
        let (eiv, ea, gas) = (E * 1000.0 * iz, E * 1000.0 * a, E * 1000.0 / (2.0 * (1.0 + NU)) * asy);
        let (hx, pz) = (100.0, -50.0);
        let ux = |x: f64| hx * x * x * (3.0 * l - x) / (6.0 * eiv) + hx * x / gas;
        let stat = Out::D2(vec![[ux(l), pz * l / ea, f64::NAN], [ux(l / 2.0), pz * l / 2.0 / ea, f64::NAN]]);
        let f = [l.powi(3) / (3.0 * eiv) + l / gas, l / ea, l / eiv, l * l / (2.0 * eiv)];
        // K = F⁻¹ for F = [[f0, 0, f3], [0, f1, 0], [f3, 0, f2]].
        let det = f[0] * f[2] - f[3] * f[3];
        let k = [f[2] / det, 0.0, -f[3] / det, 0.0, 1.0 / f[1], 0.0, -f[3] / det, 0.0, f[0] / det];
        let mut c = Case2::base("Timoshenko shear area", "deep cantilever", &["timoshenko", "shear"], canti(Some(asy)), canti(None), vec![9, 5]);
        c.analytic = vec![(Kind::Static, stat), (Kind::Reduction, Out::S(invariants(&k, 3), true))];
        c.kinds = &[Kind::Static, Kind::Reduction, Kind::Dynamic, Kind::Plastic, Kind::Kinematic, Kind::Moving];
        cases.push(c);

        // Simply supported beam with a small shear area (shear lowers the first
        // frequency and the buckling load by ~20%, rotary inertia < 1%):
        //   ω² = ω_E² / (1 + π²EI/(L²GAs)),  P_cr between Engesser and Haringx.
        let (l, b, h) = (6.0f64, 0.2f64, 0.6f64);
        let (a, iz) = (b * h, b * h.powi(3) / 12.0);
        let asy = 0.1 * a;
        let ss = |as_y: Option<f64>| {
            let mut m = M2::new();
            m.sec(1, b, h);
            m.s.sections.get_mut("1").unwrap().as_y = as_y;
            let ids: Vec<usize> = (1..=13).collect();
            m.line(&ids, (0.0, 0.0), (l, 0.0), 1);
            m.sup(1, 1, "pinned").sup(2, 13, "rollerX");
            m.nodal(13, -100.0, 0.0, 0.0).nodal(7, 0.0, -10.0, 0.0);
            m
        };
        let (eiv, gas) = (E * 1e6 * iz, E * 1e6 / (2.0 * (1.0 + NU)) * asy);
        let pi2 = std::f64::consts::PI.powi(2);
        let shear = pi2 * eiv / (l * l * gas);
        let f_e = pi2 / (2.0 * std::f64::consts::PI * l * l) * (eiv / (RHO * a)).sqrt();
        let f1 = f_e / (1.0 + shear).sqrt();
        let pe = pi2 * eiv / (l * l) / 1000.0; // kN
        let engesser = pe / (1.0 + shear);
        let haringx = gas / 1000.0 * ((1.0 + 4.0 * shear).sqrt() - 1.0) / 2.0;
        let mut c = Case2::base("Timoshenko shear area", "simply supported (eigen)", &["timoshenko", "shear"], ss(Some(asy)), ss(None), vec![7]);
        c.analytic = vec![
            (Kind::Modal, Out::S(vec![f1], true)),
            (Kind::Buckling, Out::S(vec![0.5 * (engesser + haringx) / 100.0], true)),
        ];
        c.tol = 0.02;
        c.kinds = &[Kind::Modal, Kind::Buckling];
        cases.push(c);
    }

    // ---- hinge (moment release) at a member end ----
    {
        // Beam clamped at x = 0 (sliding axially so it can be compressed), fixed at x = 6;
        // the last member is released into the right support = a pinned support there.
        let mk = |hinge: bool, pinned: bool| {
            let mut m = beam(12, 6.0);
            m.sup(1, 1, "guidedX").sup(2, 13, if pinned { "pinned" } else { "fixed" });
            if hinge {
                m.elem_mut(12).hinge_end = true;
            }
            m.nodal(4, 0.0, -40.0, 0.0).nodal(10, 0.0, -40.0, 0.0).nodal(1, 200.0, 0.0, 0.0);
            m
        };
        // Clamped–pinned beam: β₁L = 3.92660, P_cr = 20.1907·EI/L².
        let l: f64 = 6.0;
        let f1 = 3.92660f64.powi(2) / (2.0 * std::f64::consts::PI) * (ei() * 1000.0 / (RHO * 0.08 * l.powi(4))).sqrt();
        let pcr = 20.1907 * ei() / (l * l);
        let mut c = Case2::base("hinge (moment release)", "released into support", &["hinge", "release"], mk(true, false), mk(false, false), vec![5, 11]);
        c.equiv = Some(mk(false, true));
        c.analytic = vec![(Kind::Modal, Out::S(vec![f1], true)), (Kind::Buckling, Out::S(vec![pcr / 200.0], true))];
        c.tol = 3e-3;
        cases.push(c);
    }

    // ---- translational spring support ----
    {
        // Cantilever propped by a vertical spring at the tip; equivalent: a stub
        // member of the same axial stiffness to a pinned node.
        let k = 2_000.0;
        let mk = |spring: bool, stub: bool| {
            let mut m = beam(6, 6.0);
            m.sup(1, 1, "fixed");
            if spring {
                m.sup(2, 7, "spring");
                m.sup_mut(2).ky = Some(k);
            }
            if stub {
                stub_2d(&mut m, 7, (0.0, -1.0), k, 2);
            }
            m.nodal(4, 0.0, -30.0, 0.0).nodal(7, -300.0, 0.0, 0.0);
            m
        };
        let mut c = Case2::base("spring support (translational)", "propped cantilever", &["spring"], mk(true, false), mk(false, false), vec![7, 4]);
        c.equiv = Some(mk(false, true));
        cases.push(c);
    }

    // ---- rotational spring support ----
    {
        let kr = 50_000.0;
        let mk = |spring: bool, stub: bool| {
            let mut m = beam(6, 6.0);
            m.sup(1, 1, "pinned").sup(2, 7, "rollerX");
            if spring {
                m.sup_mut(1).kz = Some(kr);
            }
            if stub {
                // Stub from node 1 to a fixed node 0.5 m to the left: 4EI/ℓ = kr
                // (its translations are held by the pinned support).
                let l = 0.5;
                let i = kr * l / (4.0 * E * 1000.0);
                let h: f64 = 0.1;
                let b = 12.0 * i / h.powi(3);
                m.sec(3, b, h);
                m.node(100, -l, 0.0);
                m.elem(100, "frame", 100, 1, STUB_MAT, 3);
                m.sup(100, 100, "fixed");
            }
            m.nodal(3, 0.0, -30.0, 0.0).nodal(7, -200.0, 0.0, 0.0);
            m
        };
        let mut c = Case2::base("spring support (rotational)", "spring-restrained beam", &["spring"], mk(true, false), mk(false, false), vec![4, 1]);
        c.equiv = Some(mk(false, true));
        cases.push(c);
    }

    // ---- angled spring support ----
    {
        // Spring with local axes at 30°: kx along (cos30, sin30), ky across.
        // Equivalent: the model rotated by −30° with an axis-aligned spring.
        let alpha = 30f64.to_radians();
        let mk = |angle: Option<f64>| {
            let mut m = beam(6, 6.0);
            m.sup(1, 1, "fixed").sup(2, 7, "spring");
            m.sup_mut(2).kx = Some(20_000.0);
            m.sup_mut(2).ky = Some(1_000.0);
            m.sup_mut(2).angle = angle;
            m.nodal(4, 0.0, -30.0, 0.0).nodal(7, -100.0, 20.0, 0.0);
            m
        };
        let mut c = Case2::base("angled spring support", "spring at 30°", &["spring", "angle"], mk(Some(alpha)), mk(None), vec![4, 7]);
        c.equiv = Some(mk(None).rotated(-alpha));
        c.equiv_rot = alpha;
        cases.push(c);
    }

    // ---- inclined roller ----
    {
        // Normal (sin θ, cos θ) restrained; equivalent: model rotated by θ, rollerX.
        let theta = 30f64.to_radians();
        let mk = |ty: &str, angle: Option<f64>| {
            let mut m = beam(6, 6.0);
            m.sup(1, 1, "fixed").sup(2, 7, ty);
            m.sup_mut(2).angle = angle;
            m.nodal(4, 0.0, -30.0, 0.0).nodal(7, -150.0, 0.0, 0.0);
            m
        };
        let mut c = Case2::base("inclined support", "roller at 30°", &["inclined"], mk("inclinedRoller", Some(theta)), mk("rollerX", None), vec![4, 7]);
        c.equiv = Some(mk("rollerX", None).rotated(theta));
        c.equiv_rot = -theta;
        cases.push(c);
    }

    // ---- prescribed displacement ----
    {
        // Propped cantilever, right support settles δ; equivalent: the right end
        // free vertically and loaded by the propping reaction R = 3EIδ/L³.
        let delta = -0.01;
        let l: f64 = 6.0;
        // Propping reaction: settlement part plus the 5 kN load at x = 4.
        let (p, a) = (5.0, 4.0);
        let r = 3.0 * ei() * delta / l.powi(3) + p * a * a * (3.0 * l - a) / (2.0 * l.powi(3));
        let mk = |settle: bool, equiv: bool| {
            let mut m = beam(6, l);
            m.sup(1, 1, "fixed");
            m.nodal(4, 0.0, -5.0, 0.0);
            if equiv {
                m.sup(2, 7, "rollerZ");
                m.nodal(7, 0.0, r, 0.0);
            } else {
                m.sup(2, 7, "pinned");
                if settle {
                    m.sup_mut(2).dz = Some(delta);
                }
            }
            m
        };
        let mut c = Case2::base("prescribed displacement", "support settlement", &["prescribed", "settlement", "displacement"], mk(true, false), mk(false, false), vec![4, 7]);
        c.equiv = Some(mk(false, true));
        c.is_load = true;
        c.kinds = IMPOSED;
        cases.push(c);
    }

    // ---- thermal load ----
    {
        // Free cantilever, uniform ΔT and gradient on every member: equivalent
        // to an axial force and an end moment at the tip.
        let (dtu, dtg) = (30.0, 20.0);
        let (a, iz) = (0.08, 0.2 * 0.4f64.powi(3) / 12.0);
        let alpha = 12e-6;
        let h = (12.0 * iz / a).sqrt();
        let n = E * 1000.0 * a * alpha * dtu;
        let mth = E * 1000.0 * iz * alpha * dtg / h;
        let mk = |thermal: bool| {
            let mut m = beam(4, 4.0);
            m.sup(1, 1, "fixed");
            m.nodal(5, 0.0, -1.0, 0.0);
            if thermal {
                for e in 1..=4 {
                    m.load(SolverLoad::Thermal(SolverThermalLoad { element_id: e, dt_uniform: dtu, dt_gradient: dtg }));
                }
            }
            m
        };
        let mut eq = mk(false);
        eq.nodal(5, n, 0.0, mth);
        let mut c = Case2::base("thermal load", "free cantilever", &["thermal", "temperature"], mk(true), mk(false), vec![5, 3]);
        c.equiv = Some(eq);
        c.is_load = true;
        c.kinds = &[Kind::Static, Kind::Moving];
        cases.push(c);

        // Restrained bar: ΔT puts it in compression, which is what buckling sees.
        let mk = |thermal: bool, force: bool| {
            let mut m = beam(8, 6.0);
            m.sup(1, 1, "fixed").sup(2, 9, if force { "guidedX" } else { "fixed" });
            if thermal {
                for e in 1..=8 {
                    m.load(SolverLoad::Thermal(SolverThermalLoad { element_id: e, dt_uniform: dtu, dt_gradient: 0.0 }));
                }
            }
            if force {
                m.nodal(9, -n, 0.0, 0.0);
            }
            m.nodal(5, 0.0, -1.0, 0.0);
            m
        };
        let mut c = Case2::base("thermal load", "restrained bar (buckling)", &["thermal", "temperature"], mk(true, false), mk(false, false), vec![5]);
        c.equiv = Some(mk(false, true));
        c.is_load = true;
        c.kinds = &[Kind::Buckling];
        cases.push(c);
    }

    // ---- member loads: one 4 m member, against the member split at the load ----
    {
        let canti = |l: f64| {
            let mut m = M2::new();
            m.node(1, 0.0, 0.0).node(2, l, 0.0);
            m.frame(1, 1, 2);
            m.sup(1, 1, "fixed");
            m
        };
        // Partial uniform load on [1, 3].
        let mut w = canti(4.0);
        w.load(SolverLoad::Distributed(SolverDistributedLoad { element_id: 1, q_i: -20.0, q_j: -20.0, a: Some(1.0), b: Some(3.0) }));
        w.nodal(2, 0.0, -1.0, 0.0);
        let mut wo = canti(4.0);
        wo.load(SolverLoad::Distributed(SolverDistributedLoad { element_id: 1, q_i: -20.0, q_j: -20.0, a: None, b: None }));
        wo.nodal(2, 0.0, -1.0, 0.0);
        let mut eq = M2::new();
        eq.node(1, 0.0, 0.0).node(10, 1.0, 0.0).node(11, 3.0, 0.0).node(2, 4.0, 0.0);
        eq.frame(1, 1, 10).frame(2, 10, 11).frame(3, 11, 2);
        eq.sup(1, 1, "fixed");
        eq.load(SolverLoad::Distributed(SolverDistributedLoad { element_id: 2, q_i: -20.0, q_j: -20.0, a: None, b: None }));
        eq.nodal(2, 0.0, -1.0, 0.0);
        let mut c = Case2::base("partial distributed load", "cantilever", &["partial", "distributed"], w, wo, vec![2]);
        c.equiv = Some(eq);
        c.is_load = true;
        c.kinds = STATIC_ONLY;
        cases.push(c);

        // Trapezoidal load, against 40 sub-members with the mid-point intensity.
        let mut w = canti(4.0);
        w.load(SolverLoad::Distributed(SolverDistributedLoad { element_id: 1, q_i: -10.0, q_j: -40.0, a: None, b: None }));
        let mut wo = canti(4.0);
        wo.load(SolverLoad::Distributed(SolverDistributedLoad { element_id: 1, q_i: -10.0, q_j: -10.0, a: None, b: None }));
        let mut eq = M2::new();
        let nseg = 40;
        let ids: Vec<usize> = std::iter::once(1).chain(100..100 + nseg - 1).chain(std::iter::once(2)).collect();
        eq.line(&ids, (0.0, 0.0), (4.0, 0.0), 1);
        eq.sup(1, 1, "fixed");
        for k in 0..nseg {
            let t = (k as f64 + 0.5) / nseg as f64;
            let q = -10.0 - 30.0 * t;
            eq.load(SolverLoad::Distributed(SolverDistributedLoad { element_id: 1 + k, q_i: q, q_j: q, a: None, b: None }));
        }
        let mut c = Case2::base("trapezoidal distributed load", "cantilever", &["trapezoid", "distributed"], w, wo, vec![2]);
        c.equiv = Some(eq);
        c.is_load = true;
        c.kinds = STATIC_ONLY;
        c.tol = 2e-3;
        cases.push(c);

        // Point load (transverse + axial) and couple inside the member.
        let split = |nl: (f64, f64, f64)| {
            let mut eq = M2::new();
            eq.node(1, 0.0, 0.0).node(10, 1.5, 0.0).node(2, 4.0, 0.0);
            eq.frame(1, 1, 10).frame(2, 10, 2);
            eq.sup(1, 1, "fixed");
            eq.nodal(10, nl.0, nl.1, nl.2);
            eq.nodal(2, 0.0, -1.0, 0.0);
            eq
        };
        let mut w = canti(4.0);
        w.load(SolverLoad::PointOnElement(SolverPointLoadOnElement { element_id: 1, a: 1.5, p: -50.0, px: Some(30.0), my: None }));
        w.nodal(2, 0.0, -1.0, 0.0);
        let mut wo = canti(4.0);
        wo.nodal(2, 0.0, -1.0, 0.0);
        let mut c = Case2::base("point load on member", "cantilever", &["point"], w, wo.clone(), vec![2]);
        c.equiv = Some(split((30.0, -50.0, 0.0)));
        c.is_load = true;
        c.kinds = STATIC_ONLY;
        cases.push(c);

        let mut w = canti(4.0);
        w.load(SolverLoad::PointOnElement(SolverPointLoadOnElement { element_id: 1, a: 1.5, p: 0.0, px: None, my: Some(40.0) }));
        w.nodal(2, 0.0, -1.0, 0.0);
        let mut c = Case2::base("couple on member", "cantilever", &["couple", "moment"], w, wo, vec![2]);
        c.equiv = Some(split((0.0, 0.0, 40.0)));
        c.is_load = true;
        c.kinds = STATIC_ONLY;
        cases.push(c);
    }

    // ---- constraints: EqualDOF between coincident nodes = one node ----
    {
        let mk = |tie: bool, merged: bool| {
            let mut m = M2::new();
            // Column 1–2–3 and beam 3'–4–5 where 3' (id 30) coincides with 3.
            m.node(1, 0.0, 0.0).node(2, 0.0, 1.5).node(3, 0.0, 3.0);
            m.node(4, 2.0, 3.0).node(5, 4.0, 3.0);
            let j = if merged { 3 } else { 30 };
            if !merged {
                m.node(30, 0.0, 3.0);
            }
            m.frame(1, 1, 2).frame(2, 2, 3).frame(3, j, 4).frame(4, 4, 5);
            m.sup(1, 1, "fixed").sup(2, 5, "rollerX");
            if tie {
                m.s.constraints.push(Constraint::EqualDOF(EqualDOFConstraint { master_node: 3, slave_node: 30, dofs: vec![0, 1, 2] }));
            }
            m.nodal(3, 20.0, 0.0, 0.0).nodal(4, 0.0, -40.0, 0.0);
            m
        };
        // Without the tie the beam is a mechanism about node 5; give `without` a
        // support at 30 so it still solves (the answer is then very different).
        let mut wo = mk(false, false);
        wo.sup(3, 30, "pinned");
        let mut c = Case2::base("constraint (EqualDOF)", "tied coincident nodes", &["constraint", "equaldof", "equal dof", "mpc"], mk(true, false), wo, vec![4, 2]);
        c.equiv = Some(mk(false, true));
        cases.push(c);
    }

    // ---- constraints: rigid link = very stiff member ----
    {
        let mk = |link: bool, stiff: bool| {
            let mut m = M2::new();
            m.sec(4, 2.0, 2.0);
            m.mat(RIGID_MAT, E * 1e3);
            m.node(1, 0.0, 0.0).node(2, 0.0, 3.0).node(3, 1.0, 3.0).node(4, 4.0, 3.0);
            m.frame(1, 1, 2).frame(3, 3, 4);
            m.sup(1, 1, "fixed").sup(2, 4, "rollerX");
            if link {
                m.s.constraints.push(Constraint::RigidLink(RigidLinkConstraint { master_node: 2, slave_node: 3, dofs: vec![0, 1, 2] }));
            }
            if stiff {
                m.elem(2, "frame", 2, 3, RIGID_MAT, 4);
            }
            m.nodal(2, 20.0, 0.0, 0.0).nodal(3, 0.0, -40.0, 0.0);
            m
        };
        let mut wo = mk(false, false);
        wo.sup(3, 3, "pinned");
        let mut c = Case2::base("constraint (rigid link)", "offset beam", &["constraint", "rigid"], mk(true, false), wo, vec![2, 3]);
        c.equiv = Some(mk(false, true));
        c.tol = 2e-3;
        // The stiff member carries the rigid link's mass in a modal model.
        cases.push(c);
    }

    // ---- connector ----
    {
        // Axial connector from the tip of a cantilever to a pinned node.
        let k = 5_000.0;
        let mk = |conn: bool, stub: bool| {
            let mut m = beam(6, 6.0);
            m.sup(1, 1, "fixed");
            m.node(50, 6.0, -1.0);
            m.sup(2, 50, "fixed");
            if conn {
                m.s.connectors.insert(
                    "1".into(),
                    ConnectorElement { id: 1, node_i: 7, node_j: 50, k_axial: k, k_shear: 0.0, k_moment: 0.0, k_shear_z: 0.0, k_bend_y: 0.0, k_bend_z: 0.0 },
                );
            }
            if stub {
                stub_between_2d(&mut m, 7, 50, k, 2);
            }
            m.nodal(4, 0.0, -30.0, 0.0).nodal(7, -300.0, 0.0, 0.0);
            m
        };
        let mut c = Case2::base("connector", "axial connector", &["connector"], mk(true, false), mk(false, false), vec![7, 4]);
        c.equiv = Some(mk(false, true));
        cases.push(c);
    }

    // ---- moving loads / influence lines carry their own path ----
    for c in cases.iter_mut() {
        if c.path.is_none() && c.with.s.nodes.values().all(|n| n.z.abs() < 1e-12) && c.with.s.nodes.len() >= 7 {
            let mut ids: Vec<usize> = c
                .with
                .s
                .elements
                .values()
                .filter(|e| e.material_id != STUB_MAT)
                .map(|e| e.id)
                .collect();
            ids.sort();
            c.path = Some(ids);
        }
    }
    cases
}

/// A massless stub member from `node` along `dir` (unit) with axial stiffness
/// `k` = EA/ℓ, negligible bending, far end pinned: a translational spring.
fn stub_2d(m: &mut M2, node: usize, dir: (f64, f64), k: f64, sec_id: usize) {
    let far = 1000 + node;
    let n = m.s.nodes[&node.to_string()].clone();
    m.node(far, n.x + dir.0, n.z + dir.1);
    // Fixed, so the far node's rotation (which the hinged stub does not reach) is held.
    m.sup(far, far, "fixed");
    stub_between_2d(m, node, far, k, sec_id);
}

fn stub_between_2d(m: &mut M2, a: usize, b: usize, k: f64, sec_id: usize) {
    let na = m.s.nodes[&a.to_string()].clone();
    let nb = m.s.nodes[&b.to_string()].clone();
    let l = ((nb.x - na.x).powi(2) + (nb.z - na.z).powi(2)).sqrt();
    let area = k * l / (E * 1000.0);
    let h = 1e-3;
    m.sec(sec_id + 10, area / h, h);
    let id = 2000 + a;
    m.elem(id, "frame", a, b, STUB_MAT, sec_id + 10);
    m.elem_mut(id).hinge_start = true;
    m.elem_mut(id).hinge_end = true;
}

// ==================== 2D analyses ====================

struct Ana2 {
    name: &'static str,
    kind: Kind,
    /// Reads the model's static loads.
    uses_loads: bool,
    run: fn(&M2, &Case2) -> Result<Out, String>,
}

fn static_rows_2d() -> Vec<Ana2> {
    vec![
        Ana2 { name: "linear", kind: Kind::Static, uses_loads: true, run: |m, c| disp2(&linear::solve_2d(&m.s)?, &c.probes) },
        Ana2 { name: "linear (sparse path)", kind: Kind::Static, uses_loads: true, run: |m, c| disp2(&linear::solve_2d(&m.padded().s)?, &c.probes) },
        Ana2 {
            name: "constrained",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| {
                let ci = constraints::ConstrainedInput { solver: m.s.clone(), constraints: m.s.constraints.clone() };
                disp2(&constraints::solve_constrained_2d(&ci)?, &c.probes)
            },
        },
        Ana2 {
            name: "certified linear",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| disp2(&dedaliano_engine::certify::solver::certify_linear_2d_full(&m.s)?.results, &c.probes),
        },
        Ana2 {
            name: "load combinations",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| {
                let mut s = m.s.clone();
                let loads = std::mem::take(&mut s.loads);
                let inp = load_cases::MultiCaseInput {
                    solver: s,
                    load_cases: vec![load_cases::LoadCase { name: "D".into(), loads }],
                    combinations: vec![load_cases::CombinationDef { name: "C".into(), factors: [("D".to_string(), 1.0)].into() }],
                };
                let r = load_cases::solve_multi_case_2d(&inp)?;
                disp2(&r.combination_results[0].results, &c.probes)
            },
        },
        Ana2 { name: "P-Delta", kind: Kind::Static, uses_loads: true, run: |m, c| disp2(&conv(pdelta::solve_pdelta_2d(&m.s, 50, 1e-8).map(|r| (r.converged, r.results)))?, &c.probes) },
        Ana2 { name: "P-Delta (sparse path)", kind: Kind::Static, uses_loads: true, run: |m, c| disp2(&conv(pdelta::solve_pdelta_2d(&m.padded().s, 50, 1e-8).map(|r| (r.converged, r.results)))?, &c.probes) },
        Ana2 {
            name: "corotational",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| disp2(&conv(corotational::solve_corotational_2d(&m.s, 100, 1e-7, 4, false).map(|r| (r.converged, r.results)))?, &c.probes),
        },
        Ana2 {
            name: "corotational (sparse path)",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| disp2(&conv(corotational::solve_corotational_2d(&m.padded().s, 100, 1e-7, 4, false).map(|r| (r.converged, r.results)))?, &c.probes),
        },
        Ana2 { name: "material nonlinear", kind: Kind::Static, uses_loads: true, run: |m, c| disp2(&conv(matnl_2d(m).map(|r| (r.converged, r.results)))?, &c.probes) },
        Ana2 { name: "material nonlinear (sparse path)", kind: Kind::Static, uses_loads: true, run: |m, c| disp2(&conv(matnl_2d(&m.padded()).map(|r| (r.converged, r.results)))?, &c.probes) },
        Ana2 { name: "fiber nonlinear", kind: Kind::Static, uses_loads: true, run: |m, c| disp2(&conv(fiber_2d(m).map(|r| (r.converged, r.results)))?, &c.probes) },
        Ana2 {
            name: "arc length",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| {
                let inp = arc_length::ArcLengthInput {
                    solver: m.s.clone(), max_steps: 200, max_iter: 50, tolerance: 1e-8, initial_ds: 0.2, min_ds: 1e-6, max_ds: 0.5, target_iter: 5,
                };
                let r = arc_length::solve_arc_length(&inp)?;
                if !r.converged {
                    return Err("did not converge".into());
                }
                scale_out(disp2(&r.results, &c.probes)?, 1.0 / r.final_load_factor)
            },
        },
        Ana2 {
            name: "displacement control",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| {
                // Drive the probe's dominant DOF to its linear value: the load factor must come out 1.
                let lin = linear::solve_2d(&m.s)?;
                let Out::D2(v) = disp2(&lin, &c.probes)? else { unreachable!() };
                let (dof, &target) = v[0][..2].iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).unwrap();
                let inp = arc_length::DisplacementControlInput {
                    solver: m.s.clone(), control_node: c.probes[0], control_dof: dof, target_displacement: target, n_steps: 4, max_iter: 50, tolerance: 1e-8,
                };
                let r = arc_length::solve_displacement_control(&inp)?;
                if !r.converged {
                    return Err("did not converge".into());
                }
                scale_out(disp2(&r.results, &c.probes)?, 1.0 / r.final_load_factor)
            },
        },
        Ana2 {
            name: "staged (one stage)",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| {
                let s = &m.s;
                let inp = StagedInput {
                    nodes: s.nodes.clone(), materials: s.materials.clone(), sections: s.sections.clone(), elements: s.elements.clone(),
                    supports: s.supports.clone(), loads: s.loads.clone(), constraints: s.constraints.clone(),
                    stages: vec![ConstructionStage {
                        name: "all".into(),
                        elements_added: s.elements.values().map(|e| e.id).collect(),
                        elements_removed: vec![],
                        load_indices: (0..s.loads.len()).collect(),
                        supports_added: s.supports.values().map(|x| x.id).collect(),
                        supports_removed: vec![],
                        prestress_loads: vec![],
                    }],
                };
                disp2(&staged::solve_staged_2d(&inp)?.final_results, &c.probes)
            },
        },
        Ana2 {
            name: "Winkler (no foundation)",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| disp2(&winkler::solve_winkler_2d(&winkler::WinklerInput { solver: m.s.clone(), foundation_springs: vec![] })?, &c.probes),
        },
        Ana2 {
            name: "contact (no gaps)",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| {
                let inp: contact::ContactInput = serde_json::from_value(serde_json::json!({ "solver": m.s })).map_err(|e| e.to_string())?;
                disp2(&conv(contact::solve_contact_2d(&inp).map(|r| (r.converged, r.results)))?, &c.probes)
            },
        },
        Ana2 {
            name: "SSI (no soil springs)",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| disp2(&conv(ssi::solve_ssi_2d(&ssi::SSIInput { solver: m.s.clone(), soil_springs: vec![], max_iter: 50, tolerance: 1e-8 }).map(|r| (r.converged, r.results)))?, &c.probes),
        },
        Ana2 {
            name: "creep & shrinkage (t = t0)",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| {
                let inp = creep_shrinkage::CreepShrinkageInput {
                    solver: m.s.clone(), creep_params: HashMap::new(),
                    time_steps: vec![creep_shrinkage::TimeStep { t_days: 28.0, additional_loads: vec![] }], aging_coefficient: 0.8,
                };
                let r = creep_shrinkage::solve_creep_shrinkage_2d(&inp)?;
                let st = &r.steps[0];
                let mut v = Vec::new();
                for &p in &c.probes {
                    let d = st.displacements.iter().find(|d| d.node_id == p).ok_or("probe missing")?;
                    v.push([d.ux, d.uz, d.ry]);
                }
                Ok(Out::D2(v))
            },
        },
        Ana2 {
            name: "cable solver",
            kind: Kind::Static,
            uses_loads: true,
            run: |m, c| disp2(&conv(cable::solve_cable_2d(&m.s, &m.densities(), 50, 1e-8).map(|r| (r.converged, r.results)))?, &c.probes),
        },
    ]
}

fn other_rows_2d() -> Vec<Ana2> {
    vec![
        Ana2 {
            name: "modal",
            kind: Kind::Modal,
            uses_loads: false,
            run: |m, _| {
                let r = modal::solve_modal_2d(&m.s, &m.densities(), 3)?;
                Ok(Out::S(r.modes.iter().take(1).map(|x| x.frequency).collect(), true))
            },
        },
        Ana2 {
            name: "certified modal",
            kind: Kind::Modal,
            uses_loads: false,
            run: |m, _| {
                let r = dedaliano_engine::certify::modal::certify_modal_2d(&m.s, &m.densities(), 3)?;
                let v = serde_json::to_value(&r).map_err(|e| e.to_string())?;
                let f = v.pointer("/results/modes/0/frequency").or_else(|| v.pointer("/modal/modes/0/frequency")).and_then(|x| x.as_f64()).ok_or("no frequency in certified modal result")?;
                Ok(Out::S(vec![f], true))
            },
        },
        Ana2 {
            name: "buckling",
            kind: Kind::Buckling,
            uses_loads: true,
            run: |m, _| Ok(Out::S(vec![buckling::solve_buckling_2d(&m.s, 2)?.modes[0].load_factor], true)),
        },
        Ana2 {
            name: "time history",
            kind: Kind::Dynamic,
            uses_loads: false,
            run: |m, c| {
                let inp = TimeHistoryInput {
                    solver: m.s.clone(), densities: m.densities(), time_step: 0.002, n_steps: 120, method: "newmark".into(),
                    beta: 0.25, gamma: 0.5, alpha: None, damping_xi: Some(0.05), ground_accel: None, ground_direction: None,
                    force_history: Some(vec![TimeForceRecord { time: 0.0, loads: nodal_loads_2d(m) }, TimeForceRecord { time: 10.0, loads: nodal_loads_2d(m) }]),
                };
                let r = time_integration::solve_time_history_2d(&inp)?;
                let mut v = Vec::new();
                for &p in &c.probes {
                    let h = r.node_histories.iter().find(|h| h.node_id == p).ok_or("probe history missing")?;
                    for k in (10..h.ux.len()).step_by(10) {
                        v.push([h.ux[k], h.uz[k], h.ry[k]]);
                    }
                }
                Ok(Out::D2(v))
            },
        },
        Ana2 {
            name: "spectral",
            kind: Kind::Dynamic,
            uses_loads: false,
            run: |m, c| {
                let md = modal::solve_modal_2d(&m.s, &m.densities(), 4)?;
                let modes = md
                    .modes
                    .iter()
                    .map(|x| SpectralModeInput {
                        frequency: x.frequency, period: x.period, omega: x.omega,
                        displacements: x.displacements.iter().map(|d| SpectralModeDisp { node_id: d.node_id, ux: d.ux, uz: d.uz, ry: d.ry }).collect(),
                        participation_x: x.participation_x, participation_y: x.participation_y,
                        effective_mass_x: x.effective_mass_x, effective_mass_y: x.effective_mass_y,
                    })
                    .collect();
                let inp = SpectralInput {
                    solver: m.s.clone(), modes, densities: m.densities(),
                    spectrum: DesignSpectrum { name: "flat".into(), points: vec![SpectrumPoint { period: 0.0, sa: 0.3 }, SpectrumPoint { period: 10.0, sa: 0.3 }], in_g: Some(true) },
                    direction: "X".into(), rule: Some("SRSS".into()), xi: Some(0.05), importance_factor: None, reduction_factor: None, total_mass: None,
                };
                let r = spectral::solve_spectral_2d(&inp)?;
                let mut v = Vec::new();
                for &p in &c.probes {
                    let d = r.displacements.iter().find(|d| d.node_id == p).ok_or("probe missing")?;
                    v.extend([d.ux, d.uz]);
                }
                Ok(Out::S(v, false))
            },
        },
        Ana2 {
            name: "harmonic",
            kind: Kind::Dynamic,
            uses_loads: true,
            run: |m, c| {
                let mut out = Vec::new();
                let mut comps = Vec::new();
                for dof in ["x", "y"] {
                    let inp = harmonic::HarmonicInput {
                        solver: m.s.clone(), densities: m.densities(), frequencies: vec![0.0, 1.0], damping_ratio: 0.05,
                        response_node_id: c.probes[0], response_dof: dof.into(),
                    };
                    comps.push(harmonic::solve_harmonic_2d(&inp)?);
                }
                for k in 0..2 {
                    out.push([comps[0].response_points[k].real, comps[1].response_points[k].real, 0.0]);
                    out.push([comps[0].response_points[k].imag, comps[1].response_points[k].imag, 0.0]);
                }
                Ok(Out::D2(out))
            },
        },
        Ana2 {
            name: "Guyan reduction",
            kind: Kind::Reduction,
            uses_loads: false,
            run: |m, c| {
                let r = reduction::guyan_reduce_2d(&reduction::GuyanInput { solver: m.s.clone(), boundary_nodes: vec![c.probes[0]] })?;
                Ok(Out::S(invariants(&r.k_condensed, r.n_boundary), true))
            },
        },
        Ana2 {
            name: "Craig–Bampton",
            kind: Kind::Reduction,
            uses_loads: false,
            run: |m, c| {
                let r = reduction::craig_bampton_2d(&reduction::CraigBamptonInput {
                    solver: m.s.clone(), boundary_nodes: vec![c.probes[0]], n_modes: 2, densities: m.densities(),
                })?;
                // Boundary block of the reduced stiffness (the static condensation).
                let nb = r.n_boundary;
                let mut kb = vec![0.0; nb * nb];
                for i in 0..nb {
                    for j in 0..nb {
                        kb[i * nb + j] = r.k_reduced[i * r.n_reduced + j];
                    }
                }
                Ok(Out::S(invariants(&kb, nb), true))
            },
        },
        Ana2 {
            name: "plastic collapse",
            kind: Kind::Plastic,
            uses_loads: true,
            run: |m, _| {
                let sections = m
                    .s
                    .sections
                    .values()
                    .map(|s| {
                        let (b, h) = m.rects[&s.id];
                        let mat = m.s.elements.values().find(|e| e.section_id == s.id).map_or(1, |e| e.material_id);
                        (s.id.to_string(), PlasticSectionData { a: s.a, iz: s.iz, material_id: mat, b: Some(b), h: Some(h) })
                    })
                    .collect();
                let materials = m.s.materials.values().map(|x| (x.id.to_string(), PlasticMaterialData { fy: Some(if x.id >= RIGID_MAT { 1e9 } else { 250.0 }) })).collect();
                let r = plastic::solve_plastic_2d(&PlasticInput { solver: m.s.clone(), sections, materials, max_hinges: None, mp_overrides: None })?;
                Ok(Out::S(vec![r.collapse_factor], true))
            },
        },
        Ana2 {
            name: "kinematic check",
            kind: Kind::Kinematic,
            uses_loads: false,
            run: |m, _| {
                let r = kinematic::analyze_kinematics_2d(&m.s);
                if let Some(e) = r.invalid_input {
                    return Err(e);
                }
                Ok(Out::S(vec![r.mechanism_modes as f64, if r.is_solvable { 1.0 } else { 0.0 }], true))
            },
        },
        Ana2 {
            name: "moving loads",
            kind: Kind::Moving,
            uses_loads: true,
            run: |m, c| {
                let path = c.path.clone().ok_or("no path")?;
                let inp = MovingLoadInput {
                    solver: m.s.clone(),
                    train: LoadTrain { name: "one axle".into(), axles: vec![Axle { offset: 0.0, weight: 50.0 }] },
                    step: Some(0.5),
                    path_element_ids: Some(path.clone()),
                };
                let r = moving_loads::solve_moving_loads_2d(&inp)?;
                let mut v = Vec::new();
                for id in &path {
                    let e = &r.elements[&id.to_string()];
                    v.extend([e.m_max_pos, e.m_max_neg, e.v_max_pos, e.v_max_neg]);
                }
                Ok(Out::S(v, false))
            },
        },
        Ana2 {
            name: "influence line",
            kind: Kind::Moving,
            uses_loads: false,
            run: |m, c| {
                let inp = influence::InfluenceLineInput {
                    solver: m.s.clone(), quantity: "M".into(), target_node_id: None, target_element_id: Some(c.path.as_ref().ok_or("no path")?[1]),
                    target_position: 0.5, n_points_per_element: 4,
                };
                let r = influence::compute_influence_line(&inp)?;
                Ok(Out::S(il_values(&r, c), false))
            },
        },
    ]
}

/// Influence-line ordinates on the path, ordered by (element, t).
fn il_values(r: &influence::InfluenceLineResult, c: &Case2) -> Vec<f64> {
    let path = c.path.as_ref().unwrap();
    let mut pts: Vec<&influence::InfluenceLinePoint> = r.points.iter().filter(|p| path.contains(&p.element_id)).collect();
    pts.sort_by(|a, b| (a.element_id, (a.t * 1e6) as i64).cmp(&(b.element_id, (b.t * 1e6) as i64)));
    pts.iter().map(|p| p.value).collect()
}

/// Reference for moving loads / influence lines: the same loads, one position
/// at a time, through the linear solver on the full model.
fn moving_reference_2d(ana: &str, c: &Case2) -> Result<Out, String> {
    let path = c.path.clone().ok_or("no path")?;
    let m = &c.with;
    if ana == "moving loads" {
        let mut env: BTreeMap<usize, [f64; 4]> = path.iter().map(|&id| (id, [0.0; 4])).collect();
        let lens: Vec<f64> = path.iter().map(|id| elem_len_2d(m, *id)).collect();
        let total: f64 = lens.iter().sum();
        let mut pos = 0.0;
        while pos <= total + 1e-10 {
            let (mut acc, mut k) = (0.0, 0);
            while k + 1 < lens.len() && pos > acc + lens[k] + 1e-10 {
                acc += lens[k];
                k += 1;
            }
            let mut s = m.s.clone();
            s.loads.push(SolverLoad::PointOnElement(SolverPointLoadOnElement { element_id: path[k], a: (pos - acc).clamp(0.0, lens[k]), p: -50.0, px: None, my: None }));
            let r = linear::solve_2d(&s)?;
            for ef in &r.element_forces {
                if let Some(e) = env.get_mut(&ef.element_id) {
                    e[0] = e[0].max(ef.m_start.max(ef.m_end));
                    e[1] = e[1].min(ef.m_start.min(ef.m_end));
                    e[2] = e[2].max(ef.v_start.max(ef.v_end));
                    e[3] = e[3].min(ef.v_start.min(ef.v_end));
                }
            }
            pos += 0.5;
        }
        Ok(Out::S(path.iter().flat_map(|id| env[id]).collect(), false))
    } else {
        let target = path[1];
        let mut vals = Vec::new();
        for &eid in &path {
            let e = &m.s.elements[&eid.to_string()];
            let (ni, nj) = (&m.s.nodes[&e.node_i.to_string()], &m.s.nodes[&e.node_j.to_string()]);
            let l = elem_len_2d(m, eid);
            let (c_, s_) = ((nj.x - ni.x) / l, (nj.z - ni.z) / l);
            for k in 0..=4 {
                let t = k as f64 / 4.0;
                let mut s = m.s.clone();
                s.loads = influence::influence_unit_loads_2d(e, t * l, t, c_, s_);
                let r = linear::solve_2d(&s)?;
                vals.push(influence::extract_value("M", None, Some(target), 0.5, &r));
            }
        }
        Ok(Out::S(vals, false))
    }
}

fn elem_len_2d(m: &M2, id: usize) -> f64 {
    let e = &m.s.elements[&id.to_string()];
    let (a, b) = (&m.s.nodes[&e.node_i.to_string()], &m.s.nodes[&e.node_j.to_string()]);
    ((b.x - a.x).powi(2) + (b.z - a.z).powi(2)).sqrt()
}

/// A nonlinear result that did not converge is not an answer.
fn conv<T>(r: Result<(bool, T), String>) -> Result<T, String> {
    match r? {
        (true, t) => Ok(t),
        (false, _) => Err("did not converge".into()),
    }
}

fn scale_out(o: Out, f: f64) -> Result<Out, String> {
    if !f.is_finite() {
        return Err("non-finite load factor".into());
    }
    Ok(match o {
        Out::D2(v) => Out::D2(v.into_iter().map(|x| [x[0] * f, x[1] * f, x[2] * f]).collect()),
        Out::D3(v) => Out::D3(v.into_iter().map(|x| x.map(|y| y * f)).collect()),
        Out::S(v, i) => Out::S(v.into_iter().map(|x| x * f).collect(), i),
    })
}

fn nodal_loads_2d(m: &M2) -> Vec<SolverNodalLoad> {
    m.s.loads
        .iter()
        .filter_map(|l| if let SolverLoad::Nodal(n) = l { Some(n.clone()) } else { None })
        .collect()
}

/// Rotation invariants of a symmetric n×n matrix: trace, sum of principal 2×2
/// minors, determinant (n ≤ 3), each normalised so the comparison is relative.
fn invariants(k: &[f64], n: usize) -> Vec<f64> {
    let a = |i: usize, j: usize| k[i * n + j];
    let tr: f64 = (0..n).map(|i| a(i, i)).sum();
    let mut m2 = 0.0;
    for i in 0..n {
        for j in i + 1..n {
            m2 += a(i, i) * a(j, j) - a(i, j) * a(j, i);
        }
    }
    let det = if n == 3 {
        a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
            + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0))
    } else {
        0.0
    };
    vec![tr, m2, det]
}

fn matnl_2d(m: &M2) -> Result<NonlinearMaterialResult, String> {
    let inp = NonlinearMaterialInput {
        solver: m.s.clone(),
        material_models: m.s.materials.values().map(|x| (x.id.to_string(), MaterialModel { model_type: "elastic_perfectly_plastic".into(), fy: 1e9, alpha: None })).collect(),
        section_capacities: m.s.sections.values().map(|x| (x.id.to_string(), SectionCapacity { np: 1e12, mp: 1e12, zp: None })).collect(),
        max_iter: 50,
        tolerance: 1e-8,
        n_increments: 2,
    };
    material_nonlinear::solve_nonlinear_material_2d(&inp)
}

fn rect_fibers(b: f64, h: f64, ny: usize, nz: usize, e: f64) -> FiberSectionDef {
    let mut fibers = Vec::new();
    for i in 0..ny {
        for j in 0..nz {
            let y = -h / 2.0 + (i as f64 + 0.5) * h / ny as f64;
            let z = -b / 2.0 + (j as f64 + 0.5) * b / nz as f64;
            fibers.push(Fiber { y, z, area: b * h / (ny * nz) as f64, material_idx: 0 });
        }
    }
    FiberSectionDef { fibers, materials: vec![FiberMaterial::Elastic { e }] }
}

fn fiber_2d(m: &M2) -> Result<fiber_nonlinear::FiberNonlinearResult, String> {
    // The fiber model of a rectangle with n layers has I = bh³/12·(1 − 1/n²).
    let sections = m
        .s
        .sections
        .values()
        .map(|s| {
            let (b, h) = m.rects[&s.id];
            let e = m.s.elements.values().find(|e| e.section_id == s.id).map_or(E, |e| m.s.materials[&e.material_id.to_string()].e);
            (s.id.to_string(), rect_fibers(b, h, 200, 1, e))
        })
        .collect();
    let inp = fiber_nonlinear::FiberNonlinearInput {
        solver: m.s.clone(), fiber_sections: sections, n_integration_points: 5, max_iter: 50, tolerance: 1e-8, n_increments: 2, modified_nr: false,
    };
    fiber_nonlinear::solve_fiber_nonlinear_2d(&inp)
}

// ==================== classification ====================

fn names_feature(err: &str, keywords: &[&str]) -> bool {
    let e = err.to_lowercase();
    keywords.iter().any(|k| e.contains(k))
}

/// Classify one (case, analysis) pair, given closures that run the analysis.
#[allow(clippy::too_many_arguments)]
fn classify(
    keywords: &[&str],
    is_load: bool,
    uses_loads: bool,
    tol: f64,
    is_baseline: bool,
    run_with: Result<Out, String>,
    run_without: impl FnOnce() -> Result<Out, String>,
    reference: Result<(Out, String), String>,
) -> Cell {
    if is_load && !uses_loads {
        return Cell { verdict: Verdict::NotApplicable, note: "loads are not an input of this analysis".into() };
    }
    let with = match run_with {
        Ok(o) if any_nonfinite(&o) => return Cell { verdict: Verdict::Wrong, note: "non-finite result".into() },
        Ok(o) => o,
        Err(e) => {
            if matches!(&reference, Err(r) if r.contains(e.as_str())) {
                return Cell { verdict: Verdict::NotApplicable, note: format!("the reference fails the same way: {}", short(&e)) };
            }
            if !is_baseline && names_feature(&e, keywords) {
                return Cell { verdict: Verdict::Refused, note: short(&e) };
            }
            return Cell { verdict: Verdict::Unclear, note: format!("error: {}", short(&e)) };
        }
    };
    let (refv, refnote) = match reference {
        Ok(r) => r,
        Err(e) => {
            // No independent reference: all that can be said is whether the
            // feature reaches the answer at all.
            return match run_without().ok().and_then(|wo| rel(&with, &wo)) {
                Some(d) if d <= tol && !is_load && !is_baseline => Cell {
                    verdict: Verdict::Ignored,
                    note: format!("equals the model without the feature (no independent reference: {})", short(&e)),
                },
                Some(d) if d > tol => Cell { verdict: Verdict::Responds, note: format!("changes the answer by {d:.1e}; no independent reference") },
                _ => Cell { verdict: Verdict::Unclear, note: format!("no reference: {}", short(&e)) },
            };
        }
    };
    let without = if is_baseline { Err("baseline".to_string()) } else { run_without() };
    if let Ok(wo) = &without {
        if let Some(d) = rel(wo, &refv) {
            if d <= tol.max(1e-6) {
                return Cell { verdict: Verdict::NotApplicable, note: "feature does not change this analysis".into() };
            }
        }
    }
    let Some(d) = rel(&with, &refv) else {
        return Cell { verdict: Verdict::Unclear, note: "result not comparable with reference".into() };
    };
    if d <= tol {
        return Cell { verdict: Verdict::Supported, note: format!("matches {refnote} ({d:.1e})") };
    }
    if let Ok(wo) = &without {
        if let Some(dw) = rel(&with, wo) {
            if dw <= tol {
                return Cell { verdict: Verdict::Ignored, note: format!("equals the model without the feature; {refnote} differs by {d:.1e}") };
            }
        }
    }
    Cell { verdict: Verdict::Wrong, note: format!("differs from {refnote} by {d:.2e}") }
}

fn short(e: &str) -> String {
    let e = e.replace('\n', " ").replace('|', "/");
    if e.chars().count() > 110 { format!("{}…", e.chars().take(110).collect::<String>()) } else { e }
}

fn run_2d(c: &Case2, a: &Ana2) -> Cell {
    if !c.kinds.contains(&a.kind) {
        return Cell { verdict: Verdict::NotApplicable, note: "covered by another variant".into() };
    }
    if a.kind == Kind::Moving && c.path.is_none() {
        return Cell { verdict: Verdict::NotApplicable, note: "no horizontal load path in this model".into() };
    }
    let is_baseline = c.feature.starts_with("baseline");
    let run = |m: &M2| guarded(|| (a.run)(m, c));
    let with = run(&c.with);
    let rotate = |o: Out| match o {
        Out::D2(v) => Out::D2(v.into_iter().map(|x| rot2(x, c.equiv_rot)).collect()),
        o => o,
    };
    let reference = (|| -> Result<(Out, String), String> {
        if a.kind == Kind::Moving {
            return guarded(|| moving_reference_2d(a.name, c)).map(|o| (o, "linear solves position by position".into()));
        }
        if let Some((_, o)) = c.analytic.iter().find(|(k, _)| *k == a.kind) {
            return Ok((o.clone(), "the analytical value".into()));
        }
        if let Some(eq) = &c.equiv {
            let invariant_ok = c.equiv_rot == 0.0 || !matches!(guarded(|| (a.run)(eq, c)), Ok(Out::S(_, false)));
            if invariant_ok {
                match run(eq) {
                    Ok(o) => return Ok((rotate(o), "the equivalent model".into())),
                    Err(e) if a.kind != Kind::Static => return Err(format!("equivalent model failed: {e}")),
                    Err(_) => {}
                }
            }
        }
        if let Some((_, o)) = c.analytic.iter().find(|(k, _)| *k == a.kind) {
            return Ok((o.clone(), "the analytical value".into()));
        }
        if a.kind == Kind::Static {
            if a.name == "linear" && !is_baseline {
                return Err("linear has no equivalent model for this feature".into());
            }
            return guarded(|| disp2(&linear::solve_2d(&c.with.s)?, &c.probes)).map(|o| (o, "the linear solution".into()));
        }
        Err("no independent reference for this kind".into())
    })();
    classify(c.keywords, c.is_load, a.uses_loads, c.tol, is_baseline, with, || run(&c.without), reference)
}

// ==================== the matrix ====================

/// IGNORED / WRONG / unclear cells that are known and being fixed elsewhere:
/// (dimension, feature, analysis-prefix, where it is being fixed).
const KNOWN: &[(&str, &str, &str, &str)] = &[];

fn known_for(dim: &str, feature: &str, analysis: &str) -> Option<&'static str> {
    KNOWN.iter().find(|(d, f, a, _)| *d == dim && *f == feature && analysis.starts_with(a)).map(|k| k.3)
}

struct Matrix {
    dim: &'static str,
    features: Vec<&'static str>,
    analyses: Vec<&'static str>,
    /// (feature, analysis) → worst cell over the feature's variants, plus per-variant notes.
    cells: BTreeMap<(usize, usize), (Verdict, Vec<String>)>,
}

impl Matrix {
    fn new(dim: &'static str, analyses: Vec<&'static str>) -> Self {
        Matrix { dim, features: vec![], analyses, cells: BTreeMap::new() }
    }
    fn add(&mut self, feature: &'static str, variant: &str, analysis: usize, cell: Cell) {
        let fi = match self.features.iter().position(|f| *f == feature) {
            Some(i) => i,
            None => {
                self.features.push(feature);
                self.features.len() - 1
            }
        };
        let e = self.cells.entry((fi, analysis)).or_insert((Verdict::NotApplicable, vec![]));
        if cell.verdict != Verdict::NotApplicable || e.1.is_empty() {
            e.1.push(format!("{variant}: {} {}", cell.verdict.symbol(), cell.note));
        }
        e.0 = e.0.max(cell.verdict);
    }
}

fn render(ms: &[Matrix]) -> String {
    let mut o = String::new();
    o.push_str("# Feature × analysis support matrix\n\n");
    o.push_str("Generated by `engine/tests/feature_matrix.rs`; do not edit by hand. Regenerate with\n\n");
    o.push_str("```sh\ncd engine && UPDATE_FEATURE_MATRIX=1 cargo test --release --test feature_matrix\n```\n\n");
    o.push_str("Every analysis must either honour a model feature or refuse it by name. Each cell runs the\n");
    o.push_str("analysis on a small model where the feature matters, on the same model without the feature,\n");
    o.push_str("and on an independent reference (the same structure expressed without the feature, an\n");
    o.push_str("analytical value, or the linear solver in the linear limit).\n\n");
    o.push_str("| Symbol | Meaning |\n|---|---|\n");
    o.push_str("| ✓ | supported: matches the independent reference |\n");
    o.push_str("| R | refused with an error naming the feature |\n");
    o.push_str("| ~ | the feature changes the answer, but no independent reference exists for this analysis |\n");
    o.push_str("| · | not applicable: the feature does not change this analysis' answer |\n");
    o.push_str("| E | error that does not name the feature |\n");
    o.push_str("| IGN | silently ignored: the answer equals the model without the feature |\n");
    o.push_str("| BAD | wrong: matches neither |\n");
    o.push_str("| K | known IGN/BAD/E being fixed on another branch (see the notes) |\n\n");
    for m in ms {
        o.push_str(&format!("## {}\n\n", m.dim));
        o.push_str("| Analysis |");
        for (i, _) in m.features.iter().enumerate() {
            o.push_str(&format!(" F{} |", i + 1));
        }
        o.push_str("\n|---|");
        for _ in &m.features {
            o.push_str(":-:|");
        }
        o.push('\n');
        for (ai, a) in m.analyses.iter().enumerate() {
            o.push_str(&format!("| {a} |"));
            for fi in 0..m.features.len() {
                let (v, _) = &m.cells[&(fi, ai)];
                let sym = if *v >= Verdict::Unclear && known_for(m.dim, m.features[fi], a).is_some() { "K" } else { v.symbol() };
                o.push_str(&format!(" {sym} |"));
            }
            o.push('\n');
        }
        o.push_str("\nFeatures:\n\n");
        for (i, f) in m.features.iter().enumerate() {
            o.push_str(&format!("- **F{}** {}\n", i + 1, f));
        }
        let mut counts: BTreeMap<Verdict, usize> = BTreeMap::new();
        for (v, _) in m.cells.values() {
            *counts.entry(*v).or_default() += 1;
        }
        o.push_str("\nTotals: ");
        o.push_str(&counts.iter().map(|(v, n)| format!("{} {n}", v.symbol())).collect::<Vec<_>>().join(", "));
        o.push_str("\n\nNotes on every cell that is not ✓ or ·:\n\n");
        for ((fi, ai), (v, notes)) in &m.cells {
            if *v == Verdict::Supported || *v == Verdict::NotApplicable {
                continue;
            }
            let k = known_for(m.dim, m.features[*fi], m.analyses[*ai]).map(|k| format!(" — known, {k}")).unwrap_or_default();
            o.push_str(&format!("- {} × {}{}: {}\n", m.analyses[*ai], m.features[*fi], k, notes.join("; ")));
        }
        o.push('\n');
    }
    o
}

fn matrix_2d() -> Matrix {
    let cases = cases_2d();
    let anas: Vec<Ana2> = static_rows_2d().into_iter().chain(other_rows_2d()).collect();
    let mut m = Matrix::new("2D", anas.iter().map(|a| a.name).collect());
    for c in &cases {
        for (ai, a) in anas.iter().enumerate() {
            let cell = run_2d(c, a);
            m.add(c.feature, c.variant, ai, cell);
        }
    }
    m
}

#[test]
fn feature_matrix() {
    let ms = vec![matrix_2d()];
    let text = render(&ms);
    println!("{text}");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/verification/FEATURE_MATRIX.md");
    if std::env::var("UPDATE_FEATURE_MATRIX").is_ok() {
        std::fs::write(&path, &text).unwrap();
    }
    let mut bad = Vec::new();
    for m in &ms {
        for ((fi, ai), (v, notes)) in &m.cells {
            if *v >= Verdict::Unclear && known_for(m.dim, m.features[*fi], m.analyses[*ai]).is_none() {
                bad.push(format!("{} {} × {}: {}", m.dim, m.analyses[*ai], m.features[*fi], notes.join("; ")));
            }
        }
    }
    assert!(bad.is_empty(), "analyses that silently ignore, mishandle or fail on a feature:\n{}", bad.join("\n"));
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(on_disk == text, "docs/verification/FEATURE_MATRIX.md is out of date: rerun with UPDATE_FEATURE_MATRIX=1");
}

#[test]
#[ignore]
fn debug_probe() {
    let cases = cases_2d();
    let c = &cases[0];
    let lin = linear::solve_2d(&c.with.s).unwrap();
    println!("lin {:?}", disp2(&lin, &c.probes));
    for tol in [1e-10, 1e-6] {
        let r = corotational::solve_corotational_2d(&c.with.s, 100, tol, 4, false).unwrap();
        println!("corot tol {tol} conv {} it {} {:?}", r.converged, r.iterations, disp2(&r.results, &c.probes));
    }
    let inp = arc_length::ArcLengthInput { solver: c.with.s.clone(), max_steps: 200, max_iter: 50, tolerance: 1e-10, initial_ds: 0.2, min_ds: 1e-6, max_ds: 0.5, target_iter: 5 };
    let r = arc_length::solve_arc_length(&inp).unwrap();
    println!("arc conv {} lf {} steps {} {:?}", r.converged, r.final_load_factor, r.steps.len(), disp2(&r.results, &c.probes));
    let th = cases.iter().find(|c| c.feature == "thermal load").unwrap();
    println!("thermal with {:?}", disp2(&linear::solve_2d(&th.with.s).unwrap(), &th.probes));
    println!("thermal eq {:?}", disp2(&linear::solve_2d(&th.equiv.as_ref().unwrap().s).unwrap(), &th.probes));
}
