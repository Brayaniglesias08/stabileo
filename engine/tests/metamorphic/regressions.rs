//! Minimal reproductions of the violations the metamorphic harness found,
//! one per bug, each stated as the physics it must obey
//! (docs/verification/METAMORPHIC.md, "Violations found").

use crate::case::Case2;
use dedaliano_engine::metamorphic::transform::{transform_2d, Iso2, Iso3};
use dedaliano_engine::postprocess::influence;
use dedaliano_engine::solver::{constraints, corotational, kinematic, linear, load_cases, moving_loads, plastic, spectral, staged, winkler};
use dedaliano_engine::types::*;
use serde_json::json;

fn s2(v: serde_json::Value) -> SolverInput {
    serde_json::from_value(v).unwrap()
}

fn close(a: f64, b: f64, rel: f64, what: &str) {
    assert!((a - b).abs() <= rel * a.abs().max(b.abs()).max(1e-300), "{what}: {a} vs {b}");
}

fn disp(r: &AnalysisResults, n: usize) -> &Displacement {
    r.displacements.iter().find(|d| d.node_id == n).unwrap()
}

/// Cantilever 1–2 (fixed at 1), hinged at 2, with a moment at 2.
fn hinged_tip() -> SolverInput {
    s2(json!({
        "nodes": {"1": {"id": 1, "x": 0.0, "z": 0.0}, "2": {"id": 2, "x": 3.0, "z": 0.0}, "3": {"id": 3, "x": 3.0, "z": 2.0}},
        "materials": {"1": {"id": 1, "e": 200000.0, "nu": 0.3}},
        "sections": {"1": {"id": 1, "a": 0.01, "iz": 1e-4}},
        "elements": {
            "1": {"id": 1, "type": "frame", "nodeI": 1, "nodeJ": 2, "materialId": 1, "sectionId": 1, "hingeStart": false, "hingeEnd": true},
            "2": {"id": 2, "type": "frame", "nodeI": 1, "nodeJ": 3, "materialId": 1, "sectionId": 1}
        },
        "supports": {"1": {"id": 1, "nodeId": 1, "type": "fixed"}},
        "loads": [{"type": "nodal", "data": {"nodeId": 2, "fx": 0.0, "fz": 0.0, "my": 10.0}}]
    }))
}

/// A nodal moment on a rotation nothing stiffens is refused on every path
/// (the constrained path returned a rotation of ~1e5 rad).
#[test]
fn moment_on_unstiffened_rotation_refused_on_every_path() {
    let s = hinged_tip();
    assert!(linear::solve_2d(&s).is_err());
    let mut c = s.clone();
    c.constraints = vec![serde_json::from_value(json!({"type": "equalDOF", "masterNode": 3, "slaveNode": 2, "dofs": [0]})).unwrap()];
    let r = linear::solve_2d(&c);
    assert!(r.as_ref().is_err_and(|e| e.contains("node 2")), "{:?}", r.map(|r| disp(&r, 2).ry));
    let r = constraints::solve_constrained_2d(&constraints::ConstrainedInput { solver: c.clone(), constraints: c.constraints.clone() });
    assert!(r.is_err());
}

/// The prepared (factor-once) path has no constraint reduction and refuses a
/// constrained model instead of solving it without its constraints.
#[test]
fn prepared_static_refuses_constraints() {
    let mut s = hinged_tip();
    s.loads.clear();
    s.constraints = vec![serde_json::from_value(json!({"type": "equalDOF", "masterNode": 3, "slaveNode": 2, "dofs": [0, 1]})).unwrap()];
    assert!(linear::prepare_static_2d(&s).is_err());
    assert!(linear::prepare_static_2d_dense_reference(&s).is_err());
}

/// Two-span beam with a settlement at the middle roller.
fn settlement_beam() -> SolverInput {
    s2(json!({
        "nodes": {"1": {"id": 1, "x": 0.0, "z": 0.0}, "2": {"id": 2, "x": 4.0, "z": 0.0}, "3": {"id": 3, "x": 8.0, "z": 0.0}},
        "materials": {"1": {"id": 1, "e": 200000.0, "nu": 0.3}},
        "sections": {"1": {"id": 1, "a": 0.01, "iz": 1e-4}},
        "elements": {
            "1": {"id": 1, "type": "frame", "nodeI": 1, "nodeJ": 2, "materialId": 1, "sectionId": 1},
            "2": {"id": 2, "type": "frame", "nodeI": 2, "nodeJ": 3, "materialId": 1, "sectionId": 1}
        },
        "supports": {
            "1": {"id": 1, "nodeId": 1, "type": "fixed"},
            "2": {"id": 2, "nodeId": 2, "type": "rollerX", "dz": -0.01},
            "3": {"id": 3, "nodeId": 3, "type": "rollerX"}
        },
        "loads": [{"type": "distributed", "data": {"elementId": 1, "qI": -5.0, "qJ": -5.0}}]
    }))
}

