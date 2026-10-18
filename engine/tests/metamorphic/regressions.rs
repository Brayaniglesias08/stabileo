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

#[test]
#[ignore]
fn probe_plastic_portal() {
    use dedaliano_engine::types::*;
    let f = std::env::var("PFIX").unwrap_or("ex-frame-cirsoc-dl-case2-input".into());
    let text = std::fs::read_to_string(format!("{}/tests/fixtures/{f}.json", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let s: SolverInput = serde_json::from_str(&text).unwrap();
    let c = crate::case::Case2::new("p", s);
    for c in [c.clone(), c.iso(&dedaliano_engine::metamorphic::transform::Iso2::rotation(0.3)).unwrap()] {
        let sections = c.s.sections.values().map(|x| {
            let (b, h) = crate::ana2::rect(x.a, x.iz);
            let mat = c.s.elements.values().find(|e| e.section_id == x.id).map_or(1, |e| e.material_id);
            (x.id.to_string(), PlasticSectionData { a: x.a, iz: x.iz, material_id: mat, b: Some(b), h: Some(h) })
        }).collect();
        let materials = c.s.materials.values().map(|x| (x.id.to_string(), PlasticMaterialData { fy: Some(250.0) })).collect();
        let r = dedaliano_engine::solver::plastic::solve_plastic_2d(&PlasticInput { solver: c.s.clone(), sections, materials, max_hinges: None, mp_overrides: None }).unwrap();
        println!("factor {} mech {} warnings {:?}", r.collapse_factor, r.is_mechanism, r.warnings);
        for h in r.hinges.iter().take(6) { println!("  {:?}", h); }
    }
}
