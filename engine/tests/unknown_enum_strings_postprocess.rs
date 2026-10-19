//! Enum-like string inputs: an unknown value is refused with a message that
//! names the accepted values, never silently replaced by a default (absent
//! optional values keep their documented defaults). Each test checks the
//! accepted spelling still works and a typo is refused.

use dedaliano_engine::postprocess::cirsoc301_check::{check_flexure, Cirsoc301MemberData};
use dedaliano_engine::postprocess::diagrams::check_diagram_kind_2d;
use dedaliano_engine::postprocess::diagrams_3d::check_diagram_kind_3d;
use dedaliano_engine::postprocess::influence::{compute_influence_line, compute_influence_line_3d, InfluenceLineInput, InfluenceLineInput3D};
use dedaliano_engine::postprocess::section_stress::SectionGeometry;
use dedaliano_engine::solver::creep_shrinkage::{solve_creep_shrinkage_2d, CreepShrinkageInput};
use dedaliano_engine::solver::material_nonlinear::solve_nonlinear_material_2d;
use dedaliano_engine::types::NonlinearMaterialInput;
use serde_json::{json, Value};

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The 2D portal frame (nodes 1–4, supports at 1 and 4, elements 1–3).
fn portal_2d() -> Value {
    fixture("ex-portal-frame-input.json")
}

/// The 3D portal frame (nodes 1–8).
fn portal_3d() -> Value {
    fixture("ex-3d-portal-frame-input.json")
}

fn assert_refused<T: std::fmt::Debug>(r: Result<T, String>, needles: &[&str]) {
    let e = r.expect_err("an unknown value must be refused");
    for n in needles {
        assert!(e.contains(n), "error {e:?} should mention {n:?}");
    }
}

#[test]
fn creep_unknown_cement_class_is_refused() {
    let input = |class: &str| -> CreepShrinkageInput {
        serde_json::from_value(json!({
            "solver": portal_2d(),
            "creepParams": { "1": { "fc": 30.0, "rh": 70.0, "h0": 200.0, "cementClass": class },
                             "2": { "fc": 30.0, "rh": 70.0, "h0": 200.0 } },
            "timeSteps": [{ "tDays": 365.0 }]
        }))
        .unwrap()
    };
    assert!(solve_creep_shrinkage_2d(&input("R")).is_ok());
    // "r" was silently class N (s = 0.25) before.
    assert_refused(solve_creep_shrinkage_2d(&input("r")), &["'r'", "S, N, R"]);
}

#[test]
fn section_stress_unknown_shape_is_refused() {
    let geom = |shape: &str| {
        serde_json::from_value::<SectionGeometry>(json!({
            "shape": shape, "a": 0.01, "iy": 1e-4, "iz": 1e-5, "h": 0.3, "b": 0.15
        }))
        .map_err(|e| e.to_string())
    };
    assert!(geom("I").is_ok() && geom("C").is_ok() && geom("invL").is_ok());
    // "W" (or "i") was computed as a rectangle before.
    assert_refused(geom("W"), &["unknown section shape 'W'", "rect", "CHS"]);
}

#[test]
fn cirsoc301_unknown_load_position_is_refused() {
    let m = |pos: &str| -> Cirsoc301MemberData {
        serde_json::from_value(json!({
            "Fy": 235.0, "Fu": 370.0, "E": 200000.0,
            "A": 27.9e-4, "Iz": 1450e-8, "Iy": 81.3e-8, "h": 0.18, "b": 0.082, "tw": 0.0069, "tf": 0.0104,
            "Zx": 187e-6, "Sx": 161e-6, "J": 7.89e-8, "L": 5.0, "Lb": 5.0, "Cb": 1.14, "Cw": 5924e-12,
            "loadPosition": pos
        }))
        .unwrap()
    };
    let top = check_flexure(&m("topFlange"), 10.0, true);
    let web = check_flexure(&m("web"), 10.0, true);
    assert!(top.not_evaluated.is_none() && web.not_evaluated.is_none());
    assert!(web.capacity > top.capacity, "web loading is the less severe case");
    // "top" was read as web loading (the unconservative case) before.
    let typo = check_flexure(&m("top"), 10.0, true);
    assert!(typo.not_evaluated.is_some() && !typo.pass, "{typo:?}");
    let s = serde_json::to_string(&typo).unwrap();
    assert!(s.contains("loadPositionUnknown") && s.contains("top"), "{s}");
}

#[test]
fn influence_line_unknown_quantity_or_target_is_refused() {
    let il = |q: &str, node: Option<usize>, elem: Option<usize>| -> Result<(), String> {
        let input: InfluenceLineInput = serde_json::from_value(json!({
            "solver": portal_2d(), "quantity": q, "targetNodeId": node, "targetElementId": elem,
            "targetPosition": 0.5, "nPointsPerElement": 5
        }))
        .unwrap();
        compute_influence_line(&input).map(|_| ())
    };
    assert!(il("Rz", Some(1), None).is_ok());
    assert!(il("M", None, Some(2)).is_ok());
    // All of these gave an influence line of zeros before.
    assert_refused(il("R", Some(1), None), &["unknown influence-line quantity 'R'", "Rz", "V, M"]);
    assert_refused(il("Rz", None, None), &["targetNodeId"]);
    assert_refused(il("Rz", Some(2), None), &["node 2 has no support"]);
    assert_refused(il("Rz", Some(99), None), &["node 99 does not exist"]);
    assert_refused(il("M", None, None), &["targetElementId"]);
    assert_refused(il("M", None, Some(99)), &["element 99 does not exist"]);

    let il3 = |q: &str, node: Option<usize>, elem: Option<usize>| -> Result<(), String> {
        let input: InfluenceLineInput3D = serde_json::from_value(json!({
            "solver": portal_3d(), "quantity": q, "targetNodeId": node, "targetElementId": elem,
            "targetPosition": 0.5, "nPointsPerElement": 3
        }))
        .unwrap();
        compute_influence_line_3d(&input).map(|_| ())
    };
    assert!(il3("Fz", Some(1), None).is_ok());
    assert_refused(il3("Rz", Some(1), None), &["unknown influence-line quantity 'Rz'", "Fz", "My_diag"]);
}

#[test]
fn nonlinear_material_unknown_model_type_is_refused() {
    let input = |t: &str| -> NonlinearMaterialInput {
        serde_json::from_value(json!({
            "solver": portal_2d(),
            "materialModels": { "1": { "modelType": t, "fy": 250e3 }, "2": { "modelType": "bilinear", "fy": 25e3 } },
            "sectionCapacities": { "1": { "np": 1e4, "mp": 1e3 }, "2": { "np": 1e4, "mp": 1e3 }, "3": { "np": 1e4, "mp": 1e3 } }
        }))
        .unwrap()
    };
    assert!(solve_nonlinear_material_2d(&input("elastic_perfectly_plastic")).is_ok());
    // Any name ran the same bilinear law before.
    assert_refused(solve_nonlinear_material_2d(&input("trilinear")), &["'trilinear'", "elastic_perfectly_plastic, bilinear"]);
}

#[test]
fn diagram_value_unknown_kind_is_refused() {
    assert!(check_diagram_kind_2d("moment").is_ok());
    // compute_diagram_value_at (WASM) returned 0 for these before.
    assert_refused(check_diagram_kind_2d("Moment"), &["'Moment'", "moment, shear, axial"]);
    assert!(check_diagram_kind_3d("momentZ").is_ok());
    assert_refused(check_diagram_kind_3d("moment"), &["'moment'", "momentY", "torsion"]);
}