/// A settlement on a roller rotated into an inclined support moves the
/// structure the same way in Winkler and material-nonlinear analyses as in
/// the linear solver (they assigned the raw global dz to the normal DOF).
#[test]
fn settlement_on_inclined_support_in_winkler_and_material_nonlinear() {
    let iso = Iso2::rotation(0.5);
    let s = transform_2d(&settlement_beam(), &iso).unwrap();
    assert!(s.supports.values().any(|x| x.support_type == "inclinedRoller"));
    let lin = linear::solve_2d(&s).unwrap();
    let w = winkler::solve_winkler_2d(&winkler::WinklerInput { solver: s.clone(), foundation_springs: vec![] }).unwrap();
    let inp = NonlinearMaterialInput {
        solver: s.clone(),
        material_models: [("1".to_string(), MaterialModel { model_type: "elastic_perfectly_plastic".into(), fy: 1e9, alpha: None })].into(),
        section_capacities: [("1".to_string(), SectionCapacity { np: 1e12, mp: 1e12, zp: None })].into(),
        max_iter: 50,
        tolerance: 1e-10,
        n_increments: 1,
    };
    let m = dedaliano_engine::solver::material_nonlinear::solve_nonlinear_material_2d(&inp).unwrap();
    for n in [2, 3] {
        for (got, name) in [(disp(&w, n), "Winkler"), (disp(&m.results, n), "material nonlinear")] {
            // On the 0.01 m settlement's scale.
            assert!((got.ux - disp(&lin, n).ux).abs() < 1e-9 && (got.uz - disp(&lin, n).uz).abs() < 1e-9, "{name} at node {n}");
        }
    }
}

/// Influence ordinates are unit-load responses: a support settlement in the
/// model does not shift them.
#[test]
fn influence_line_ignores_settlements() {
    let with = settlement_beam();
    let mut without = with.clone();
    for s in without.supports.values_mut() {
        s.dz = None;
    }
    let il = |s: &SolverInput| {
        influence::compute_influence_line(&influence::InfluenceLineInput {
            solver: s.clone(), quantity: "M".into(), target_node_id: None, target_element_id: Some(1), target_position: 0.5, n_points_per_element: 4,
        })
        .unwrap()
        .points
        .iter()
        .map(|p| p.value)
        .collect::<Vec<_>>()
    };
    let (a, b) = (il(&with), il(&without));
    for (x, y) in a.iter().zip(&b) {
        close(*x, *y, 1e-9, "influence ordinate");
    }
}

/// A deck member drawn right-to-left carries the train downwards, at the
/// right place: the envelope is that of the same deck drawn left-to-right
/// (the near-horizontal branch pushed the train up, and positions were
/// measured from the wrong end).
#[test]
fn moving_load_on_member_drawn_against_travel() {
    let mut s = settlement_beam();
    for sp in s.supports.values_mut() {
        sp.dz = None;
    }
    s.loads.clear();
    let mut rev = s.clone();
    let e = rev.elements.get_mut("2").unwrap();
    std::mem::swap(&mut e.node_i, &mut e.node_j);
    let run = |s: &SolverInput| {
        moving_loads::solve_moving_loads_2d(&MovingLoadInput {
            solver: s.clone(),
            train: LoadTrain { name: "t".into(), axles: vec![Axle { offset: 0.0, weight: 50.0 }, Axle { offset: 1.5, weight: 25.0 }] },
            step: Some(0.25),
            path_element_ids: Some(vec![1, 2]),
        })
        .unwrap()
    };
    let (a, b) = (run(&s), run(&rev));
    let (ea, eb) = (&a.elements["1"], &b.elements["1"]);
    close(ea.m_max_pos, eb.m_max_pos, 1e-9, "M+ on the forward member");
    close(ea.m_max_neg, eb.m_max_neg, 1e-9, "M- on the forward member");
    let (ea, eb) = (&a.elements["2"], &b.elements["2"]);
    // Reversed member: end moments map (mS, mE) → (−mE, −mS).
    close(ea.m_max_pos, -eb.m_max_neg, 1e-9, "M+ on the reversed member");
    close(ea.m_max_neg, -eb.m_max_pos, 1e-9, "M- on the reversed member");
}

