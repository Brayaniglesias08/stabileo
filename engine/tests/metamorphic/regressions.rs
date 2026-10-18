//! Regression tests for violations found by the harness.

#[test]
#[ignore]
fn dump_case() {
    let name = std::env::var("DUMP").unwrap();
    for c in crate::corpus::cases_2d(crate::corpus::Tier::Pr, 20261018) {
        if c.name == name {
            println!("{}", serde_json::to_string(&c.s).unwrap());
        }
    }
    for c in crate::corpus::cases_3d(crate::corpus::Tier::Pr, 20261018) {
        if c.name == name {
            println!("{}", serde_json::to_string(&c.s).unwrap());
        }
    }
}

#[test]
fn probe_2d_moment_on_hinged_node() {
    let s: dedaliano_engine::types::SolverInput = serde_json::from_value(serde_json::json!({
        "nodes": {"1": {"id": 1, "x": 0.0, "z": 0.0}, "2": {"id": 2, "x": 3.0, "z": 0.0}},
        "materials": {"1": {"id": 1, "e": 200000.0, "nu": 0.3}},
        "sections": {"1": {"id": 1, "a": 0.01, "iz": 1e-4}},
        "elements": {"1": {"id": 1, "type": "frame", "nodeI": 1, "nodeJ": 2, "materialId": 1, "sectionId": 1, "hingeStart": false, "hingeEnd": true}},
        "supports": {"1": {"id": 1, "nodeId": 1, "type": "fixed"}},
        "loads": [{"type": "nodal", "data": {"nodeId": 2, "fx": 0.0, "fz": 0.0, "my": 10.0}}]
    })).unwrap();
    let r = dedaliano_engine::solver::linear::solve_2d(&s);
    println!("{:?}", r.as_ref().map(|r| r.displacements.clone()).map_err(|e| e.clone()));
    let mut s = s;
    s.nodes.insert("3".into(), dedaliano_engine::types::SolverNode { id: 3, x: 3.0, z: 2.0 });
    s.elements.insert("2".into(), serde_json::from_value(serde_json::json!({"id": 2, "type": "frame", "nodeI": 1, "nodeJ": 3, "materialId": 1, "sectionId": 1})).unwrap());
    s.constraints = vec![serde_json::from_value(serde_json::json!({"type": "equalDOF", "masterNode": 3, "slaveNode": 2, "dofs": [0]})).unwrap()];
    let r = dedaliano_engine::solver::linear::solve_2d(&s);
    println!("{:?}", r.as_ref().map(|r| r.displacements.clone()).map_err(|e| e.clone()));
}
