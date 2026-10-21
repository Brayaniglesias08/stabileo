//! Containment of the known issues (src/known_issues.rs): a result the issue can make wrong
//! carries a `KnownIssue` warning naming it, and a result it cannot affect carries none.

mod common;
use common::{make_3d_beam, make_beam};
use dedaliano_engine::known_issues::{for_3d, for_ssi, Analysis};
use dedaliano_engine::solver::linear::solve_2d;
use dedaliano_engine::solver::material_nonlinear::solve_nonlinear_material_2d;
use dedaliano_engine::solver::soil_curves::SoilCurve;
use dedaliano_engine::types::*;
use std::collections::HashMap;

const E: f64 = 200_000.0;

fn ids(d: &[StructuredDiagnostic]) -> Vec<String> {
    d.iter().filter(|x| x.code == DiagnosticCode::KnownIssue).map(|x| x.phase.clone().unwrap_or_default()).collect()
}

fn tip(fz: f64) -> Vec<SolverLoad> {
    vec![SolverLoad::Nodal(SolverNodalLoad { node_id: 5, fx: 0.0, fz, my: 0.0 })]
}

#[test]
fn a_plain_frame_carries_no_known_issue() {
    let r = solve_2d(&make_beam(4, 4.0, E, 0.045, 3.375e-4, "fixed", None, tip(-10.0))).unwrap();
    assert!(ids(&r.structured_diagnostics).is_empty(), "{:?}", ids(&r.structured_diagnostics));
}

#[test]
fn a_frame_temperature_load_is_flagged_on_its_members() {
    let mut loads = tip(-10.0);
    loads.push(SolverLoad::Thermal(SolverThermalLoad { element_id: 2, dt_uniform: 20.0, dt_gradient: 0.0 }));
    let r = solve_2d(&make_beam(4, 4.0, E, 0.045, 3.375e-4, "fixed", None, loads)).unwrap();
    let d = r.structured_diagnostics.iter().find(|x| x.phase.as_deref() == Some("FR-4")).expect("FR-4");
    assert_eq!(d.severity, Severity::Warning);
    assert_eq!(d.value, Some(407.0));
    assert_eq!(d.element_ids, vec![2]);
    assert!(d.message.contains("#407"));
}

#[test]
fn material_nonlinear_results_are_flagged() {
    let input = NonlinearMaterialInput {
        solver: make_beam(4, 4.0, E, 0.045, 3.375e-4, "fixed", None, tip(-100.0)),
        material_models: HashMap::from([("1".into(), MaterialModel { model_type: "elastic_perfectly_plastic".into(), fy: 250.0, alpha: Some(0.01) })]),
        section_capacities: HashMap::from([("1".into(), SectionCapacity { np: 11_250.0, mp: 843.75, zp: None })]),
        max_iter: 50,
        tolerance: 1e-8,
        n_increments: 5,
    };
    let r = solve_nonlinear_material_2d(&input).unwrap();
    assert!(ids(&r.results.structured_diagnostics).contains(&"NL-5".to_string()));
}

fn beam3d() -> SolverInput3D {
    make_3d_beam(4, 4.0, E, 0.3, 0.01, 1e-4, 1e-4, 1e-5, vec![true; 6], None, vec![])
}

#[test]
fn curved_shells_and_warped_mitc9_and_arches_are_flagged_only_when_present() {
    let base = beam3d();
    assert!(ids(&for_3d(&base, Analysis::Linear)).is_empty());

    // A curved shell, named with its family.
    let mut s = base.clone();
    s.curved_shells.insert("7".into(), SolverCurvedShellElement { id: 7, nodes: [1, 2, 3, 4], material_id: 1, thickness: 0.2, normals: None });
    let d = for_3d(&s, Analysis::Linear);
    let sh6 = d.iter().find(|x| x.phase.as_deref() == Some("SH-6")).expect("SH-6");
    assert_eq!((sh6.element_ids.clone(), sh6.element_kind), (vec![7], Some(ElementKind::CurvedShell)));

    // MITC9: flat is fine, warped is flagged.
    let add_nodes = |s: &mut SolverInput3D, pts: &[[f64; 3]]| {
        for (k, p) in pts.iter().enumerate() {
            let id = 100 + k;
            s.nodes.insert(id.to_string(), SolverNode3D { id, x: p[0], y: p[1], z: p[2] });
        }
    };
    let flat = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.0, 0.0], [1.0, 0.5, 0.0], [0.5, 1.0, 0.0], [0.0, 0.5, 0.0], [0.5, 0.5, 0.0]];
    let q9 = SolverQuad9Element { id: 9, nodes: [100, 101, 102, 103, 104, 105, 106, 107, 108], material_id: 1, thickness: 0.1 };
    let mut s = base.clone();
    add_nodes(&mut s, &flat);
    s.quad9s.insert("9".into(), q9.clone());
    assert!(!ids(&for_3d(&s, Analysis::Linear)).contains(&"SH-7".to_string()), "a flat MITC9 is not affected");
    let mut warped = flat;
    warped[8][2] = 0.2;
    let mut s = base.clone();
    add_nodes(&mut s, &warped);
    s.quad9s.insert("9".into(), q9);
    assert!(ids(&for_3d(&s, Analysis::Linear)).contains(&"SH-7".to_string()));

    // A plan curve is fine, an arch is flagged.
    let curve = || CurvedBeamInput { node_start: 1, node_mid: 200, node_end: 5, material_id: 1, section_id: 1, num_segments: 4, hinge_start: false, hinge_end: false };
    let mut s = base.clone();
    s.nodes.insert("200".into(), SolverNode3D { id: 200, x: 2.0, y: 1.0, z: 0.0 });
    s.curved_beams.push(curve());
    assert!(!ids(&for_3d(&s, Analysis::Linear)).contains(&"SP-15".to_string()), "a horizontal curve is not affected");
    let mut s = base;
    s.nodes.insert("200".into(), SolverNode3D { id: 200, x: 2.0, y: 0.0, z: 1.0 });
    s.curved_beams.push(curve());
    assert!(ids(&for_3d(&s, Analysis::Linear)).contains(&"SP-15".to_string()));
}

#[test]
fn cable_analysis_is_flagged() {
    assert!(ids(&for_3d(&beam3d(), Analysis::Cable)).contains(&"SP-12".to_string()));
    assert!(!ids(&for_3d(&beam3d(), Analysis::Linear)).contains(&"SP-12".to_string()));
}

#[test]
fn soil_curves_are_flagged_once_per_family() {
    let soft = SoilCurve::PySoftClay { su: 50.0, gamma_eff: 8.0, d: 0.6, depth: 2.0, eps_50: 0.01 };
    let tz = SoilCurve::Tz { t_ult: 40.0, z_ult: 0.005 };
    let d = for_ssi(&[&soft, &soft, &tz]);
    assert_eq!(ids(&d), vec!["AC-12".to_string(), "AC-16".to_string()]);
    assert!(for_ssi(&[]).is_empty());
}
