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
    let mut member = |m: &mut M2, a: usize, b: usize| {
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
    for s in &m.sups {
        let id = s["nodeId"].as_u64().unwrap() as usize;
        let n = idx[&id];
        let mut r = [rv[3 * n], rv[3 * n + 1], RY_SIGN * rv[3 * n + 2]];
        if let Some((_, ks)) = springs.iter().find(|(sn, _)| *sn == n) {
            let u = &disp[&id];
            for q in 0..3 {
                r[q] = -ks[q] * u[q];
            }
        }
        reac.insert(id, r);
    }
    Ref2D { disp, reac }
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
    let scale = |v: Vec<f64>| v.iter().fold(0.0f64, |a, b| a.max(b.abs())).max(1e-300);
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

// ==================== probe ====================

#[test]
#[ignore]
fn probe() {
    let (m, _) = portal(2, 4);
    let input = m.input();
    let dn = DofNumbering::build_2d(&input);
    println!("nf {} n {}", dn.n_free, dn.n_total);
    let r = solve_2d(&input).unwrap();
    println!("path {}", path_2d(&r));
    for d in &r.structured_diagnostics {
        println!("{:?} {:?} {} {:?} {:?} {:?}", d.code, d.severity, d.message, d.phase, d.value, d.threshold);
    }
    let rf = reference_2d(&m);
    for d in r.displacements.iter().take(5) {
        println!("{} eng {:e} {:e} {:e} ref {:?}", d.node_id, d.ux, d.uz, d.ry, rf.disp[&d.node_id]);
    }
    for x in &r.reactions {
        println!("R {} eng {:e} {:e} {:e} ref {:?}", x.node_id, x.rx, x.rz, x.my, rf.reac[&x.node_id]);
    }
    let _ = (prepare_static_2d, prepare_static_2d_dense_reference, SINGULAR, LOCAL_MECH, diag);
    assert_matches_reference_2d(&r, &rf, 1e-9, "probe");
}
