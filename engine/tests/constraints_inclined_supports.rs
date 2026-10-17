//! Multi-point constraints on nodes with inclined supports, in every analysis.
//!
//! Assembly rotates an inclined-support node's translations into the
//! support's frame; constraint equations are written in global components.
//! The constraint transform used to be built in global components over those
//! rotated DOFs, so the solve enforced the wrong equations and returned
//! results that violated the constraint with no warning (found by
//! tools/differential/codeaster, aster3d-16). The transform is now built in
//! the support frames.
//!
//! Each case is checked against a reference that has no inclined frame:
//! - `Ref::Mpc`: the inclined support replaced by the linear MPC `n·u = 0` on
//!   the node's global translations;
//! - `Ref::Rotated`: the whole structure turned rigidly so that the support is
//!   a plain restraint (for constraints that turn with it).
//!
//! and directly: the constraint equations and the support condition hold in
//! the results.

use dedaliano_engine::solver::linear::{solve_2d, solve_3d};
use dedaliano_engine::types::*;
use serde_json::{json, Value};

const THETA: f64 = 0.5;
/// Unit normal of the 3D inclined support (largest component on z).
const N3: [f64; 3] = [0.48, 0.36, 0.8];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Sup {
    /// The model as given: an inclined support at node 4.
    Inclined,
    /// Turned rigidly so that node 4's support is a plain restraint.
    Rotated,
    /// Node 4 unsupported, with the linear MPC n·u4 = 0.
    Mpc,
}

fn assert_close(what: &str, a: f64, b: f64, scale: f64, rel: f64) {
    assert!((a - b).abs() <= rel * scale.max(1e-300), "{what}: {a} vs {b} (scale {scale})");
}

// ─── 2D ────────────────────────────────────────────────────────────────────

/// Rotation by +theta in the x–z plane (turns the support normal
/// (sin θ, cos θ) onto z).
fn rot2(theta: f64, v: [f64; 2]) -> [f64; 2] {
    let (s, c) = theta.sin_cos();
    [c * v[0] - s * v[1], s * v[0] + c * v[1]]
}

fn n2() -> [f64; 2] {
    [THETA.sin(), THETA.cos()]
}

/// Portal 1-2-3-4 (fixed at 1, inclined roller at 4) and a frame 7-8 fixed at
/// 8 beside it; frames split into `sub` elements (sparse path past 64 free
/// DOFs). `constraints` gets the vector map of the variant (identity, or the
/// rigid turn) for offsets and MPC coefficient vectors.
fn model_2d(sup: Sup, sub: usize, constraints: &dyn Fn(&dyn Fn([f64; 2]) -> [f64; 2]) -> Vec<Value>) -> SolverInput {
    model_2d_at(THETA, sup, sub, constraints)
}