/// rollerZ and guidedZ (aliases of rollerY and guidedY) count their
/// restraints in the kinematic degree.
#[test]
fn kinematic_counts_z_aliases() {
    let mut a = settlement_beam();
    a.supports.get_mut("3").unwrap().support_type = "rollerY".into();
    a.supports.get_mut("2").unwrap().support_type = "guidedY".into();
    a.supports.get_mut("2").unwrap().dz = None;
    let mut b = a.clone();
    b.supports.get_mut("3").unwrap().support_type = "rollerZ".into();
    b.supports.get_mut("2").unwrap().support_type = "guidedZ".into();
    assert_eq!(kinematic::analyze_kinematics_2d(&a).degree, kinematic::analyze_kinematics_2d(&b).degree);
}

/// The co-rotational element of a member drawn along −X: a cantilever
/// pointing left deflects as the mirror image of one pointing right (the
/// chord angle jumped by 2π across atan2's cut).
#[test]
fn corotational_member_along_negative_x() {
    let right = s2(json!({
        "nodes": {"1": {"id": 1, "x": 0.0, "z": 0.0}, "2": {"id": 2, "x": 2.0, "z": 0.0}, "3": {"id": 3, "x": 4.0, "z": 0.0}},
        "materials": {"1": {"id": 1, "e": 200000.0, "nu": 0.3}},
        "sections": {"1": {"id": 1, "a": 0.01, "iz": 1e-5}},
        "elements": {
            "1": {"id": 1, "type": "frame", "nodeI": 1, "nodeJ": 2, "materialId": 1, "sectionId": 1},
            "2": {"id": 2, "type": "frame", "nodeI": 2, "nodeJ": 3, "materialId": 1, "sectionId": 1}
        },
        "supports": {"1": {"id": 1, "nodeId": 1, "type": "fixed"}},
        "loads": [{"type": "nodal", "data": {"nodeId": 3, "fx": 0.0, "fz": -20.0, "my": 0.0}}]
    }));
    let left = transform_2d(&right, &Iso2::mirror_x()).unwrap();
    let a = corotational::solve_corotational_2d(&right, 100, 1e-10, 4, false).unwrap();
    let b = corotational::solve_corotational_2d(&left, 100, 1e-10, 4, false).unwrap();
    assert!(a.converged && b.converged);
    close(disp(&b.results, 3).ux, -disp(&a.results, 3).ux, 1e-6, "ux");
    close(disp(&b.results, 3).uz, disp(&a.results, 3).uz, 1e-6, "uz");
}

/// The co-rotational solve converges for a model far from the origin (the
/// chord was a difference of absolute deformed positions).
#[test]
fn corotational_far_from_origin() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ex-3d-building-case3-input.json")).unwrap();
    let s: SolverInput3D = serde_json::from_str(&text).unwrap();
    let far = dedaliano_engine::metamorphic::transform::transform_3d(&s, &Iso3::axis_permutation(0).translated([4.0e3, -4.0e3, 4.0e2])).unwrap();
    let a = corotational::solve_corotational_3d(&s, 100, 1e-10, 4, false).unwrap();
    let b = corotational::solve_corotational_3d(&far, 100, 1e-10, 4, false).unwrap();
    assert!(a.converged && b.converged, "far-away building did not converge");
    close(a.max_displacement, b.max_displacement, 1e-7, "max displacement");
}

fn plastic_2d(s: &SolverInput) -> Result<plastic::PlasticResult, String> {
    let c = Case2::new("p", s.clone());
    let sections = c
        .s
        .sections
        .values()
        .map(|x| {
            let (b, h) = crate::ana2::rect(x.a, x.iz);
            let mat = c.s.elements.values().find(|e| e.section_id == x.id).map_or(1, |e| e.material_id);
            (x.id.to_string(), PlasticSectionData { a: x.a, iz: x.iz, material_id: mat, b: Some(b), h: Some(h) })
        })
        .collect();
    let materials = c.s.materials.values().map(|x| (x.id.to_string(), PlasticMaterialData { fy: Some(250.0) })).collect();
    plastic::solve_plastic_2d(&PlasticInput { solver: c.s.clone(), sections, materials, max_hinges: None, mp_overrides: None })
}

