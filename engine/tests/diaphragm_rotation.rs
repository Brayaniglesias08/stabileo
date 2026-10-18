//! A rigid diaphragm rotates as one body: each slave node's in-plane rotation
//! equals the master's (#342).
//!
//! `Constraint::Diaphragm` tied each slave's in-plane translations to the
//! master's translation and rotation but left the slave's own in-plane
//! rotation (θz for an XY diaphragm) free. SAP2000/ETABS diaphragms and
//! OpenSees `rigidDiaphragm` tie it. Left free, a column under a rigid floor
//! twisted against nothing at its top: the storey was too soft in torsion and
//! the column end torques came out zero.
//!
//! Reference: the same storey with the floor made of beams that are rigid in
//! the floor's plane (axial and in-plane bending) and nearly free out of it
//! (out-of-plane bending and torsion), so the floor is a rigid in-plane body
//! by stiffness rather than by constraint.

use dedaliano_engine::solver::linear::{solve_2d, solve_3d};
use dedaliano_engine::solver::modal::solve_modal_3d;
use dedaliano_engine::types::*;
use serde_json::{json, Value};
use std::collections::HashMap;

const H: f64 = 3.5;
/// Column bases 1-4, tops 5-8, plan 6 m × 4 m.
const PLAN: [(f64, f64); 4] = [(0.0, 0.0), (6.0, 0.0), (6.0, 4.0), (0.0, 4.0)];

#[derive(Clone, Copy, PartialEq)]
enum Floor {
    /// Diaphragm constraint, master node 5, no floor members.
    Diaphragm,
    /// Floor beams rigid in plane, flexible out of plane; no constraint.
    RigidBeams,
}

fn storey(floor: Floor, top_support: Option<Value>) -> SolverInput3D {
    let mut nodes = serde_json::Map::new();
    for (i, &(x, y)) in PLAN.iter().enumerate() {
        let (b, t) = (i + 1, i + 5);
        nodes.insert(b.to_string(), json!({"id": b, "x": x, "y": y, "z": 0.0}));
        nodes.insert(t.to_string(), json!({"id": t, "x": x, "y": y, "z": H}));
    }
    let mut elements = serde_json::Map::new();
    for i in 1..=4 {
        elements.insert(i.to_string(), json!({"id": i, "type": "frame", "nodeI": i, "nodeJ": i + 4,
            "materialId": 1, "sectionId": 1}));
    }
    let mut constraints = Vec::new();
    match floor {
        Floor::Diaphragm => constraints.push(json!({"type": "diaphragm", "masterNode": 5,
            "slaveNodes": [6, 7, 8], "plane": "XY"})),
        Floor::RigidBeams => {
            // Local y vertical: Iy is bending in the floor's plane, Iz out of it.
            for (k, (a, b)) in [(5, 6), (6, 7), (7, 8), (8, 5)].into_iter().enumerate() {
                let id = 10 + k;
                elements.insert(id.to_string(), json!({"id": id, "type": "frame", "nodeI": a, "nodeJ": b,
                    "materialId": 2, "sectionId": 2, "localYx": 0.0, "localYy": 0.0, "localYz": 1.0}));
            }
        }
    }
    let mut supports = serde_json::Map::new();
    for i in 1..=4 {
        supports.insert(i.to_string(), json!({"nodeId": i, "rx": true, "ry": true, "rz": true,
            "rrx": true, "rry": true, "rrz": true}));
    }
    if let Some(s) = top_support {
        supports.insert("7".into(), s);
    }
    let loads = vec![
        json!({"type": "nodal", "data": {"nodeId": 6, "fx": 100.0, "fy": 0.0, "fz": 0.0, "mx": 0.0, "my": 0.0, "mz": 0.0}}),
        json!({"type": "nodal", "data": {"nodeId": 7, "fx": 0.0, "fy": 60.0, "fz": -20.0, "mx": 0.0, "my": 0.0, "mz": 15.0}}),
    ];
    serde_json::from_value(json!({
        "nodes": nodes,
        "materials": {"1": {"id": 1, "e": 30000.0, "nu": 0.2}, "2": {"id": 2, "e": 30000.0, "nu": 0.2}},
        "sections": {
            // 0.3 m × 0.5 m column: unequal bending stiffness, real torsion.
            "1": {"id": 1, "a": 0.15, "iy": 3.125e-3, "iz": 1.125e-3, "j": 2.8e-3},
            "2": {"id": 2, "a": 50.0, "iy": 50.0, "iz": 1e-9, "j": 1e-9}
        },
        "elements": elements,
        "supports": supports,
        "loads": loads,
        "constraints": constraints,
    }))
    .unwrap()
}