fn model_2d_at(theta: f64, sup: Sup, sub: usize, constraints: &dyn Fn(&dyn Fn([f64; 2]) -> [f64; 2]) -> Vec<Value>) -> SolverInput {
    let rotated = sup == Sup::Rotated;
    let r = move |v: [f64; 2]| if rotated { rot2(theta, v) } else { v };
    let pts = [(1, 0.0, 0.0), (2, 0.0, 4.0), (3, 5.0, 4.0), (4, 5.0, 0.0), (7, 6.5, 1.0), (8, 9.0, 1.0)];
    let mut nodes = serde_json::Map::new();
    let mut xy = std::collections::HashMap::new();
    for (id, x, z) in pts {
        xy.insert(id, [x, z]);
        let p = r([x, z]);
        nodes.insert(id.to_string(), json!({"id": id, "x": p[0], "z": p[1]}));
    }
    let mut elements = serde_json::Map::new();
    let mut next_node = 100;
    let mut next_elem = 1;
    for (a, b) in [(1, 2), (2, 3), (3, 4), (7, 8)] {
        let (pa, pb) = (xy[&a], xy[&b]);
        let mut prev = a;
        for k in 1..=sub {
            let id = if k == sub { b } else {
                let t = k as f64 / sub as f64;
                let p = r([pa[0] + t * (pb[0] - pa[0]), pa[1] + t * (pb[1] - pa[1])]);
                next_node += 1;
                nodes.insert(next_node.to_string(), json!({"id": next_node, "x": p[0], "z": p[1]}));
                next_node
            };
            elements.insert(next_elem.to_string(), json!({"id": next_elem, "type": "frame",
                "nodeI": prev, "nodeJ": id, "materialId": 1, "sectionId": 1,
                "hingeStart": false, "hingeEnd": false}));
            next_elem += 1;
            prev = id;
        }
    }
    let mut supports = serde_json::Map::new();
    supports.insert("1".into(), json!({"id": 1, "nodeId": 1, "type": "fixed"}));
    supports.insert("8".into(), json!({"id": 8, "nodeId": 8, "type": "fixed"}));
    match sup {
        Sup::Inclined => {
            supports.insert("4".into(), json!({"id": 4, "nodeId": 4, "type": "inclinedRoller", "angle": theta}));
        }
        Sup::Rotated => {
            supports.insert("4".into(), json!({"id": 4, "nodeId": 4, "type": "rollerX"}));
        }
        Sup::Mpc => {}
    }
    let mut cs = constraints(&r);
    if sup == Sup::Mpc {
        let n = [theta.sin(), theta.cos()];
        cs.push(json!({"type": "linearMPC", "terms": [
            {"nodeId": 4, "dof": 0, "coefficient": n[0]},
            {"nodeId": 4, "dof": 1, "coefficient": n[1]}]}));
    }
    let loads: Vec<Value> = [(3, [10.0, -20.0], 0.0), (7, [5.0, -30.0], 2.0), (2, [4.0, 0.0], -1.0)]
        .iter()
        .map(|&(node, f, m)| {
            let f = r(f);
            json!({"type": "nodal", "data": {"nodeId": node, "fx": f[0], "fz": f[1], "my": m}})
        })
        .collect();
    serde_json::from_value(json!({
        "nodes": nodes,
        "materials": {"1": {"id": 1, "e": 200000.0, "nu": 0.3}},
        "sections": {"1": {"id": 1, "a": 0.01, "iz": 1e-4}},
        "elements": elements,
        "supports": supports,
        "loads": loads,
        "constraints": cs,
    }))
    .unwrap()
}

type Constraints2D = fn(&dyn Fn([f64; 2]) -> [f64; 2]) -> Vec<Value>;

/// (name, constraints, reference). Node 4 is on the inclined roller; 7 is
/// beside it.
fn cases_2d() -> Vec<(&'static str, Constraints2D, Sup)> {
    vec![
        ("rigid link, master on the inclined node", |_| vec![
            json!({"type": "rigidLink", "masterNode": 4, "slaveNode": 7, "dofs": [0, 1, 2]})], Sup::Mpc),
        ("rigid link, master on the inclined node (rotated)", |_| vec![
            json!({"type": "rigidLink", "masterNode": 4, "slaveNode": 7, "dofs": [0, 1, 2]})], Sup::Rotated),
        ("rigid link, slave on the inclined node", |_| vec![
            json!({"type": "rigidLink", "masterNode": 7, "slaveNode": 4, "dofs": [0, 1, 2]})], Sup::Rotated),
        ("rigid link translations only, slave on the inclined node", |_| vec![
            json!({"type": "rigidLink", "masterNode": 7, "slaveNode": 4, "dofs": [0, 1]})], Sup::Rotated),
        ("equal DOF, master on the inclined node", |_| vec![
            json!({"type": "equalDOF", "masterNode": 4, "slaveNode": 7, "dofs": [0, 1]})], Sup::Mpc),
        ("equal DOF, slave on the inclined node", |_| vec![
            json!({"type": "equalDOF", "masterNode": 7, "slaveNode": 4, "dofs": [0, 1, 2]})], Sup::Rotated),
        // u4x = u7x either way round; the reference states it with 4 as master.
        ("equal DOF on ux only, slave on the inclined node", |_| vec![
            json!({"type": "equalDOF", "masterNode": 7, "slaveNode": 4, "dofs": [0]})], Sup::Mpc),
        ("equal DOF on uz only, master on the inclined node", |_| vec![
            json!({"type": "equalDOF", "masterNode": 4, "slaveNode": 7, "dofs": [1]})], Sup::Mpc),
        ("eccentric connection, master on the inclined node", |r| {
            let o = r([1.5, 1.0]);
            vec![json!({"type": "eccentricConnection", "masterNode": 4, "slaveNode": 7,
                        "offsetX": o[0], "offsetY": o[1], "offsetZ": 0.0})]
        }, Sup::Rotated),
        ("eccentric connection, slave on the inclined node", |r| {
            let o = r([-1.5, -1.0]);
            vec![json!({"type": "eccentricConnection", "masterNode": 7, "slaveNode": 4,
                        "offsetX": o[0], "offsetY": o[1], "offsetZ": 0.0})]
        }, Sup::Rotated),
        ("diaphragm, master on the inclined node", |_| vec![
            json!({"type": "diaphragm", "masterNode": 4, "slaveNodes": [7], "plane": "XY"})], Sup::Mpc),
        ("diaphragm, slave on the inclined node", |_| vec![
            json!({"type": "diaphragm", "masterNode": 7, "slaveNodes": [4], "plane": "XY"})], Sup::Rotated),
        ("linear MPC on the inclined node's translations", |r| {
            let (a, b) = (r([0.3, 1.0]), r([1.0, -0.4]));
            vec![json!({"type": "linearMPC", "terms": [
                {"nodeId": 4, "dof": 0, "coefficient": a[0]}, {"nodeId": 4, "dof": 1, "coefficient": a[1]},
                {"nodeId": 7, "dof": 0, "coefficient": -b[0]}, {"nodeId": 7, "dof": 1, "coefficient": -b[1]},
                {"nodeId": 7, "dof": 2, "coefficient": -0.7}]})]
        }, Sup::Rotated),
    ]
}