/// A symmetric fixed portal under a beam UDL collapses by the beam mechanism
/// with hinges at both column tops (the weaker sections) and midspan:
/// λ·qL²/8 = Mp_beam + Mp_column. Twin column hinges reached Mp ~1e-9 apart
/// and formed in separate events; the second never formed and the reported
/// factor was 15% too high.
#[test]
fn plastic_symmetric_portal_collapses_by_its_mechanism() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ex-frame-cirsoc-dl-case2-input.json")).unwrap();
    let s: SolverInput = serde_json::from_str(&text).unwrap();
    let r = plastic_2d(&s).unwrap();
    // Mp = fy·b·h²/4: beam 0.2×0.5, columns 0.3×0.3, fy = 250 MPa; q = 5, L = 6.
    let (mp_b, mp_c) = (250e3 * 0.2 * 0.25 / 4.0, 250e3 * 0.3 * 0.09 / 4.0);
    let expected = (mp_b + mp_c) / (5.0 * 36.0 / 8.0);
    close(r.collapse_factor, expected, 1e-3, "collapse factor");
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

/// Plastic collapse refuses a model loaded only by imposed deformations, and
/// loads the structure carries without bending, instead of reporting a
/// factor of ~1e16.
#[test]
fn plastic_refuses_what_it_cannot_scale_to_collapse() {
    let mut s = settlement_beam();
    s.loads.clear();
    assert!(plastic_2d(&s).is_err_and(|e| e.contains("mechanical load")));
    let arch = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ex-three-hinge-arch-input.json")).unwrap();
    let a: SolverInput = serde_json::from_str(&arch).unwrap();
    let r = plastic_2d(&a);
    assert!(r.as_ref().is_err_and(|e| e.contains("without bending")), "{:?}", r.map(|r| r.collapse_factor));
}

/// A combination 1·A + 1·B equals the single solve of A + B: the model's
/// settlement enters once, not once per case.
#[test]
fn combination_counts_settlement_once() {
    let mut s = settlement_beam();
    s.loads.push(SolverLoad::Nodal(SolverNodalLoad { node_id: 3, fx: 1.0, fz: -3.0, my: 0.0 }));
    let lin = linear::solve_2d(&s).unwrap();
    let mut base = s.clone();
    let loads = std::mem::take(&mut base.loads);
    let inp = load_cases::MultiCaseInput {
        solver: base,
        load_cases: vec![
            load_cases::LoadCase { name: "A".into(), loads: vec![loads[0].clone()] },
            load_cases::LoadCase { name: "B".into(), loads: vec![loads[1].clone()] },
        ],
        combinations: vec![load_cases::CombinationDef { name: "C".into(), factors: [("A".to_string(), 1.0), ("B".to_string(), 1.0)].into() }],
    };
    let r = load_cases::solve_multi_case_2d(&inp).unwrap();
    for n in [2, 3] {
        close(disp(&r.combination_results[0].results, n).uz, disp(&lin, n).uz, 1e-9, "combined uz");
        close(disp(&r.combination_results[0].results, n).ry, disp(&lin, n).ry, 1e-9, "combined ry");
    }
}

/// A staged model whose loads arrive in two stages, every member and support
/// present from the first, ends where the linear solve does: the settlement
/// is applied once, not once per stage.
#[test]
fn staged_counts_settlement_once() {
    let mut s = settlement_beam();
    s.loads.push(SolverLoad::Nodal(SolverNodalLoad { node_id: 3, fx: 0.0, fz: -3.0, my: 0.0 }));
    let lin = linear::solve_2d(&s).unwrap();
    let st = |name: &str, loads: Vec<usize>, all: bool| ConstructionStage {
        name: name.into(),
        elements_added: if all { vec![1, 2] } else { vec![] },
        elements_removed: vec![],
        load_indices: loads,
        supports_added: if all { vec![1, 2, 3] } else { vec![] },
        supports_removed: vec![],
        prestress_loads: vec![],
    };
    let inp = StagedInput {
        nodes: s.nodes.clone(), materials: s.materials.clone(), sections: s.sections.clone(), elements: s.elements.clone(),
        supports: s.supports.clone(), loads: s.loads.clone(), constraints: vec![], stages: vec![st("1", vec![0], true), st("2", vec![1], false)],
    };
    let r = staged::solve_staged_2d(&inp).unwrap();
    for n in [2, 3] {
        close(disp(&r.final_results, n).uz, disp(&lin, n).uz, 1e-9, "staged uz");
    }
}

