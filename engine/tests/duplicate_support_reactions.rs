//! Several supports on one node (found by the `model_certify` fuzz target,
//! docs/verification/FUZZ_MUTANTS.md): the DOF numbering merges them, but the
//! reactions listed one entry per support, each carrying the node's full
//! restrained-DOF reaction, so ΣR double-counted it and a certified result was
//! out of global equilibrium by the applied load. Now each node gets a single
//! entry and its restrained reactions are counted once.
//!
//! (In 2D the DOF numbering lets the highest-id support of a node win, in 3D
//! it ORs their restraints; the tests use duplicates where both agree.)

use dedaliano_engine::certify::{certify_linear_2d_full, certify_linear_3d_full};
use dedaliano_engine::solver::linear::{solve_2d, solve_3d};
use dedaliano_engine::types::*;
use serde_json::json;

fn cantilever_2d(second_support: serde_json::Value) -> SolverInput {
    serde_json::from_value(json!({
        "nodes": { "1": { "id": 1, "x": 0.0, "z": 0.0 }, "2": { "id": 2, "x": 3.0, "z": 0.0 } },
        "materials": { "1": { "id": 1, "e": 200000.0, "nu": 0.3 } },
        "sections": { "1": { "id": 1, "a": 0.01, "iz": 1e-4 } },
        "elements": { "1": { "id": 1, "type": "frame", "nodeI": 1, "nodeJ": 2, "materialId": 1, "sectionId": 1 } },
        "supports": {
            "1": { "id": 1, "nodeId": 1, "type": "fixed" },
            "2": second_support
        },
        "loads": [{ "type": "nodal", "data": { "nodeId": 2, "fx": 123.0, "fz": -40.0, "my": 7.0 } }]
    }))
    .unwrap()
}

fn cantilever_3d() -> SolverInput3D {
    let sup = |dx: Option<f64>| {
        json!({ "nodeId": 1, "rx": true, "ry": true, "rz": true, "rrx": true, "rry": true, "rrz": true, "dx": dx })
    };
    serde_json::from_value(json!({
        "nodes": { "1": { "id": 1, "x": -0.5, "y": -0.5, "z": -1.0 }, "2": { "id": 2, "x": -0.5, "y": -35.5, "z": -0.5 } },
        "materials": { "1": { "id": 1, "e": 100000.0, "nu": 0.3 } },
        "sections": { "1": { "id": 1, "a": 0.01, "iy": 1e-4, "iz": 5e-5, "j": 3e-5 } },
        "elements": { "1": { "id": 1, "type": "frame", "nodeI": 2, "nodeJ": 1, "materialId": 1, "sectionId": 1 } },
        "supports": { "1": sup(Some(0.0)), "2": sup(None) },
        "loads": [{ "type": "nodal", "data": { "nodeId": 2, "fx": 123.0, "fy": 8.0, "fz": 10.0, "mx": 0.0, "my": 0.0, "mz": 0.0 } }]
    }))
    .unwrap()
}

fn assert_balanced(sum: &[f64], load: &[f64]) {
    for (s, f) in sum.iter().zip(load) {
        assert!((s + f).abs() < 1e-6 * (1.0 + f.abs()), "ΣR = {sum:?} does not balance the load {load:?}");
    }
}

#[test]
fn two_rigid_supports_on_one_node_2d() {
    for second in [
        json!({ "id": 2, "nodeId": 1, "type": "fixed" }),
        json!({ "id": 2, "nodeId": 1, "type": "fixed", "dx": 0.0 }),
    ] {
        let input = cantilever_2d(second);
        let r = solve_2d(&input).unwrap();
        assert_eq!(r.reactions.len(), 1, "one reaction entry per supported node: {:?}", r.reactions);
        let sum = [r.reactions[0].rx, r.reactions[0].rz];
        assert_balanced(&sum, &[123.0, -40.0]);
        let c = certify_linear_2d_full(&input).unwrap();
        assert!(c.certificate.is_certified());
        assert_balanced(&[c.results.reactions[0].rx, c.results.reactions[0].rz], &[123.0, -40.0]);
    }
}

#[test]
fn two_rigid_supports_on_one_node_3d() {
    let input = cantilever_3d();
    let r = solve_3d(&input).unwrap();
    assert_eq!(r.reactions.len(), 1, "one reaction entry per supported node: {:?}", r.reactions);
    let x = &r.reactions[0];
    assert_balanced(&[x.fx, x.fy, x.fz], &[123.0, 8.0, 10.0]);
    let c = certify_linear_3d_full(&input).unwrap();
    assert!(c.certificate.is_certified());
    let x = &c.results.reactions[0];
    assert_balanced(&[x.fx, x.fy, x.fz], &[123.0, 8.0, 10.0]);
}
