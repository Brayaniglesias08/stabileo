//! Large-model solve paths of the linear static solver (#339): models with at
//! least 64 free DOFs that force each path of `prepare_static_2d_impl` /
//! `PreparedStatic2D::solve_loads` and of `prepare_static_3d` /
//! `PreparedStatic3D::solve_loads_*` — sparse Cholesky, its dense-LU
//! fallbacks, the 3D diagonal-shift ladder, PCG with each preconditioner and
//! each fallback — and check the displacements and reactions against an
//! independent reference (a dense Lagrange-multiplier solve written here,
//! closed forms, or the same structure on the small dense path), and the exact
//! structured diagnostics each path emits.

use dedaliano_engine::solver::dof::DofNumbering;
use dedaliano_engine::solver::linear::{
    prepare_static_2d, prepare_static_2d_dense_reference, solve_2d,
};
use dedaliano_engine::types::*;
use serde_json::{json, Value};

const SINGULAR: &str = "Singular stiffness matrix — structure is a mechanism";
const LOCAL_MECH: &str = "Local mechanism detected";

// ==================== 2D model builder ====================

/// E in MPa as the engine takes it; stiffness uses E·1000 (kN/m²).
const E_MPA: f64 = 200_000.0;
const A: f64 = 0.01;
const IZ: f64 = 1e-4;