/// The MPC reference states a one-component equal DOF with node 4 as master
/// (the same equation): its slave cannot also carry the support MPC.
fn mpc_reference_2d(name: &str, c: Constraints2D, sub: usize) -> SolverInput {
    if name.starts_with("equal DOF on ux only, slave") {
        return model_2d(Sup::Mpc, sub, &|_| vec![
            json!({"type": "equalDOF", "masterNode": 4, "slaveNode": 7, "dofs": [0]})]);
    }
    model_2d(Sup::Mpc, sub, &c)
}

fn disp_2d(res: &AnalysisResults, node: usize) -> [f64; 3] {
    let d = res.displacements.iter().find(|d| d.node_id == node).unwrap();
    [d.ux, d.uz, d.ry]
}

/// Every node of `a` (the inclined model) against `b` (the reference), with
/// `b` turned back to the original axes when it is the rotated model.
fn compare_2d(what: &str, a: &[Displacement], b: &[Displacement], rotated: bool) {
    let scale = a.iter().fold(0.0f64, |m, d| m.max(d.ux.abs()).max(d.uz.abs()).max(d.ry.abs()));
    assert!(scale > 1e-9, "{what}: no response");
    for db in b {
        let da = a.iter().find(|d| d.node_id == db.node_id).unwrap();
        let t = if rotated { rot2(-THETA, [db.ux, db.uz]) } else { [db.ux, db.uz] };
        assert_close(&format!("{what} node {} ux", db.node_id), da.ux, t[0], scale, 1e-8);
        assert_close(&format!("{what} node {} uz", db.node_id), da.uz, t[1], scale, 1e-8);
        assert_close(&format!("{what} node {} ry", db.node_id), da.ry, db.ry, scale, 1e-8);
    }
}

/// The support condition and the rigid-link equations at node 4 / 7.
fn check_support_2d(what: &str, res: &AnalysisResults) {
    let u4 = disp_2d(res, 4);
    let n = n2();
    let scale = res.displacements.iter().fold(0.0f64, |m, d| m.max(d.ux.abs()).max(d.uz.abs()));
    assert_close(&format!("{what}: support n·u4"), n[0] * u4[0] + n[1] * u4[1], 0.0, scale, 1e-9);
}

#[test]
fn linear_static_2d_constraints_on_inclined_roller_match_references() {
    for sub in [1, 12] {
        for (name, c, reference) in cases_2d() {
            let what = format!("sub {sub}, {name}");
            let a = solve_2d(&model_2d(Sup::Inclined, sub, &c)).unwrap_or_else(|e| panic!("{what}: {e}"));
            check_support_2d(&what, &a);
            let b_input = if reference == Sup::Mpc { mpc_reference_2d(name, c, sub) } else { model_2d(reference, sub, &c) };
            let b = solve_2d(&b_input).unwrap_or_else(|e| panic!("{what} reference: {e}"));
            compare_2d(&what, &a.displacements, &b.displacements, reference == Sup::Rotated);
        }
    }
}