fn disp(res: &AnalysisResults3D, node: usize) -> [f64; 6] {
    let d = res.displacements.iter().find(|d| d.node_id == node).unwrap();
    [d.ux, d.uy, d.uz, d.rx, d.ry, d.rz]
}

fn forces(res: &AnalysisResults3D, elem: usize) -> [f64; 12] {
    let f = res.element_forces.iter().find(|f| f.element_id == elem).unwrap();
    [f.n_start, f.vy_start, f.vz_start, f.mx_start, f.my_start, f.mz_start,
     f.n_end, f.vy_end, f.vz_end, f.mx_end, f.my_end, f.mz_end]
}

fn close(what: &str, a: f64, b: f64, scale: f64, rel: f64) {
    assert!((a - b).abs() <= rel * scale, "{what}: {a} vs {b} (scale {scale:e}, rel {rel:e})");
}

#[test]
fn slave_in_plane_rotation_equals_master_rotation() {
    let res = solve_3d(&storey(Floor::Diaphragm, None)).unwrap();
    let rz5 = disp(&res, 5)[5];
    assert!(rz5.abs() > 1e-6, "the load twists the storey: rz5 = {rz5}");
    for n in [6, 7, 8] {
        close(&format!("rz{n} = rz5"), disp(&res, n)[5], rz5, rz5.abs(), 1e-12);
    }
    // Out-of-plane rotations stay each node's own.
    assert!((disp(&res, 6)[3] - disp(&res, 8)[3]).abs() > 1e-9);
}

#[test]
fn diaphragm_matches_floor_of_in_plane_rigid_beams_static() {
    let a = solve_3d(&storey(Floor::Diaphragm, None)).unwrap();
    let b = solve_3d(&storey(Floor::RigidBeams, None)).unwrap();
    let uscale = (5..=8).flat_map(|n| disp(&b, n)).fold(0.0f64, |m, v| m.max(v.abs()));
    for n in 5..=8 {
        let (da, db) = (disp(&a, n), disp(&b, n));
        for k in 0..6 {
            close(&format!("node {n} dof {k}"), da[k], db[k], uscale, 2e-4);
        }
    }
    // Column end forces, torques included: the columns share the storey
    // torque by their torsional stiffness, which a free slave θz ignored.
    let fscale = (1..=4).flat_map(|e| forces(&b, e)).fold(0.0f64, |m, v| m.max(v.abs()));
    for e in 1..=4 {
        let (fa, fb) = (forces(&a, e), forces(&b, e));
        for k in 0..12 {
            close(&format!("column {e} force {k}"), fa[k], fb[k], fscale, 2e-4);
        }
        assert!(fb[3].abs() > 1e-3 * fscale, "column {e} carries torque in the reference: {}", fb[3]);
    }
}

#[test]
fn diaphragm_matches_floor_of_in_plane_rigid_beams_modal() {
    let densities: HashMap<String, f64> = HashMap::from([("1".into(), 2500.0)]);
    let a = solve_modal_3d(&storey(Floor::Diaphragm, None), &densities, 4).unwrap();
    let b = solve_modal_3d(&storey(Floor::RigidBeams, None), &densities, 4).unwrap();
    // Sway X, sway Y and torsion: the torsional period is the one a free
    // slave θz got wrong (the columns' own twist stiffness was lost).
    for k in 0..3 {
        let (ta, tb) = (a.modes[k].period, b.modes[k].period);
        close(&format!("period {}", k + 1), ta, tb, tb, 1e-4);
    }
    let torsional = (0..3)
        .max_by(|&i, &j| {
            let rz = |m: &dedaliano_engine::solver::modal::ModeShape3D| {
                m.displacements.iter().find(|d| d.node_id == 5).unwrap().rz.abs()
            };
            rz(&a.modes[i]).total_cmp(&rz(&a.modes[j]))
        })
        .unwrap();
    let m = &a.modes[torsional];
    let rz5 = m.displacements.iter().find(|d| d.node_id == 5).unwrap().rz;
    for n in [6, 7, 8] {
        let rz = m.displacements.iter().find(|d| d.node_id == n).unwrap().rz;
        close(&format!("torsional mode rz{n} = rz5"), rz, rz5, rz5.abs(), 1e-9);
    }
}