#[derive(Clone, Default)]
struct M2 {
    nodes: Vec<(usize, f64, f64)>,
    /// (id, type, i, j, hinge_start, hinge_end)
    elems: Vec<(usize, &'static str, usize, usize, bool, bool)>,
    sups: Vec<Value>,
    loads: Vec<Value>,
    connectors: Vec<Value>,
    e: Option<f64>,
    opts: Option<Value>,
}

impl M2 {
    fn node(&mut self, x: f64, z: f64) -> usize {
        let id = self.nodes.len() + 1;
        self.nodes.push((id, x, z));
        id
    }
    fn elem(&mut self, t: &'static str, i: usize, j: usize) -> usize {
        let id = self.elems.len() + 1;
        self.elems.push((id, t, i, j, false, false));
        id
    }
    fn sup(&mut self, node: usize, t: &str, extra: Value) {
        let mut s = json!({ "id": self.sups.len() + 1, "nodeId": node, "type": t });
        for (k, v) in extra.as_object().unwrap() {
            s[k] = v.clone();
        }
        self.sups.push(s);
    }
    fn load(&mut self, node: usize, fx: f64, fz: f64, my: f64) {
        self.loads.push(json!({ "type": "nodal", "data": { "nodeId": node, "fx": fx, "fz": fz, "my": my } }));
    }
    fn input(&self) -> SolverInput {
        let e = self.e.unwrap_or(E_MPA);
        let nodes: serde_json::Map<String, Value> = self
            .nodes
            .iter()
            .map(|&(id, x, z)| (id.to_string(), json!({ "id": id, "x": x, "z": z })))
            .collect();
        let elems: serde_json::Map<String, Value> = self
            .elems
            .iter()
            .map(|&(id, t, i, j, hs, he)| {
                (id.to_string(), json!({ "id": id, "type": t, "nodeI": i, "nodeJ": j, "materialId": 1, "sectionId": 1, "hingeStart": hs, "hingeEnd": he }))
            })
            .collect();
        let sups: serde_json::Map<String, Value> =
            self.sups.iter().map(|s| (s["id"].to_string(), s.clone())).collect();
        let conns: serde_json::Map<String, Value> =
            self.connectors.iter().map(|c| (c["id"].to_string(), c.clone())).collect();
        let mut v = json!({
            "nodes": nodes,
            "materials": { "1": { "id": 1, "e": e, "nu": 0.3 } },
            "sections": { "1": { "id": 1, "a": A, "iz": IZ } },
            "elements": elems,
            "supports": sups,
            "loads": self.loads,
            "connectors": conns,
        });
        if let Some(o) = &self.opts {
            v["solverOptions"] = o.clone();
        }
        serde_json::from_value(v).unwrap()
    }
}

/// Two-storey, `bays`-bay portal frame, each member split into `seg`
/// segments, offset from the origin (x from 2, z from 1) so the
/// characteristic length is a genuine diagonal. Supports: fixed, pinned, a
/// rollerX with a settlement, an inclined roller and an elastic spring
/// support, one at each column base. Nodal loads at the floor nodes.
/// Returns the builder and the ids of the column-base nodes.
fn portal(bays: usize, seg: usize) -> (M2, Vec<usize>) {
    let mut m = M2::default();
    let (x0, z0, w, h) = (2.0, 1.0, 4.0, 3.0);
    // grid nodes (column lines × levels)
    let mut grid = vec![vec![0usize; 3]; bays + 1];
    for (c, col) in grid.iter_mut().enumerate() {
        for (l, g) in col.iter_mut().enumerate() {
            *g = m.node(x0 + c as f64 * w, z0 + l as f64 * h);
        }
    }
    let member = |m: &mut M2, a: usize, b: usize| {
        let (_, xa, za) = m.nodes[a - 1];
        let (_, xb, zb) = m.nodes[b - 1];
        let mut prev = a;
        for k in 1..seg {
            let t = k as f64 / seg as f64;
            let n = m.node(xa + t * (xb - xa), za + t * (zb - za));
            m.elem("frame", prev, n);
            prev = n;
        }
        m.elem("frame", prev, b);
    };
    for c in 0..=bays {
        for l in 0..2 {
            member(&mut m, grid[c][l], grid[c][l + 1]);
        }
    }
    for c in 0..bays {
        for l in 1..3 {
            member(&mut m, grid[c][l], grid[c + 1][l]);
        }
    }
    let bases: Vec<usize> = grid.iter().map(|col| col[0]).collect();
    let kinds = ["fixed", "pinned", "rollerX", "inclinedRoller", "spring"];
    for (c, &b) in bases.iter().enumerate() {
        match kinds[c % kinds.len()] {
            "rollerX" => m.sup(b, "rollerX", json!({ "dz": -0.004 })),
            "inclinedRoller" => m.sup(b, "inclinedRoller", json!({ "angle": 0.5 })),
            "spring" => m.sup(b, "spring", json!({ "kx": 4.0e4, "ky": 9.0e4, "kz": 2.0e4 })),
            k => m.sup(b, k, json!({})),
        }
    }
    for c in 0..=bays {
        m.load(grid[c][1], 30.0 + 5.0 * c as f64, -80.0, 4.0);
        m.load(grid[c][2], 15.0, -120.0 - 10.0 * c as f64, -6.0);
    }
    (m, bases)
}

// ==================== Independent 2D reference ====================

/// Sign of the engine's rotation `ry` relative to the counter-clockwise
/// rotation (x toward z) the reference element is written in.
const RY_SIGN: f64 = 1.0;

struct Ref2D {
    /// node id → (ux, uz, ry)
    disp: std::collections::HashMap<usize, [f64; 3]>,
    /// node id → (rx, rz, my)
    reac: std::collections::HashMap<usize, [f64; 3]>,
    /// node id → 3×3 diagonal block of the reference K (global frame)
    kdiag: std::collections::HashMap<usize, [[f64; 3]; 3]>,
}

/// Dense reference solve of an `M2` model built from frames, trusses, nodal
/// loads, rigid supports (prescribed values included), inclined rollers and
/// springs: Euler-Bernoulli element matrices written out here, every support
/// as a Lagrange-multiplier constraint (no free/restrained partition), and
/// Gaussian elimination with partial pivoting. Hinges and connectors are not
/// modelled (those tests use closed forms).
fn reference_2d(m: &M2) -> Ref2D {
    let e = m.e.unwrap_or(E_MPA) * 1000.0;
    let idx: std::collections::HashMap<usize, usize> =
        m.nodes.iter().enumerate().map(|(k, n)| (n.0, k)).collect();
    let nd = 3 * m.nodes.len();
    // constraints: (dof coefficients, rhs)
    let mut cons: Vec<(Vec<(usize, f64)>, f64)> = Vec::new();
    let mut k = vec![0.0; nd * nd];
    let mut f = vec![0.0; nd];
    for &(_, t, i, j, hs, he) in &m.elems {
        assert!(!hs && !he, "reference_2d does not model hinges");
        let (_, xi, zi) = m.nodes[idx[&i]];
        let (_, xj, zj) = m.nodes[idx[&j]];
        let l = ((xj - xi).powi(2) + (zj - zi).powi(2)).sqrt();
        let (c, s) = ((xj - xi) / l, (zj - zi) / l);
        let ea = e * A / l;
        let ei = if t == "frame" { e * IZ } else { 0.0 };
        let (k1, k2, k3, k4) = (12.0 * ei / l.powi(3), 6.0 * ei / l.powi(2), 4.0 * ei / l, 2.0 * ei / l);
        let kl = [
            [ea, 0.0, 0.0, -ea, 0.0, 0.0],
            [0.0, k1, k2, 0.0, -k1, k2],
            [0.0, k2, k3, 0.0, -k2, k4],
            [-ea, 0.0, 0.0, ea, 0.0, 0.0],
            [0.0, -k1, -k2, 0.0, k1, -k2],
            [0.0, k2, k4, 0.0, -k2, k3],
        ];
        let mut tr = [[0.0; 6]; 6];
        for b in 0..2 {
            let o = 3 * b;
            tr[o][o] = c;
            tr[o][o + 1] = s;
            tr[o + 1][o] = -s;
            tr[o + 1][o + 1] = c;
            tr[o + 2][o + 2] = 1.0;
        }
        let dofs: Vec<usize> = (0..3).map(|d| 3 * idx[&i] + d).chain((0..3).map(|d| 3 * idx[&j] + d)).collect();
        for p in 0..6 {
            for q in 0..6 {
                let mut v = 0.0;
                for a in 0..6 {
                    for b in 0..6 {
                        v += tr[a][p] * kl[a][b] * tr[b][q];
                    }
                }
                k[dofs[p] * nd + dofs[q]] += v;
            }
        }
    }
    // Axial-only connectors: a spring of stiffness kAxial along the connector.
    for cn in &m.connectors {
        for key in ["kShear", "kMoment"] {
            assert_eq!(cn.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0), 0.0, "reference_2d models axial connectors only");
        }
        let ka = cn["kAxial"].as_f64().unwrap();
        let (i, j) = (idx[&(cn["nodeI"].as_u64().unwrap() as usize)], idx[&(cn["nodeJ"].as_u64().unwrap() as usize)]);
        let (_, xi, zi) = m.nodes[i];
        let (_, xj, zj) = m.nodes[j];
        let l = ((xj - xi).powi(2) + (zj - zi).powi(2)).sqrt();
        let g = [-(xj - xi) / l, -(zj - zi) / l, (xj - xi) / l, (zj - zi) / l];
        let dofs = [3 * i, 3 * i + 1, 3 * j, 3 * j + 1];
        for p in 0..4 {
            for q in 0..4 {
                k[dofs[p] * nd + dofs[q]] += ka * g[p] * g[q];
            }
        }
    }
    // An all-truss model has no rotation DOFs: hold the reference's at zero.
    let has_frame = m.elems.iter().any(|e| e.1 == "frame");
    if !has_frame {
        for n in 0..m.nodes.len() {
            cons.push((vec![(3 * n + 2, 1.0)], 0.0));
        }
    }
    let gv = |s: &Value, key: &str| s.get(key).and_then(|v| v.as_f64());
    let mut springs: Vec<(usize, [f64; 3])> = Vec::new();
    for s in &m.sups {
        let n = idx[&(s["nodeId"].as_u64().unwrap() as usize)];
        let d = |q: usize| 3 * n + q;
        let t = s["type"].as_str().unwrap();
        let (dx, dz, dry) = (gv(s, "dx").unwrap_or(0.0), gv(s, "dz").unwrap_or(0.0), gv(s, "dry").unwrap_or(0.0));
        match t {
            "fixed" => {
                cons.push((vec![(d(0), 1.0)], dx));
                cons.push((vec![(d(1), 1.0)], dz));
                cons.push((vec![(d(2), RY_SIGN)], dry));
            }
            "pinned" => {
                cons.push((vec![(d(0), 1.0)], dx));
                cons.push((vec![(d(1), 1.0)], dz));
            }
            "rollerX" => cons.push((vec![(d(1), 1.0)], dz)),
            "inclinedRoller" => {
                let th = gv(s, "angle").unwrap();
                // restrained direction (sin θ, cos θ)
                cons.push((vec![(d(0), th.sin()), (d(1), th.cos())], dx * th.sin() + dz * th.cos()));
            }
            "spring" => {
                let ks = [gv(s, "kx").unwrap_or(0.0), gv(s, "ky").unwrap_or(0.0), gv(s, "kz").unwrap_or(0.0)];
                for q in 0..3 {
                    k[d(q) * nd + d(q)] += ks[q];
                }
                springs.push((n, ks));
            }
            other => panic!("reference_2d: support type {other}"),
        }
    }
    for l in &m.loads {
        let dta = &l["data"];
        let n = idx[&(dta["nodeId"].as_u64().unwrap() as usize)];
        f[3 * n] += dta["fx"].as_f64().unwrap();
        f[3 * n + 1] += dta["fz"].as_f64().unwrap();
        f[3 * n + 2] += RY_SIGN * dta["my"].as_f64().unwrap();
    }
    let mut kdiag = std::collections::HashMap::new();
    for (id, &n) in &idx {
        let kn = |a: usize, b: usize| k[(3 * n + a) * nd + 3 * n + b];
        kdiag.insert(*id, [[kn(0, 0), kn(0, 1), kn(0, 2)], [kn(1, 0), kn(1, 1), kn(1, 2)], [kn(2, 0), kn(2, 1), kn(2, 2)]]);
    }
    // augmented system [K Cᵀ; C 0] [u; λ] = [f; d]
    let nc = cons.len();
    let na = nd + nc;
    let mut a = vec![0.0; na * na];
    let mut b = vec![0.0; na];
    for r in 0..nd {
        a[r * na..r * na + nd].copy_from_slice(&k[r * nd..r * nd + nd]);
        b[r] = f[r];
    }
    for (ci, (coef, rhs)) in cons.iter().enumerate() {
        for &(d, v) in coef {
            a[(nd + ci) * na + d] = v;
            a[d * na + nd + ci] = v;
        }
        b[nd + ci] = *rhs;
    }
    let x = gauss(a, b, na);
    let mut disp = std::collections::HashMap::new();
    for (id, &n) in &idx {
        disp.insert(*id, [x[3 * n], x[3 * n + 1], RY_SIGN * x[3 * n + 2]]);
    }
    // reaction on the structure = −Cᵀλ (in the reference rotation sense)
    let mut rv = vec![0.0; nd];
    for (ci, (coef, _)) in cons.iter().enumerate() {
        for &(d, v) in coef {
            rv[d] -= v * x[nd + ci];
        }
    }
    let mut reac = std::collections::HashMap::new();
    if !has_frame {
        for d in disp.values_mut() {
            d[2] = 0.0;
        }
    }
    for s in &m.sups {
        let id = s["nodeId"].as_u64().unwrap() as usize;
        let n = idx[&id];
        let mut r = [rv[3 * n], rv[3 * n + 1], if has_frame { RY_SIGN * rv[3 * n + 2] } else { 0.0 }];
        if let Some((_, ks)) = springs.iter().find(|(sn, _)| *sn == n) {
            let u = &disp[&id];
            for q in 0..3 {
                r[q] = -ks[q] * u[q];
            }
        }
        reac.insert(id, r);
    }
    Ref2D { disp, reac, kdiag }
}