/// u7 = u4 + θ4 × r for a rigid link 4 → 7, read straight off the results.
#[test]
fn linear_static_2d_rigid_link_holds_at_inclined_roller() {
    for (master, slave) in [(4, 7), (7, 4)] {
        let input = model_2d(Sup::Inclined, 1, &|_| vec![
            json!({"type": "rigidLink", "masterNode": master, "slaveNode": slave, "dofs": [0, 1, 2]})]);
        let res = solve_2d(&input).unwrap();
        let (m, s) = (disp_2d(&res, master), disp_2d(&res, slave));
        let (pm, ps) = (&input.nodes[&master.to_string()], &input.nodes[&slave.to_string()]);
        let (dx, dz) = (ps.x - pm.x, ps.z - pm.z);
        let scale = m.iter().chain(&s).fold(0.0f64, |a, v| a.max(v.abs()));
        assert_close("ux", s[0], m[0] - dz * m[2], scale, 1e-10);
        assert_close("uz", s[1], m[1] + dx * m[2], scale, 1e-10);
        assert_close("ry", s[2], m[2], scale, 1e-10);
        check_support_2d("rigid link", &res);
        // Global equilibrium: reactions balance the loads.
        let (fx, fz) = input.loads.iter().fold((0.0, 0.0), |(x, z), l| match l {
            SolverLoad::Nodal(n) => (x + n.fx, z + n.fz),
            _ => (x, z),
        });
        let (rx, rz) = res.reactions.iter().fold((0.0, 0.0), |(x, z), r| (x + r.rx, z + r.rz));
        assert_close("ΣFx", rx + fx, 0.0, fx.abs() + fz.abs(), 1e-9);
        assert_close("ΣFz", rz + fz, 0.0, fx.abs() + fz.abs(), 1e-9);
        // The roller's reaction is along its normal.
        let r4 = res.reactions.iter().find(|r| r.node_id == 4).unwrap();
        let n = n2();
        assert_close("roller reaction ⟂ normal", r4.rx * n[1] - r4.rz * n[0], 0.0, r4.rx.abs() + r4.rz.abs(), 1e-9);
    }
}

// ─── 3D ────────────────────────────────────────────────────────────────────

/// Rotation turning N3 onto +z (Rodrigues about N3 × z).
fn q3() -> [[f64; 3]; 3] {
    let n = N3;
    let (ax, ay) = (n[1], -n[0]); // n × z = (ny, -nx, 0)
    let s = (ax * ax + ay * ay).sqrt();
    let c = n[2];
    let (kx, ky) = (ax / s, ay / s);
    // R = I + sinφ K + (1 − cosφ) K², K = [k]× with k = (kx, ky, 0)
    let k = [[0.0, 0.0, ky], [0.0, 0.0, -kx], [-ky, kx, 0.0]];
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let k2: f64 = (0..3).map(|l| k[i][l] * k[l][j]).sum();
            r[i][j] = if i == j { 1.0 } else { 0.0 } + s * k[i][j] + (1.0 - c) * k2;
        }
    }
    r
}

fn mul3(r: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| r[i][0] * v[0] + r[i][1] * v[1] + r[i][2] * v[2])
}

fn tr3(r: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    [0, 1, 2].map(|i| [0, 1, 2].map(|j| r[j][i]))
}