fn cantilever_3d(left_hand: bool) -> SolverInput3D {
    serde_json::from_value(json!({
        "nodes": {"1": {"id": 1, "x": 0.0, "y": 0.0, "z": 0.0}, "2": {"id": 2, "x": 3.0, "y": 0.0, "z": 0.0}},
        "materials": {"1": {"id": 1, "e": 200000.0, "nu": 0.3}},
        "sections": {"1": {"id": 1, "a": 0.01, "iy": 2e-4, "iz": 1e-4, "j": 5e-5}},
        "elements": {"1": {"id": 1, "type": "frame", "nodeI": 1, "nodeJ": 2, "materialId": 1, "sectionId": 1}},
        "supports": {"1": {"nodeId": 1, "rx": true, "ry": true, "rz": true, "rrx": true, "rry": true, "rrz": true}},
        "loads": [{"type": "distributed", "data": {"elementId": 1, "qYI": 2.0, "qYJ": 2.0, "qZI": -4.0, "qZJ": -1.0}}],
        "leftHand": left_hand
    }))
    .unwrap()
}

fn staged_one_stage_3d(s: &SolverInput3D) -> AnalysisResults3D {
    let v = json!({
        "nodes": s.nodes, "materials": s.materials, "sections": s.sections, "elements": s.elements,
        "supports": s.supports, "loads": s.loads, "leftHand": s.left_hand,
        "stages": [{"name": "all", "elementsAdded": [1], "loadIndices": [0], "supportsAdded": [1]}]
    });
    staged::solve_staged_3d(&staged::parse_staged_3d(&v.to_string()).unwrap()).unwrap().final_results
}

/// A one-stage 3D staged analysis reports what the linear solver does:
/// member end forces with fixed-end forces and the engine's sign convention,
/// reactions, and the left-handed axes the model asks for.
#[test]
fn staged_3d_matches_linear() {
    for lh in [false, true] {
        let s = cantilever_3d(lh);
        let lin = linear::solve_3d(&s).unwrap();
        let st = staged_one_stage_3d(&s);
        let (a, b) = (&lin.element_forces[0], &st.element_forces[0]);
        for (x, y, n) in [
            (a.n_start, b.n_start, "N"), (a.vy_start, b.vy_start, "Vy"), (a.vz_end, b.vz_end, "Vz"),
            (a.my_start, b.my_start, "My"), (a.mz_start, b.mz_start, "Mz"), (a.mz_end, b.mz_end, "Mz end"),
        ] {
            close(y, x, 1e-9, &format!("{n} (leftHand {lh})"));
        }
        let (ra, rb) = (&lin.reactions[0], &st.reactions[0]);
        close(rb.fy, ra.fy, 1e-9, "Fy");
        close(rb.fz, ra.fz, 1e-9, "Fz");
        close(rb.my, ra.my, 1e-9, "My");
        close(rb.mz, ra.mz, 1e-9, "Mz");
    }
}