/// Gaussian elimination with partial pivoting (dense, row-major).
fn gauss(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Vec<f64> {
    for col in 0..n {
        let p = (col..n).max_by(|&r, &s| a[r * n + col].abs().total_cmp(&a[s * n + col].abs())).unwrap();
        assert!(a[p * n + col].abs() > 0.0, "reference system singular");
        if p != col {
            for c in 0..n {
                a.swap(p * n + c, col * n + c);
            }
            b.swap(p, col);
        }
        let piv = a[col * n + col];
        for r in col + 1..n {
            let fct = a[r * n + col] / piv;
            if fct != 0.0 {
                for c in col..n {
                    a[r * n + c] -= fct * a[col * n + c];
                }
                b[r] -= fct * b[col];
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for c in r + 1..n {
            s -= a[r * n + c] * x[c];
        }
        x[r] = s / a[r * n + r];
    }
    x
}

/// Max relative difference of displacements and reactions against the
/// reference, scaled by the largest magnitude of each quantity.
fn assert_matches_reference_2d(r: &AnalysisResults, rf: &Ref2D, tol: f64, label: &str) {
    let scale = |v: Vec<f64>| v.iter().fold(0.0f64, |a, b| a.max(b.abs())).max(1e-9);
    let su = scale(rf.disp.values().flat_map(|d| [d[0], d[1]]).collect());
    let sr = scale(rf.disp.values().map(|d| d[2]).collect());
    assert_eq!(r.displacements.len(), rf.disp.len(), "{label}: displacement count");
    for d in &r.displacements {
        let e = rf.disp[&d.node_id];
        for (got, want, s, what) in [(d.ux, e[0], su, "ux"), (d.uz, e[1], su, "uz"), (d.ry, e[2], sr, "ry")] {
            assert!((got - want).abs() <= tol * s, "{label}: node {} {what} = {got:e}, reference {want:e}", d.node_id);
        }
    }
    let sf = scale(rf.reac.values().flat_map(|x| [x[0], x[1]]).collect());
    let sm = scale(rf.reac.values().map(|x| x[2]).collect());
    assert_eq!(r.reactions.len(), rf.reac.len(), "{label}: reaction count");
    for x in &r.reactions {
        let e = rf.reac[&x.node_id];
        for (got, want, s, what) in [(x.rx, e[0], sf, "rx"), (x.rz, e[1], sf, "rz"), (x.my, e[2], sm, "my")] {
            assert!((got - want).abs() <= tol * s, "{label}: reaction node {} {what} = {got:e}, reference {want:e}", x.node_id);
        }
    }
}

fn path_2d(r: &AnalysisResults) -> &str {
    &r.solver_run_meta.as_ref().unwrap().solver_path
}

fn codes(d: &[StructuredDiagnostic]) -> Vec<DiagnosticCode> {
    d.iter().map(|x| x.code).collect()
}

fn diag(r: &[StructuredDiagnostic], c: DiagnosticCode) -> &StructuredDiagnostic {
    let v: Vec<_> = r.iter().filter(|d| d.code == c).collect();
    assert_eq!(v.len(), 1, "expected exactly one {c:?} in {:?}", codes(r));
    v[0]
}

// ==================== 2D scenarios ====================

/// The full support mix (fixed, pinned, settled rollerX, inclined roller,
/// spring) on a 4-bay, 2-storey frame with members in `seg` segments:
/// 200 free DOFs at seg = 4, 45 at seg = 1 (small dense path).
fn mixed(seg: usize) -> M2 {
    portal(4, seg).0
}

/// Adds the indefinite but non-singular gadget: free nodes P, Q on springs
/// (kx = ky = kz = k) joined by a horizontal axial connector of stiffness −k.
/// Their x block is [[0, k], [k, 0]] (eigenvalues ±k): Cholesky fails on the
/// zero pivot, LU solves it, and both x diagonals are exactly zero. Loads
/// fx = 10 on P and −4 on Q give u_Q = 10/k, u_P = −4/k. Returns (P, Q).
fn add_gadget(m: &mut M2, k: f64) -> (usize, usize) {
    let p = m.node(5.0, 2.0);
    let q = m.node(6.0, 2.0);
    for n in [p, q] {
        m.sup(n, "spring", json!({ "kx": k, "ky": k, "kz": k }));
    }
    let id = m.connectors.len() + 1;
    m.connectors.push(json!({ "id": id, "nodeI": p, "nodeJ": q, "kAxial": -k }));
    m.load(p, 10.0, 0.0, 0.0);
    m.load(q, -4.0, 0.0, 0.0);
    (p, q)
}

/// Adds a free node held only by springs of stiffness `k` (decoupled from
/// the structure), loaded by a moment `my`: it sets the minimum diagonal of
/// K_ff, and its rotation is my/k.
fn add_floating(m: &mut M2, k: f64, my: f64) -> usize {
    let n = m.node(4.0, 1.5);
    m.sup(n, "spring", json!({ "kx": k, "ky": k, "kz": k }));
    m.load(n, 0.0, 0.0, my);
    n
}

/// Truss chain of 65 bars on springs at every node (diagonally dominant:
/// strict IC(0) and MIC factor it).
fn truss_chain() -> M2 {
    let mut m = M2::default();
    let mut prev = m.node(1.0, 0.5);
    for k in 1..=65 {
        let n = m.node(1.0 + 0.5 * k as f64, 0.5);
        m.elem("truss", prev, n);
        prev = n;
    }
    for n in 1..=66 {
        m.sup(n, "spring", json!({ "kx": 4e6, "ky": 4e6 }));
        m.load(n, 10.0 + n as f64, -3.0, 0.0);
    }
    m
}

// ==================== 2D path runner and checks ====================

#[derive(Clone, Copy, Debug)]
enum Path2 {
    /// No solver options: sparse (nf ≥ 64) or dense (nf < 64) direct.
    Default,
    /// `prepare_static_2d_dense_reference`: dense assembly, sparse Cholesky of
    /// the dense K_ff for nf ≥ 64, dense LU when Cholesky fails.
    DenseRef,
    /// Explicit PCG with this preconditioner (tolerance 1e-12).
    Pcg(&'static str),
    /// Explicit PCG capped at one iteration: always falls back to direct.
    PcgMaxit1,
}

fn with_opts(m: &M2, o: Value) -> M2 {
    let mut mm = m.clone();
    mm.opts = Some(o);
    mm
}

fn solve_path_2d(m: &M2, p: Path2) -> Result<AnalysisResults, String> {
    match p {
        Path2::Default => solve_2d(&m.input()),
        Path2::DenseRef => {
            let i = m.input();
            let prepared = prepare_static_2d_dense_reference(&i)?;
            prepared.solve_loads(&i.loads)
        }
        Path2::Pcg(pc) => solve_2d(&with_opts(m, json!({ "method": "pcg", "preconditioner": pc, "tolerance": 1e-12 })).input()),
        Path2::PcgMaxit1 => solve_2d(&with_opts(m, json!({ "method": "pcg", "maxIterations": 1 })).input()),
    }
}

use DiagnosticCode as C;

/// What the reference predicts for the conditioning and displacement checks.
struct Expect2 {
    nf: usize,
    maxd: f64,
    ratio: f64,
    near_zero: Vec<usize>,
    max_translation: f64,
    char_length: f64,
}

fn expect_2d(m: &M2, rf: &Ref2D) -> Expect2 {
    let input = m.input();
    let dn = DofNumbering::build_2d(&input);
    let nf = dn.n_free;
    let incl: std::collections::HashMap<usize, f64> = m
        .sups
        .iter()
        .filter(|s| s["type"] == "inclinedRoller")
        .map(|s| (s["nodeId"].as_u64().unwrap() as usize, s["angle"].as_f64().unwrap()))
        .collect();
    let mut diag = vec![0.0; nf];
    let mut max_translation = 0.0f64;
    for (&(node, local), &g) in &dn.map {
        if g >= nf {
            continue;
        }
        let kd = rf.kdiag[&node];
        let u = rf.disp[&node];
        let (d, t) = match (incl.get(&node), local) {
            (Some(&th), 0) => {
                let t = [th.cos(), -th.sin()];
                let mut d = 0.0;
                for a in 0..2 {
                    for b in 0..2 {
                        d += t[a] * kd[a][b] * t[b];
                    }
                }
                (d, (t[0] * u[0] + t[1] * u[1]).abs())
            }
            (Some(_), l) if l < 2 => panic!("inclined roller: local DOF {l} free"),
            _ => (kd[local][local], if local < 2 { u[local].abs() } else { 0.0 }),
        };
        diag[g] = d;
        max_translation = max_translation.max(t);
    }
    let maxd = diag.iter().fold(0.0f64, |a, d| a.max(d.abs()));
    let thr = maxd * 1e-12;
    let near_zero: Vec<usize> = (0..nf).filter(|&g| diag[g].abs() <= thr).collect();
    let mind = diag.iter().map(|d| d.abs()).filter(|&d| d > thr).fold(f64::MAX, f64::min);
    let (mut x0, mut x1, mut z0, mut z1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &(_, x, z) in &m.nodes {
        x0 = x0.min(x);
        x1 = x1.max(x);
        z0 = z0.min(z);
        z1 = z1.max(z);
    }
    let char_length = ((x1 - x0).powi(2) + (z1 - z0).powi(2)).sqrt().max(1.0);
    Expect2 { nf, maxd, ratio: maxd / mind, near_zero, max_translation, char_length }
}

fn close(got: f64, want: f64, rel: f64) -> bool {
    (got - want).abs() <= rel * want.abs().max(1e-300)
}

/// Checks one 2D result against the reference and the exact diagnostics:
/// `pre` are the pre-solve gate codes, `head` the path's own codes (solver
/// path, fallbacks); the conditioning, displacement and residual codes that
/// follow are predicted from the reference.
#[allow(clippy::too_many_arguments)]
fn check_2d(
    label: &str,
    r: &AnalysisResults,
    m: &M2,
    rf: &Ref2D,
    path: &str,
    pre: &[C],
    head: &[C],
    ref_tol: f64,
    resid_max: f64,
) {
    let ex = expect_2d(m, rf);
    let nf = ex.nf;
    let meta = r.solver_run_meta.as_ref().unwrap();
    assert_eq!(meta.solver_path, path, "{label}: solver path");
    assert_eq!(meta.n_free_dofs, nf, "{label}: n_free_dofs");
    assert_matches_reference_2d(r, rf, ref_tol, label);

    let sd = &r.structured_diagnostics;
    let pre_got: Vec<C> = sd.iter().filter(|d| d.phase.as_deref() == Some("pre_solve")).map(|d| d.code).collect();
    assert_eq!(pre_got, pre, "{label}: pre-solve diagnostics");
    let tail: Vec<&StructuredDiagnostic> = sd.iter().filter(|d| d.phase.as_deref() != Some("pre_solve")).collect();
    let excessive = ex.max_translation > 1000.0 * ex.char_length;
    let high = ex.ratio > 1e8;
    let mut want: Vec<C> = head.to_vec();
    if excessive {
        want.push(C::ExcessiveDisplacement);
    }
    if high {
        want.push(C::HighDiagonalRatio);
    }
    if !ex.near_zero.is_empty() {
        want.push(C::NearZeroDiagonal);
    }
    want.push(C::ResidualOk);
    assert_eq!(tail.iter().map(|d| d.code).collect::<Vec<_>>(), want, "{label}: diagnostics {:#?}", tail);

    let lu_msg = "Cholesky factorization failed — LU fallback succeeded but model may be unstable (not positive-definite)";
    for d in &tail {
        let ph = d.phase.as_deref();
        match d.code {
            C::SparseCholesky => {
                assert_eq!(d.message, format!("Sparse Cholesky solver ({nf} free DOFs)"), "{label}");
                assert_eq!((d.severity, ph), (Severity::Info, Some("solve")), "{label}");
            }
            C::DenseLu => {
                assert_eq!(d.message, format!("Dense solver ({nf} free DOFs)"), "{label}");
                assert_eq!((d.severity, ph), (Severity::Info, Some("solve")), "{label}");
            }
            C::CholeskyFailedLuFallback => {
                assert_eq!(d.message, lu_msg, "{label}");
                assert_eq!((d.severity, ph), (Severity::Warning, Some("solve")), "{label}");
            }
            C::PcgFallbackDirect => {
                assert_eq!(d.message, format!("PCG failed verification, fell back to direct solver ({nf} free DOFs)"), "{label}");
                assert_eq!((d.severity, ph), (Severity::Warning, Some("solve")), "{label}");
            }
            C::PcgSolve => {
                let pc = path.strip_prefix("pcg_").unwrap();
                let legacy = r.solver_diagnostics.last().unwrap();
                assert_eq!(legacy.message, d.message, "{label}: both channels name the same PCG run");
                assert_eq!((legacy.category.as_str(), legacy.severity.as_str()), ("solver_path", "info"), "{label}");
                let its: usize = d
                    .message
                    .strip_prefix(&format!("PCG solver ({pc} preconditioner, "))
                    .and_then(|t| t.strip_suffix(&format!(" iterations, {nf} free DOFs)")))
                    .unwrap_or_else(|| panic!("{label}: {}", d.message))
                    .parse()
                    .unwrap();
                assert!(its >= 1, "{label}");
                assert_eq!((d.severity, ph), (Severity::Info, Some("solve")), "{label}");
            }
            C::StiffnessNotPositiveDefinite => {
                assert_eq!((d.severity, ph), (Severity::Warning, Some("factorization")), "{label}");
            }
            C::ExcessiveDisplacement => {
                let v = d.value.unwrap();
                assert!(close(v, ex.max_translation, 1e-6), "{label}: max displacement {v:e}, reference {:e}", ex.max_translation);
                assert!(close(d.threshold.unwrap(), 1000.0 * ex.char_length, 1e-12), "{label}: threshold {:?}", d.threshold);
                assert_eq!(
                    d.message,
                    format!(
                        "Maximum displacement {:.2e} exceeds 1000× characteristic length {:.2e} — likely mechanism or instability",
                        v, ex.char_length
                    ),
                    "{label}"
                );
                assert_eq!((d.severity, ph), (Severity::Warning, Some("solve")), "{label}");
            }
            C::HighDiagonalRatio => {
                let v = d.value.unwrap();
                assert!(close(v, ex.ratio, 1e-9), "{label}: diagonal ratio {v:e}, reference {:e}", ex.ratio);
                assert_eq!(d.threshold, Some(1e8), "{label}");
                assert_eq!(d.message, format!("High diagonal ratio {:.2e}", v), "{label}");
                assert_eq!((d.severity, ph), (Severity::Warning, Some("conditioning")), "{label}");
            }
            C::NearZeroDiagonal => {
                assert_eq!(d.dof_indices, ex.near_zero, "{label}: near-zero DOFs");
                assert_eq!(d.message, format!("{} near-zero diagonal entries", ex.near_zero.len()), "{label}");
                assert_eq!((d.severity, ph), (Severity::Warning, Some("conditioning")), "{label}");
            }
            C::ResidualOk => {
                let v = d.value.unwrap();
                assert!(v > 0.0 && v <= resid_max, "{label}: residual {v:e} (max {resid_max:e})");
                assert_eq!(d.threshold, Some(1e-6), "{label}");
                assert_eq!(d.message, format!("Residual {:.2e} ({} free DOFs)", v, nf), "{label}");
                assert_eq!((d.severity, ph), (Severity::Info, Some("solve")), "{label}");
                let eq = r.equilibrium.as_ref().unwrap();
                assert_eq!(eq.relative_residual, v, "{label}: equilibrium carries the same residual");
                assert!(eq.residual_ok, "{label}: equilibrium {eq:?}");
            }
            other => panic!("{label}: unexpected {other:?}"),
        }
    }
}

fn not_pd_message(r: &AnalysisResults) -> &str {
    &diag(&r.structured_diagnostics, C::StiffnessNotPositiveDefinite).message
}

// ==================== 2D tests ====================

/// Sparse Cholesky (default and on the dense assembly), PCG with each
/// preconditioner that factors this pattern, and PCG falling back to sparse
/// Cholesky, on the full support mix with a settlement: displacements and
/// reactions match the reference, diagnostics are exactly the path's.
#[test]
fn spd_frame_every_2d_path_matches_reference() {
    let m = mixed(4);
    let rf = reference_2d(&m);
    assert_eq!(expect_2d(&m, &rf).nf, 200);
    let sc = [C::SparseCholesky];
    for (p, path, head, tol, res) in [
        (Path2::Default, "sparse_cholesky", &sc[..], 1e-9, 1e-12),
        (Path2::DenseRef, "sparse_cholesky", &sc[..], 1e-9, 1e-12),
        (Path2::Pcg("ics"), "pcg_ics", &[C::PcgSolve][..], 1e-7, 1e-11),
        (Path2::Pcg("ssor"), "pcg_ssor", &[C::PcgSolve][..], 1e-7, 1e-11),
        (Path2::Pcg("jacobi"), "pcg_jacobi", &[C::PcgSolve][..], 1e-7, 1e-11),
        (Path2::Pcg("none"), "pcg_none", &[C::PcgSolve][..], 1e-7, 1e-11),
        (Path2::PcgMaxit1, "pcg_fallback_sparse_cholesky", &[C::PcgFallbackDirect, C::SparseCholesky][..], 1e-9, 1e-12),
    ] {
        let r = solve_path_2d(&m, p).unwrap();
        check_2d(&format!("{p:?}"), &r, &m, &rf, path, &[], head, tol, res);
    }
    // Explicit strict IC(0) and MIC do not factor this pattern: they degrade
    // to Jacobi with warnings in the legacy channel.
    for pc in ["ic0", "mic"] {
        let r = solve_path_2d(&m, Path2::Pcg(pc)).unwrap();
        check_2d(pc, &r, &m, &rf, "pcg_jacobi", &[], &[C::PcgSolve], 1e-7, 1e-11);
        let msgs: Vec<&str> = r.solver_diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(msgs[0], format!("Preconditioner '{pc}' failed to build (non-SPD pivot or zero diagonal), degrading"));
        assert_eq!(msgs[1], "Preconditioner degraded to 'jacobi'");
    }
    // The fallback's legacy channel: the last PCG failure, then the direct solve.
    let r = solve_path_2d(&m, Path2::PcgMaxit1).unwrap();
    let n = r.solver_diagnostics.len();
    let fb = &r.solver_diagnostics[n - 2];
    assert_eq!((fb.category.as_str(), fb.severity.as_str()), ("fallback", "warning"));
    assert!(fb.message.ends_with(", falling back to direct solver"), "{}", fb.message);
    let last = &r.solver_diagnostics[n - 1];
    assert_eq!(
        (last.category.as_str(), last.severity.as_str(), last.message.as_str()),
        ("solver_path", "info", "Sparse Cholesky solver (200 free DOFs)")
    );
}

/// The same prepared structure solved for a second load set 10⁴ times
/// larger: the factorization is reused, the result still matches the
/// reference, and the residual stays relative (≤ 1e-12 although ‖f‖ ~ 1e6).
#[test]
fn prepared_2d_paths_reuse_the_factor_for_a_second_load_set() {
    let m = mixed(4);
    let mut big = m.clone();
    for l in big.loads.iter_mut() {
        for k in ["fx", "fz", "my"] {
            let v = l["data"][k].as_f64().unwrap();
            l["data"][k] = json!(v * 1e4);
        }
    }
    let rf = reference_2d(&m);
    let rf_big = reference_2d(&big);
    let big_loads = big.input().loads;
    for p in [Path2::Default, Path2::DenseRef, Path2::Pcg("ics")] {
        let mm = match p {
            Path2::Pcg(pc) => with_opts(&m, json!({ "method": "pcg", "preconditioner": pc, "tolerance": 1e-12 })),
            _ => m.clone(),
        };
        let i = mm.input();
        let prepared = match p {
            Path2::DenseRef => prepare_static_2d_dense_reference(&i).unwrap(),
            _ => prepare_static_2d(&i).unwrap(),
        };
        let (path, head, tol, res) = match p {
            Path2::Pcg(_) => ("pcg_ics", C::PcgSolve, 1e-7, 1e-11),
            _ => ("sparse_cholesky", C::SparseCholesky, 1e-9, 1e-12),
        };
        let r1 = prepared.solve_loads(&i.loads).unwrap();
        check_2d(&format!("{p:?} case 1"), &r1, &mm, &rf, path, &[], &[head], tol, res);
        let r2 = prepared.solve_loads(&big_loads).unwrap();
        check_2d(&format!("{p:?} case 2"), &r2, &big, &rf_big, path, &[], &[head], tol, res);
    }
}

/// A diagonally dominant truss chain: strict IC(0), MIC and shifted IC all
/// factor it, so each preconditioner is the one reported.
#[test]
fn truss_chain_runs_pcg_with_ic0_mic_and_ics() {
    let m = truss_chain();
    let rf = reference_2d(&m);
    // the gates flag every truss-only node
    let pre = vec![C::InstabilityRisk; 66];
    for pc in ["ic0", "mic", "ics"] {
        let r = solve_path_2d(&m, Path2::Pcg(pc)).unwrap();
        check_2d(pc, &r, &m, &rf, &format!("pcg_{pc}"), &pre, &[C::PcgSolve], 1e-8, 1e-11);
    }
    let r = solve_path_2d(&m, Path2::Default).unwrap();
    check_2d("direct", &r, &m, &rf, "sparse_cholesky", &pre, &[C::SparseCholesky], 1e-9, 1e-12);
}

/// Below 64 free DOFs the same frame takes the dense Cholesky path.
#[test]
fn small_frame_takes_the_dense_path() {
    let m = mixed(1);
    let rf = reference_2d(&m);
    assert!(expect_2d(&m, &rf).nf < 64);
    for p in [Path2::Default, Path2::DenseRef] {
        let r = solve_path_2d(&m, p).unwrap();
        check_2d(&format!("{p:?}"), &r, &m, &rf, "dense_lu", &[], &[C::DenseLu], 1e-9, 1e-12);
    }
}

/// Solver options choose the path: omitted, "direct" and "auto" below
/// 1000 free DOFs stay on sparse Cholesky; "auto" from 1000 free DOFs and
/// "pcg" at any size take PCG. Both give the same answer.
#[test]
fn solver_options_dispatch_2d() {
    let small = mixed(4);
    let rf = reference_2d(&small);
    for o in [json!({ "method": "auto" }), json!({ "method": "direct" })] {
        let mm = with_opts(&small, o.clone());
        let r = solve_2d(&mm.input()).unwrap();
        check_2d(&o.to_string(), &r, &mm, &rf, "sparse_cholesky", &[], &[C::SparseCholesky], 1e-9, 1e-12);
    }
    let big = mixed(20);
    let nf = DofNumbering::build_2d(&big.input()).n_free;
    assert!(nf >= 1000, "nf {nf}");
    let direct = solve_2d(&big.input()).unwrap();
    assert_eq!(path_2d(&direct), "sparse_cholesky");
    let d2 = solve_2d(&with_opts(&big, json!({ "method": "direct" })).input()).unwrap();
    assert_eq!(path_2d(&d2), "sparse_cholesky");
    // "auto" at nf ≥ 1000 goes down the PCG branch (here PCG does not reach
    // 1e-12 and the branch's direct fallback answers).
    let auto = solve_2d(&with_opts(&big, json!({ "method": "auto", "tolerance": 1e-12 })).input()).unwrap();
    assert_eq!(path_2d(&auto), "pcg_fallback_sparse_cholesky");
    let umax = direct.displacements.iter().fold(0.0f64, |a, d| a.max(d.ux.abs()).max(d.uz.abs()));
    for (a, b) in direct.displacements.iter().zip(&auto.displacements) {
        assert_eq!(a.node_id, b.node_id);
        assert!((a.ux - b.ux).abs() < 1e-7 * umax && (a.uz - b.uz).abs() < 1e-7 * umax);
    }
}

/// Indefinite, non-singular K_ff (the connector gadget): sparse Cholesky
/// fails and every path solves by dense LU, reporting it; the zero diagonals
/// are named. The gadget is decoupled, so the frame matches the reference
/// and u_Q = 10/k, u_P = −4/k with spring reactions −k·u.
#[test]
fn indefinite_stiffness_falls_back_to_dense_lu_on_every_2d_path() {
    for seg in [4, 1] {
        let mut m = mixed(seg);
        let (p, q) = add_gadget(&mut m, 1e6);
        let rf = reference_2d(&m);
        // closed form for the gadget (independent of the reference solve)
        assert!(close(rf.disp[&q][0], 1e-5, 1e-9) && close(rf.disp[&p][0], -4e-6, 1e-9));
        assert!(close(rf.reac[&q][0], -10.0, 1e-9) && close(rf.reac[&p][0], 4.0, 1e-9));
        let lu = [C::StiffnessNotPositiveDefinite, C::DenseLu, C::CholeskyFailedLuFallback];
        let paths: Vec<(Path2, &[C])> = if seg == 4 {
            vec![
                (Path2::Default, &lu[..]),
                (Path2::DenseRef, &lu[..]),
                (Path2::Pcg("ics"), &[C::StiffnessNotPositiveDefinite, C::PcgFallbackDirect, C::CholeskyFailedLuFallback][..]),
                (Path2::Pcg("jacobi"), &[C::StiffnessNotPositiveDefinite, C::PcgFallbackDirect, C::CholeskyFailedLuFallback][..]),
            ]
        } else {
            vec![(Path2::Default, &lu[..])]
        };
        for (path, head) in paths {
            let label = format!("seg {seg} {path:?}");
            let r = solve_path_2d(&m, path).unwrap();
            check_2d(&label, &r, &m, &rf, "dense_lu", &[], head, 1e-9, 1e-12);
            let why = match (seg, path) {
                (4, Path2::Default) | (4, Path2::Pcg(_)) => "sparse Cholesky failed; solved by dense LU",
                _ => "Cholesky failed; solved by dense LU",
            };
            assert!(not_pd_message(&r).contains(&format!("({why})")), "{label}: {}", not_pd_message(&r));
            if let Path2::Pcg(_) = path {
                // PCG is never tried on a matrix unshifted Cholesky cannot verify
                assert_eq!(r.solver_diagnostics.len(), 1, "{label}: {:?}", r.solver_diagnostics);
                let d = &r.solver_diagnostics[0];
                assert_eq!((d.category.as_str(), d.severity.as_str()), ("fallback", "warning"));
                assert_eq!(d.message, "Unshifted Cholesky could not verify structural stability; using the direct solver chain instead of PCG");
            }
        }
    }
}

/// A spring-held free node of stiffness 1e-2 sets the diagonal ratio above
/// 1e8 (HighDiagonalRatio, value = max/min diagonal of the reference) on
/// every path; its moment gives a rotation (my/k = 1e5) far above 1000× the
/// characteristic length while every translation is small, so only
/// translations count as excessive displacement.
#[test]
fn high_diagonal_ratio_and_rotation_only_node_on_every_2d_path() {
    for (seg, gadget) in [(4, false), (4, true), (1, false), (1, true)] {
        let mut m = mixed(seg);
        let f = add_floating(&mut m, 1e-2, 1e3);
        if gadget {
            add_gadget(&mut m, 1e6);
        }
        let rf = reference_2d(&m);
        assert!(close(rf.disp[&f][2], 1e5, 1e-9));
        let ex = expect_2d(&m, &rf);
        assert!(ex.ratio > 1e8 && ex.ratio < 1e12 && ex.max_translation < 1000.0 * ex.char_length);
        let lu = [C::StiffnessNotPositiveDefinite, C::DenseLu, C::CholeskyFailedLuFallback];
        let pcg_lu = [C::StiffnessNotPositiveDefinite, C::PcgFallbackDirect, C::CholeskyFailedLuFallback];
        let runs: Vec<(Path2, &str, &[C])> = match (seg, gadget) {
            (4, false) => vec![
                (Path2::Default, "sparse_cholesky", &[C::SparseCholesky][..]),
                (Path2::DenseRef, "sparse_cholesky", &[C::SparseCholesky][..]),
                (Path2::Pcg("ics"), "pcg_ics", &[C::PcgSolve][..]),
                (Path2::PcgMaxit1, "pcg_fallback_sparse_cholesky", &[C::PcgFallbackDirect, C::SparseCholesky][..]),
            ],
            (4, true) => vec![
                (Path2::Default, "dense_lu", &lu[..]),
                (Path2::DenseRef, "dense_lu", &lu[..]),
                (Path2::Pcg("ics"), "dense_lu", &pcg_lu[..]),
            ],
            (_, false) => vec![(Path2::Default, "dense_lu", &[C::DenseLu][..])],
            (_, true) => vec![(Path2::Default, "dense_lu", &lu[..])],
        };
        for (p, path, head) in runs {
            let r = solve_path_2d(&m, p).unwrap();
            let tol = if matches!(p, Path2::Pcg(_)) { 1e-7 } else { 1e-9 };
            let res = if matches!(p, Path2::Pcg(_)) { 1e-11 } else { 1e-12 };
            check_2d(&format!("seg {seg} gadget {gadget} {p:?}"), &r, &m, &rf, path, &[C::DisconnectedNode], head, tol, res);
        }
    }
}

/// A very soft frame (E = 1e-3 MPa): translations far above 1000× the
/// characteristic length (the diagonal of the bounding box) are reported with
/// the exact maximum and threshold, on every path.
#[test]
fn excessive_displacement_on_every_2d_path() {
    for (seg, gadget) in [(4, false), (4, true), (1, false), (1, true)] {
        let mut m = mixed(seg);
        m.e = Some(1e-3);
        if gadget {
            add_gadget(&mut m, 1.0);
        }
        let rf = reference_2d(&m);
        let ex = expect_2d(&m, &rf);
        assert!(ex.max_translation > 1000.0 * ex.char_length);
        let lu = [C::StiffnessNotPositiveDefinite, C::DenseLu, C::CholeskyFailedLuFallback];
        let pcg_lu = [C::StiffnessNotPositiveDefinite, C::PcgFallbackDirect, C::CholeskyFailedLuFallback];
        let runs: Vec<(Path2, &str, &[C])> = match (seg, gadget) {
            (4, false) => vec![
                (Path2::Default, "sparse_cholesky", &[C::SparseCholesky][..]),
                (Path2::DenseRef, "sparse_cholesky", &[C::SparseCholesky][..]),
                (Path2::Pcg("ics"), "pcg_ics", &[C::PcgSolve][..]),
                (Path2::PcgMaxit1, "pcg_fallback_sparse_cholesky", &[C::PcgFallbackDirect, C::SparseCholesky][..]),
            ],
            (4, true) => vec![
                (Path2::Default, "dense_lu", &lu[..]),
                (Path2::DenseRef, "dense_lu", &lu[..]),
                (Path2::Pcg("ics"), "dense_lu", &pcg_lu[..]),
            ],
            (_, false) => vec![(Path2::Default, "dense_lu", &[C::DenseLu][..])],
            (_, true) => vec![(Path2::Default, "dense_lu", &lu[..])],
        };
        for (p, path, head) in runs {
            let r = solve_path_2d(&m, p).unwrap();
            let tol = if matches!(p, Path2::Pcg(_)) { 1e-7 } else { 1e-9 };
            check_2d(&format!("soft seg {seg} gadget {gadget} {p:?}"), &r, &m, &rf, path, &[], head, tol, 1e-10);
        }
    }
}

/// Runs of the frame on each path family: large sparse (seg 4) through
/// default, dense-reference, PCG and PCG-fallback; small dense (seg 1); and,
/// with the gadget, the dense-LU fallbacks.
fn all_2d_variants(base: impl Fn(usize) -> M2) -> Vec<(String, M2, Path2)> {
    let mut v = Vec::new();
    for p in [Path2::Default, Path2::DenseRef, Path2::Pcg("ics"), Path2::PcgMaxit1] {
        v.push((format!("large {p:?}"), base(4), p));
    }
    v.push(("small Default".into(), base(1), Path2::Default));
    for p in [Path2::Default, Path2::DenseRef, Path2::Pcg("ics")] {
        let mut m = base(4);
        add_gadget(&mut m, 1e6);
        v.push((format!("gadget {p:?}"), m, p));
    }
    let mut m = base(1);
    add_gadget(&mut m, 1e6);
    v.push(("small gadget Default".into(), m, Path2::Default));
    v
}

/// Loads so large that the displacements are finite but K·u overflows: the
/// residual is NaN and every path reports ResidualHigh (and keeps the
/// solution, legacy 2D semantics). Loads that overflow the displacements
/// themselves are refused as a mechanism by every path, after a successful
/// prepare.
#[test]
fn overflowing_loads_on_every_2d_path() {
    let scaled = |seg: usize, e: f64, fz: f64| {
        let mut m = mixed(seg);
        m.e = Some(e);
        for l in m.loads.iter_mut() {
            l["data"]["fz"] = json!(fz);
        }
        m
    };
    for (label, m, p) in all_2d_variants(|seg| scaled(seg, E_MPA, -3e304)) {
        let r = solve_path_2d(&m, p).unwrap_or_else(|e| panic!("{label}: {e}"));
        let nf = r.solver_run_meta.as_ref().unwrap().n_free_dofs;
        let d = r.structured_diagnostics.last().unwrap();
        assert_eq!((d.code, d.severity, d.phase.as_deref()), (C::ResidualHigh, Severity::Warning, Some("solve")), "{label}");
        assert!(d.value.unwrap().is_nan(), "{label}: {:?}", d.value);
        assert_eq!(d.threshold, Some(1e-6), "{label}");
        assert_eq!(d.message, format!("Residual NaN exceeds tolerance ({nf} free DOFs)"), "{label}");
        assert!(!r.equilibrium.as_ref().unwrap().residual_ok, "{label}");
        assert!(r.displacements.iter().all(|x| x.ux.is_finite() && x.uz.is_finite()), "{label}");
        diag(&r.structured_diagnostics, C::ExcessiveDisplacement);
    }
    for (label, m, p) in all_2d_variants(|seg| scaled(seg, 1e-3, -1e306)) {
        let i = m.input();
        assert!(prepare_static_2d(&i).is_ok(), "{label}: prepare");
        assert_eq!(solve_path_2d(&m, p).err().as_deref(), Some(SINGULAR), "{label}");
    }
}

/// A floor joint with every member end hinged gets artificial rotational
/// stiffness 1e-10·max diag: a small moment turns it by M/k (and the diagonal
/// ratio is exactly 1e10); a moment that turns it past 100 rad is refused as
/// a local mechanism, on every path.
#[test]
fn all_hinged_joint_on_every_2d_path() {
    let hinged = |seg: usize, my: f64| {
        let mut m = mixed(seg);
        for l in m.loads.iter_mut() {
            l["data"]["my"] = json!(0.0);
        }
        let j = 2; // column line 0, first floor
        for e in m.elems.iter_mut() {
            if e.2 == j {
                e.4 = true;
            }
            if e.3 == j {
                e.5 = true;
            }
        }
        m.load(j, 0.0, 0.0, my);
        m
    };
    let maxd = |seg: usize| {
        let m = mixed(seg);
        expect_2d(&m, &reference_2d(&m)).maxd
    };
    for (label, m, p) in all_2d_variants(|seg| hinged(seg, 1e-3)) {
        let r = solve_path_2d(&m, p).unwrap_or_else(|e| panic!("{label}: {e}"));
        let seg = if label.contains("small") { 1 } else { 4 };
        let th = r.displacements.iter().find(|d| d.node_id == 2).unwrap().ry;
        let want = 1e-3 / (1e-10 * maxd(seg));
        assert!(th < 100.0 && close(th, want, 1e-6), "{label}: rotation {th:e}, M/k {want:e}");
        let hd = diag(&r.structured_diagnostics, C::HighDiagonalRatio);
        assert!(close(hd.value.unwrap(), 1e10, 1e-9), "{label}: {:?}", hd.value);
        // global force equilibrium from the reported reactions (springs and
        // the inclined roller included)
        let input = m.input();
        let (mut fx, mut fz) = (0.0, 0.0);
        for l in &input.loads {
            if let SolverLoad::Nodal(n) = l {
                fx += n.fx;
                fz += n.fz;
            }
        }
        let rx: f64 = r.reactions.iter().map(|x| x.rx).sum();
        let rz: f64 = r.reactions.iter().map(|x| x.rz).sum();
        assert!((rx + fx).abs() < 1e-6 * fz.abs() && (rz + fz).abs() < 1e-6 * fz.abs(), "{label}: ΣR = ({rx}, {rz}), ΣF = ({fx}, {fz})");
    }
    for (label, m, p) in all_2d_variants(|seg| hinged(seg, 1.0)) {
        let e = solve_path_2d(&m, p).err().unwrap_or_else(|| panic!("{label}: solved"));
        assert!(e.starts_with(LOCAL_MECH), "{label}: {e}");
    }
}

/// Straight beam of `n` frame elements (0.5 m each, from x = 1) on rollerX
/// supports at both ends: a horizontal rigid-body mechanism.
fn rolling_beam(n: usize) -> M2 {
    let mut m = M2::default();
    let mut prev = m.node(1.0, 0.0);
    for k in 1..=n {
        let nn = m.node(1.0 + 0.5 * k as f64, 0.0);
        m.elem("frame", prev, nn);
        prev = nn;
    }
    m.sup(1, "rollerX", json!({}));
    m.sup(prev, "rollerX", json!({}));
    m.load(3, 0.0, -1.0, 0.0);
    m
}

/// A genuine mechanism is refused on every path: at prepare by the direct
/// paths (Cholesky and LU both fail), at solve by PCG (whose stability check
/// is lazy).
#[test]
fn mechanism_is_refused_on_every_2d_path() {
    let large = rolling_beam(40);
    for p in [Path2::Default, Path2::DenseRef] {
        let i = large.input();
        let prep = if matches!(p, Path2::DenseRef) { prepare_static_2d_dense_reference(&i) } else { prepare_static_2d(&i) };
        assert_eq!(prep.err().as_deref(), Some(SINGULAR), "{p:?}");
    }
    for p in [Path2::Pcg("ics"), Path2::PcgMaxit1] {
        let mm = match p {
            Path2::Pcg(pc) => with_opts(&large, json!({ "method": "pcg", "preconditioner": pc })),
            _ => with_opts(&large, json!({ "method": "pcg", "maxIterations": 1 })),
        };
        let i = mm.input();
        let prepared = prepare_static_2d(&i).unwrap();
        assert_eq!(prepared.solve_loads(&i.loads).err().as_deref(), Some(SINGULAR), "{p:?}");
    }
    assert_eq!(solve_2d(&rolling_beam(10).input()).err().as_deref(), Some(SINGULAR), "small");
}

/// Every DOF restrained (nf = 0): reactions are K·u_r − F, with a settlement
/// and rotation imposed (closed form 12EIδ/L³, 6EIδ/L² through the
/// reference) and without (reactions = −F).
#[test]
fn fully_restrained_2d() {
    for settle in [true, false] {
        let mut m = M2::default();
        let a = m.node(1.0, 1.0);
        let b = m.node(4.0, 1.0);
        let c = m.node(4.0, 3.0);
        m.elem("frame", a, b);
        m.elem("frame", b, c);
        m.sup(a, "fixed", json!({}));
        m.sup(b, "fixed", if settle { json!({ "dz": -0.01, "dry": 0.002 }) } else { json!({}) });
        m.sup(c, "fixed", if settle { json!({ "dx": 0.003 }) } else { json!({}) });
        m.load(b, 3.0, -7.0, 2.0);
        m.load(c, -1.0, 0.0, 0.0);
        let rf = reference_2d(&m);
        if settle {
            let ei = E_MPA * 1000.0 * IZ;
            // member a–b (L = 3): end shear 12EIδ/L³ − 6EIθ/L², δ = −0.01, θ = 0.002
            let (l, d, t): (f64, f64, f64) = (3.0, -0.01, 0.002);
            let (v1, v2) = (12.0 * ei * d / l.powi(3), 6.0 * ei * t / l.powi(2));
            let got = rf.reac[&a][1].abs();
            assert!(close(got, (v1 + v2).abs(), 1e-9) || close(got, (v1 - v2).abs(), 1e-9), "{got} vs 12EIδ/L³ ± 6EIθ/L²");
        } else {
            for (got, want) in rf.reac[&b].iter().zip([-3.0, 7.0, -2.0]) {
                assert!(close(*got, want, 1e-12));
            }
        }
        for p in [Path2::Default, Path2::DenseRef] {
            let r = solve_path_2d(&m, p).unwrap();
            assert_eq!(path_2d(&r), "fully_restrained");
            assert_matches_reference_2d(&r, &rf, 1e-12, &format!("settle {settle} {p:?}"));
            let d = r.structured_diagnostics.last().unwrap();
            assert_eq!((d.code, d.message.as_str()), (C::ResidualOk, "Fully restrained model (0 free DOFs, 9 restrained)"));
        }
    }
}