/// Space frame: portal 1-2-3-4 (fixed at 1, inclined support with normal N3
/// at 4), frame 7-8 fixed at 8. Sections are axisymmetric (iy = iz) so that
/// the rigid turn of the reference does not depend on member local axes.
fn model_3d(sup: Sup, sub: usize, constraints: &dyn Fn(&dyn Fn([f64; 3]) -> [f64; 3]) -> Vec<Value>) -> SolverInput3D {
    let q = q3();
    let rotated = sup == Sup::Rotated;
    let r = move |v: [f64; 3]| if rotated { mul3(&q, v) } else { v };
    let pts = [(1, [0.0, 0.0, 0.0]), (2, [0.0, 0.0, 4.0]), (3, [5.0, 0.0, 4.0]), (4, [5.0, 0.0, 0.0]),
               (7, [6.5, 0.8, 1.0]), (8, [9.0, 0.8, 1.0])];
    let mut nodes = serde_json::Map::new();
    let mut xyz = std::collections::HashMap::new();
    for (id, p) in pts {
        xyz.insert(id, p);
        let p = r(p);
        nodes.insert(id.to_string(), json!({"id": id, "x": p[0], "y": p[1], "z": p[2]}));
    }
    let mut elements = serde_json::Map::new();
    let mut next_node = 100;
    let mut next_elem = 1;
    for (a, b) in [(1, 2), (2, 3), (3, 4), (7, 8)] {
        let (pa, pb) = (xyz[&a], xyz[&b]);
        let mut prev = a;
        for k in 1..=sub {
            let id = if k == sub { b } else {
                let t = k as f64 / sub as f64;
                let p = r([0, 1, 2].map(|i| pa[i] + t * (pb[i] - pa[i])));
                next_node += 1;
                nodes.insert(next_node.to_string(), json!({"id": next_node, "x": p[0], "y": p[1], "z": p[2]}));
                next_node
            };
            elements.insert(next_elem.to_string(), json!({"id": next_elem, "type": "frame",
                "nodeI": prev, "nodeJ": id, "materialId": 1, "sectionId": 1}));
            next_elem += 1;
            prev = id;
        }
    }
    let fixed = |id: usize| json!({"nodeId": id, "rx": true, "ry": true, "rz": true,
                                   "rrx": true, "rry": true, "rrz": true});
    let mut supports = serde_json::Map::new();
    supports.insert("1".into(), fixed(1));
    supports.insert("8".into(), fixed(8));
    match sup {
        Sup::Inclined => {
            supports.insert("4".into(), json!({"nodeId": 4, "rx": false, "ry": false, "rz": true,
                "rrx": false, "rry": false, "rrz": false, "isInclined": true,
                "normalX": N3[0], "normalY": N3[1], "normalZ": N3[2]}));
        }
        Sup::Rotated => {
            supports.insert("4".into(), json!({"nodeId": 4, "rx": false, "ry": false, "rz": true,
                "rrx": false, "rry": false, "rrz": false}));
        }
        Sup::Mpc => {}
    }
    let mut cs = constraints(&r);
    if sup == Sup::Mpc {
        cs.push(json!({"type": "linearMPC", "terms": [
            {"nodeId": 4, "dof": 0, "coefficient": N3[0]},
            {"nodeId": 4, "dof": 1, "coefficient": N3[1]},
            {"nodeId": 4, "dof": 2, "coefficient": N3[2]}]}));
    }
    let loads: Vec<Value> = [(3, [10.0, 6.0, -20.0], [0.0, 1.0, 0.0]), (7, [5.0, -4.0, -30.0], [2.0, 0.0, -1.0]),
                             (2, [4.0, 3.0, 0.0], [0.0, 0.0, 1.5])]
        .iter()
        .map(|&(node, f, m)| {
            let (f, m) = (r(f), r(m));
            json!({"type": "nodal", "data": {"nodeId": node, "fx": f[0], "fy": f[1], "fz": f[2],
                                              "mx": m[0], "my": m[1], "mz": m[2]}})
        })
        .collect();
    serde_json::from_value(json!({
        "nodes": nodes,
        "materials": {"1": {"id": 1, "e": 200000.0, "nu": 0.3}},
        "sections": {"1": {"id": 1, "a": 0.01, "iy": 1e-4, "iz": 1e-4, "j": 2e-4}},
        "elements": elements,
        "supports": supports,
        "loads": loads,
        "constraints": cs,
    }))
    .unwrap()
}

type Constraints3D = fn(&dyn Fn([f64; 3]) -> [f64; 3]) -> Vec<Value>;