#[test]
fn slave_on_inclined_support_rotates_with_master() {
    // Node 7 (a slave) on an inclined support: its translations are solved in
    // the support's frame, its θz still equals the master's.
    let n = [0.48, 0.36, 0.8];
    let sup = json!({"nodeId": 7, "rx": false, "ry": false, "rz": true, "rrx": false, "rry": false, "rrz": false,
        "isInclined": true, "normalX": n[0], "normalY": n[1], "normalZ": n[2]});
    let res = solve_3d(&storey(Floor::Diaphragm, Some(sup))).unwrap();
    let (u5, u7) = (disp(&res, 5), disp(&res, 7));
    let uscale = u5.iter().chain(u7.iter()).fold(0.0f64, |m, v| m.max(v.abs()));
    assert!(uscale > 1e-9);
    close("rz7 = rz5", u7[5], u5[5], uscale, 1e-9);
    close("n·u7 = 0", n[0] * u7[0] + n[1] * u7[1] + n[2] * u7[2], 0.0, uscale, 1e-9);
    // In-plane rigid-body kinematics about node 5.
    let (dx, dy) = (PLAN[2].0 - PLAN[0].0, PLAN[2].1 - PLAN[0].1);
    close("ux7", u7[0], u5[0] - dy * u5[5], uscale, 1e-9);
    close("uy7", u7[1], u5[1] + dx * u5[5], uscale, 1e-9);
}

#[test]
fn diaphragm_2d_slave_rotation_equals_master() {
    // 2D: the diaphragm is a rigid body in the model's plane.
    let input: SolverInput = serde_json::from_value(json!({
        "nodes": {"1": {"id": 1, "x": 0.0, "z": 0.0}, "2": {"id": 2, "x": 0.0, "z": 3.0},
                  "3": {"id": 3, "x": 5.0, "z": 3.0}, "4": {"id": 4, "x": 5.0, "z": 0.0}},
        "materials": {"1": {"id": 1, "e": 200000.0, "nu": 0.3}},
        "sections": {"1": {"id": 1, "a": 0.01, "iz": 1e-4}},
        "elements": {
            "1": {"id": 1, "type": "frame", "nodeI": 1, "nodeJ": 2, "materialId": 1, "sectionId": 1},
            "2": {"id": 2, "type": "frame", "nodeI": 4, "nodeJ": 3, "materialId": 1, "sectionId": 1}
        },
        "supports": {"1": {"id": 1, "nodeId": 1, "type": "fixed"}, "4": {"id": 4, "nodeId": 4, "type": "pinned"}},
        "loads": [{"type": "nodal", "data": {"nodeId": 2, "fx": 10.0, "fz": 0.0, "my": 0.0}}],
        "constraints": [{"type": "diaphragm", "masterNode": 2, "slaveNodes": [3], "plane": "XY"}]
    }))
    .unwrap();
    let res = solve_2d(&input).unwrap();
    let d = |n: usize| res.displacements.iter().find(|d| d.node_id == n).unwrap().clone();
    let (d2, d3) = (d(2), d(3));
    assert!(d2.ry.abs() > 1e-9);
    close("ry3 = ry2", d3.ry, d2.ry, d2.ry.abs(), 1e-12);
    close("ux3", d3.ux, d2.ux, d2.ux.abs(), 1e-9);
    close("uz3", d3.uz, d2.uz + 5.0 * d2.ry, d2.ux.abs(), 1e-9);
}