/// CQC base shear is basis-independent: relabelling a model with repeated
/// frequencies (a square grid slab) does not change it (it was the SRSS of
/// the modal base shears whatever the rule).
#[test]
fn spectral_cqc_base_shear_independent_of_mode_basis() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ex-3d-grid-slab-input.json")).unwrap();
    let s: SolverInput3D = serde_json::from_str(&text).unwrap();
    let c = crate::case::Case3::new("g", s);
    let rl = dedaliano_engine::metamorphic::relabel::Relabel::random_3d(&c.s, 11);
    let shear = |c: &crate::case::Case3| {
        let md = dedaliano_engine::solver::modal::solve_modal_3d(&c.s, &c.dens, 6).unwrap();
        let modes = md
            .modes
            .iter()
            .map(|x| SpectralModeInput3D {
                frequency: x.frequency, period: x.period, omega: x.omega,
                displacements: x.displacements.iter().map(|d| SpectralModeDisp3D { node_id: d.node_id, ux: d.ux, uy: d.uy, uz: d.uz, rx: d.rx, ry: d.ry, rz: d.rz }).collect(),
                participation_x: x.participation_x, participation_y: x.participation_y, participation_z: x.participation_z,
                effective_mass_x: x.effective_mass_x, effective_mass_y: x.effective_mass_y, effective_mass_z: x.effective_mass_z,
            })
            .collect();
        spectral::solve_spectral_3d(&SpectralInput3D {
            solver: c.s.clone(), modes, densities: c.dens.clone(),
            spectrum: DesignSpectrum { name: "flat".into(), points: vec![SpectrumPoint { period: 0.0, sa: 0.3 }, SpectrumPoint { period: 10.0, sa: 0.3 }], in_g: Some(true) },
            direction: "X".into(), rule: Some("CQC".into()), xi: Some(0.05), importance_factor: None, reduction_factor: None, total_mass: None,
        })
        .unwrap()
        .base_shear
    };
    close(shear(&c.relabel(&rl, 11)), shear(&c), 1e-7, "base shear");
}

#[test]
#[ignore]
fn probe_units_3d() {
    use dedaliano_engine::metamorphic::units::{scale_3d, Units};
    let f = std::env::var("PFILE").unwrap();
    let s: SolverInput3D = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    let base = linear::solve_3d(&s).unwrap();
    let pick = |r: &AnalysisResults3D| r.displacements.iter().map(|d| (d.node_id, d.ux, d.uz, d.ry)).collect::<Vec<_>>();
    println!("base {:?}", pick(&base));
    for (l, fo) in [(1e-3, 1e3), (25.0, 1e-2), (0.3048, 4.448), (1.0, 7.0), (2.0, 1.0)] {
        let u = Units::froude(l, fo);
        let r = linear::solve_3d(&scale_3d(&s, &u)).unwrap();
        let v: Vec<_> = pick(&r).iter().map(|&(n, a, b, c)| (n, a / l, b / l, c)).collect();
        println!("{l} {fo}: {:?}", v);
    }
}

#[test]
#[ignore]
fn probe_krx() {
    let f = "tests/metamorphic_corpus/feature_matrix/3d-spring-support--rotational--spring-restrained-beam-with.json";
    let s: SolverInput3D = serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
    for k in 0..4 {
        let iso = Iso3::about_z(std::f64::consts::FRAC_PI_2 * k as f64);
        let t = dedaliano_engine::metamorphic::transform::transform_3d(&s, &iso).unwrap();
        let sup = t.supports.values().find(|x| x.node_id == 1).unwrap();
        println!("k={k} flags {:?} springs {:?} -> {:?}", (sup.rrx, sup.rry, sup.rrz), (sup.krx, sup.kry, sup.krz), linear::solve_3d(&t).map(|r| r.displacements.len()).map_err(|e| e.chars().take(80).collect::<String>()));
    }
}

/// Shell rotary inertia acts about the shell's in-plane axes: a cantilever
/// strip of MITC4 shells has the same frequencies lying flat and standing as
/// a wall (it was put on global rx, ry whatever the orientation).
#[test]
fn shell_rotary_inertia_follows_the_shell() {
    let f = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/metamorphic_corpus/feature_matrix/3d-quad-shell--mitc4--strip-cantilever------0-with.json");
    let s: SolverInput3D = serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
    let c = crate::case::Case3::new("strip", s);
    for k in [3, 7, 13] {
        let iso = Iso3::axis_permutation(k);
        let t = c.iso(&iso).unwrap();
        let a = dedaliano_engine::solver::modal::solve_modal_3d(&c.s, &c.dens, 6).unwrap();
        let b = dedaliano_engine::solver::modal::solve_modal_3d(&t.s, &t.dens, 6).unwrap();
        for (x, y) in a.modes.iter().zip(&b.modes) {
            close(y.frequency, x.frequency, 1e-7, "frequency");
        }
    }
}

#[test]
#[ignore]
fn probe_plastic_file() {
    let f = std::env::var("PFILE").unwrap();
    let s: SolverInput = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    match plastic_2d(&s) {
        Ok(r) => { println!("factor {} mech {} warn {:?}", r.collapse_factor, r.is_mechanism, r.warnings); for h in &r.hinges { println!("  {h:?}"); } }
        Err(e) => println!("err {e}"),
    }
}