fn cases_3d() -> Vec<(&'static str, Constraints3D, Sup)> {
    vec![
        ("rigid link, master on the inclined node", |_| vec![
            json!({"type": "rigidLink", "masterNode": 4, "slaveNode": 7, "dofs": [0, 1, 2, 3, 4, 5]})], Sup::Mpc),
        ("rigid link, master on the inclined node (rotated)", |_| vec![
            json!({"type": "rigidLink", "masterNode": 4, "slaveNode": 7, "dofs": [0, 1, 2, 3, 4, 5]})], Sup::Rotated),
        ("rigid link, slave on the inclined node", |_| vec![
            json!({"type": "rigidLink", "masterNode": 7, "slaveNode": 4, "dofs": [0, 1, 2, 3, 4, 5]})], Sup::Rotated),
        ("rigid link translations only, slave on the inclined node", |_| vec![
            json!({"type": "rigidLink", "masterNode": 7, "slaveNode": 4, "dofs": [0, 1, 2]})], Sup::Rotated),
        ("equal DOF, master on the inclined node", |_| vec![
            json!({"type": "equalDOF", "masterNode": 4, "slaveNode": 7, "dofs": [0, 1, 2]})], Sup::Mpc),
        ("equal DOF, slave on the inclined node", |_| vec![
            json!({"type": "equalDOF", "masterNode": 7, "slaveNode": 4, "dofs": [0, 1, 2, 3, 4, 5]})], Sup::Rotated),
        ("equal DOF on ux only, slave on the inclined node", |_| vec![
            json!({"type": "equalDOF", "masterNode": 7, "slaveNode": 4, "dofs": [0]})], Sup::Mpc),
        ("equal DOF on uz only, master on the inclined node", |_| vec![
            json!({"type": "equalDOF", "masterNode": 4, "slaveNode": 7, "dofs": [2]})], Sup::Mpc),
        ("eccentric connection, master on the inclined node", |r| {
            let o = r([1.5, 0.8, 1.0]);
            vec![json!({"type": "eccentricConnection", "masterNode": 4, "slaveNode": 7,
                        "offsetX": o[0], "offsetY": o[1], "offsetZ": o[2]})]
        }, Sup::Rotated),
        ("eccentric connection, slave on the inclined node", |r| {
            let o = r([-1.5, -0.8, -1.0]);
            vec![json!({"type": "eccentricConnection", "masterNode": 7, "slaveNode": 4,
                        "offsetX": o[0], "offsetY": o[1], "offsetZ": o[2]})]
        }, Sup::Rotated),
        ("diaphragm, master on the inclined node", |_| vec![
            json!({"type": "diaphragm", "masterNode": 4, "slaveNodes": [7], "plane": "XY"})], Sup::Mpc),
        // n·u4 = 0 eliminates uz4 (largest coefficient), not a diaphragm slave.
        ("diaphragm, slave on the inclined node", |_| vec![
            json!({"type": "diaphragm", "masterNode": 7, "slaveNodes": [4], "plane": "XY"})], Sup::Mpc),
        ("linear MPC on the inclined node's translations", |r| {
            let (a, b, t) = (r([0.3, -0.5, 1.0]), r([1.0, 0.2, -0.4]), r([0.0, 0.7, 0.3]));
            vec![json!({"type": "linearMPC", "terms": [
                {"nodeId": 4, "dof": 0, "coefficient": a[0]}, {"nodeId": 4, "dof": 1, "coefficient": a[1]},
                {"nodeId": 4, "dof": 2, "coefficient": a[2]},
                {"nodeId": 7, "dof": 0, "coefficient": -b[0]}, {"nodeId": 7, "dof": 1, "coefficient": -b[1]},
                {"nodeId": 7, "dof": 2, "coefficient": -b[2]},
                {"nodeId": 7, "dof": 3, "coefficient": t[0]}, {"nodeId": 7, "dof": 4, "coefficient": t[1]},
                {"nodeId": 7, "dof": 5, "coefficient": t[2]}]})]
        }, Sup::Rotated),
    ]
}

fn mpc_reference_3d(name: &str, c: Constraints3D, sub: usize) -> SolverInput3D {
    if name.starts_with("equal DOF on ux only, slave") {
        return model_3d(Sup::Mpc, sub, &|_| vec![
            json!({"type": "equalDOF", "masterNode": 4, "slaveNode": 7, "dofs": [0]})]);
    }
    model_3d(Sup::Mpc, sub, &c)
}

fn disp_3d(res: &AnalysisResults3D, node: usize) -> [f64; 6] {
    let d = res.displacements.iter().find(|d| d.node_id == node).unwrap();
    [d.ux, d.uy, d.uz, d.rx, d.ry, d.rz]
}

fn compare_3d(what: &str, a: &[Displacement3D], b: &[Displacement3D], rotated: bool) {
    let qt = tr3(&q3());
    let scale = a.iter().fold(0.0f64, |m, d| [d.ux, d.uy, d.uz, d.rx, d.ry, d.rz].iter().fold(m, |m, v| m.max(v.abs())));
    assert!(scale > 1e-9, "{what}: no response");
    for db in b {
        let da = a.iter().find(|d| d.node_id == db.node_id).unwrap();
        let (u, t) = ([db.ux, db.uy, db.uz], [db.rx, db.ry, db.rz]);
        let (u, t) = if rotated { (mul3(&qt, u), mul3(&qt, t)) } else { (u, t) };
        let got = [da.ux, da.uy, da.uz, da.rx, da.ry, da.rz];
        let want = [u[0], u[1], u[2], t[0], t[1], t[2]];
        for k in 0..6 {
            assert_close(&format!("{what} node {} dof {k}", db.node_id), got[k], want[k], scale, 1e-8);
        }
    }
}

fn check_support_3d(what: &str, res: &AnalysisResults3D) {
    let u4 = disp_3d(res, 4);
    let scale = res.displacements.iter().fold(0.0f64, |m, d| m.max(d.ux.abs()).max(d.uy.abs()).max(d.uz.abs()));
    assert_close(&format!("{what}: support n·u4"), N3[0] * u4[0] + N3[1] * u4[1] + N3[2] * u4[2], 0.0, scale, 1e-9);
}

#[test]
fn linear_static_3d_constraints_on_inclined_support_match_references() {
    for sub in [1, 6] {
        for (name, c, reference) in cases_3d() {
            let what = format!("sub {sub}, {name}");
            let a = solve_3d(&model_3d(Sup::Inclined, sub, &c)).unwrap_or_else(|e| panic!("{what}: {e}"));
            check_support_3d(&what, &a);
            let b_input = if reference == Sup::Mpc { mpc_reference_3d(name, c, sub) } else { model_3d(reference, sub, &c) };
            let b = solve_3d(&b_input).unwrap_or_else(|e| panic!("{what} reference: {e}"));
            compare_3d(&what, &a.displacements, &b.displacements, reference == Sup::Rotated);
        }
    }
}

#[test]
fn linear_static_3d_rigid_link_holds_at_inclined_support() {
    for (master, slave) in [(4, 7), (7, 4)] {
        let input = model_3d(Sup::Inclined, 1, &|_| vec![
            json!({"type": "rigidLink", "masterNode": master, "slaveNode": slave, "dofs": [0, 1, 2, 3, 4, 5]})]);
        let res = solve_3d(&input).unwrap();
        let (m, s) = (disp_3d(&res, master), disp_3d(&res, slave));
        let (pm, ps) = (&input.nodes[&master.to_string()], &input.nodes[&slave.to_string()]);
        let d = [ps.x - pm.x, ps.y - pm.y, ps.z - pm.z];
        let w = [m[3], m[4], m[5]];
        let wxr = [w[1] * d[2] - w[2] * d[1], w[2] * d[0] - w[0] * d[2], w[0] * d[1] - w[1] * d[0]];
        let scale = m.iter().chain(&s).fold(0.0f64, |a, v| a.max(v.abs()));
        for k in 0..3 {
            assert_close(&format!("{master}->{slave} u{k}"), s[k], m[k] + wxr[k], scale, 1e-10);
            assert_close(&format!("{master}->{slave} r{k}"), s[3 + k], m[3 + k], scale, 1e-10);
        }
        check_support_3d("rigid link", &res);
        let (mut f, mut r) = ([0.0; 3], [0.0; 3]);
        for l in &input.loads {
            if let SolverLoad3D::Nodal(n) = l {
                f[0] += n.fx; f[1] += n.fy; f[2] += n.fz;
            }
        }
        for x in &res.reactions {
            r[0] += x.fx; r[1] += x.fy; r[2] += x.fz;
        }
        for k in 0..3 {
            assert_close(&format!("ΣF{k}"), r[k] + f[k], 0.0, 50.0, 1e-9);
        }
        let r4 = res.reactions.iter().find(|x| x.node_id == 4).unwrap();
        let along = r4.fx * N3[0] + r4.fy * N3[1] + r4.fz * N3[2];
        let mag = (r4.fx * r4.fx + r4.fy * r4.fy + r4.fz * r4.fz).sqrt();
        assert_close("support reaction along the normal", along.abs(), mag, mag, 1e-9);
    }
}

