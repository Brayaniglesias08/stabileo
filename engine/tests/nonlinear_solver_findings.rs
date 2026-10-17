//! Nonlinear and special solvers against independent references.
//!
//! Each test states the structure twice: once with the feature the solver
//! handles nonlinearly (a slack brace, a closed gap, a corotational frame in
//! the small-load limit, ...), and once as a plain linear model that is the
//! same structure without the feature (the slack brace removed, the closed
//! gap replaced by the spring and offset it amounts to). The two must agree.

mod common;

use common::*;
use dedaliano_engine::solver::contact::*;
use dedaliano_engine::solver::linear;
use dedaliano_engine::types::*;
use std::collections::HashMap;

fn close(a: f64, b: f64, rel: f64, what: &str) {
    let scale = a.abs().max(b.abs()).max(1e-9);
    assert!((a - b).abs() <= rel * scale, "{what}: {a} vs {b} (rel {:.3e})", (a - b).abs() / scale);
}

fn contact_input(solver: SolverInput) -> ContactInput {
    ContactInput {
        solver,
        element_behaviors: HashMap::new(),
        gap_elements: vec![],
        uplift_supports: vec![],
        max_iter: Some(40),
        tolerance: None,
        augmented_lagrangian: None,
        max_flips: None,
        damping_coefficient: None,
        al_max_iter: None,
        contact_type: ContactType::default(),
        node_to_surface_pairs: vec![],
    }
}

/// Compare every displacement, reaction and member force of `got` with `want`
/// (members missing from `want` must carry no force in `got`).
fn same_results_2d(got: &AnalysisResults, want: &AnalysisResults, rel: f64) {
    let umax = want.displacements.iter().map(|d| d.ux.abs().max(d.uz.abs())).fold(0.0, f64::max);
    for w in &want.displacements {
        let g = got.displacements.iter().find(|d| d.node_id == w.node_id).expect("node missing");
        assert!((g.ux - w.ux).abs() <= rel * umax, "node {} ux {} vs {}", w.node_id, g.ux, w.ux);
        assert!((g.uz - w.uz).abs() <= rel * umax, "node {} uz {} vs {}", w.node_id, g.uz, w.uz);
    }
    let rmax = want.reactions.iter().map(|r| r.rx.abs().max(r.rz.abs())).fold(0.0, f64::max);
    for w in &want.reactions {
        let g = got.reactions.iter().find(|r| r.node_id == w.node_id).expect("reaction missing");
        assert!((g.rx - w.rx).abs() <= rel * rmax, "node {} rx {} vs {}", w.node_id, g.rx, w.rx);
        assert!((g.rz - w.rz).abs() <= rel * rmax, "node {} rz {} vs {}", w.node_id, g.rz, w.rz);
        assert!((g.my - w.my).abs() <= rel * rmax.max(1.0), "node {} my {} vs {}", w.node_id, g.my, w.my);
    }
    let fmax = want.element_forces.iter().map(|f| f.n_start.abs().max(f.m_start.abs()).max(f.m_end.abs())).fold(0.0, f64::max);
    for g in &got.element_forces {
        match want.element_forces.iter().find(|f| f.element_id == g.element_id) {
            Some(w) => {
                for (a, b, what) in [(g.n_start, w.n_start, "N"), (g.v_start, w.v_start, "V"), (g.m_start, w.m_start, "Mi"), (g.m_end, w.m_end, "Mj")] {
                    assert!((a - b).abs() <= rel * fmax, "member {} {what} {a} vs {b}", g.element_id);
                }
            }
            None => {
                for v in [g.n_start, g.n_end, g.v_start, g.v_end, g.m_start, g.m_end] {
                    assert!(v.abs() <= rel * fmax, "inactive member {} carries {v}", g.element_id);
                }
            }
        }
    }
}

/// `storeys`-storey, one-bay frame (4 m × 3 m) with both diagonals of every
/// storey as trusses; returns the model and the ids of the braces that run
/// from bottom-right to top-left (shortened by a rightward sway).
fn braced_frame(storeys: usize, base: &str, fx: f64) -> (SolverInput, Vec<usize>) {
    let mut nodes = vec![];
    for s in 0..=storeys {
        nodes.push((2 * s + 1, 0.0, 3.0 * s as f64));
        nodes.push((2 * s + 2, 4.0, 3.0 * s as f64));
    }
    let mut elems = vec![];
    let mut id = 0;
    let mut compressed = vec![];
    for s in 0..storeys {
        let (bl, br, tl, tr) = (2 * s + 1, 2 * s + 2, 2 * s + 3, 2 * s + 4);
        id += 1; elems.push((id, "frame", bl, tl, 1, 1, false, false));
        id += 1; elems.push((id, "frame", br, tr, 1, 1, false, false));
        id += 1; elems.push((id, "frame", tl, tr, 1, 1, false, false));
        id += 1; elems.push((id, "truss", bl, tr, 1, 2, false, false));
        id += 1; elems.push((id, "truss", br, tl, 1, 2, false, false));
        compressed.push(id);
    }
    let loads = (1..=storeys)
        .map(|s| SolverLoad::Nodal(SolverNodalLoad { node_id: 2 * s + 1, fx, fz: -20.0, my: 0.0 }))
        .collect();
    let input = make_input(
        nodes,
        vec![(1, 200_000.0, 0.3)],
        vec![(1, 0.01, 1e-4), (2, 0.002, 1e-8)],
        elems,
        vec![(1, 1, base), (2, 2, base)],
        loads,
    );
    (input, compressed)
}

fn without(input: &SolverInput, ids: &[usize]) -> SolverInput {
    let mut m = input.clone();
    m.elements.retain(|_, e| !ids.contains(&e.id));
    m
}

fn tension_only(ci: &mut ContactInput, ids: &[usize]) {
    for (_, e) in ci.solver.elements.iter() {
        if e.elem_type == "truss" {
            ci.element_behaviors.insert(e.id.to_string(), "tension_only".into());
        }
    }
    let _ = ids;
}

/// A tension-only X-braced frame under sway: the compressed diagonals go
/// slack, and the answer is the linear solve of the frame without them. The
/// contact solver used to return the linear solve of the model as given,
/// slack braces included (both the displacements and the brace forces).
#[test]
fn contact_2d_slack_braces_match_frame_without_them() {
    for storeys in [1, 12] {
        let (model, compressed) = braced_frame(storeys, "pinned", 100.0);
        let mut ci = contact_input(model.clone());
        tension_only(&mut ci, &compressed);
        let r = solve_contact_2d(&ci).unwrap();
        assert!(r.converged, "{storeys} storeys: not converged");
        for &id in &compressed {
            let s = r.element_status.iter().find(|e| e.element_id == id).unwrap();
            assert_eq!(s.status, "inactive", "{storeys} storeys: brace {id}");
        }
        let reference = linear::solve_2d(&without(&model, &compressed)).unwrap();
        same_results_2d(&r.results, &reference, 1e-6);
    }
}

/// The same, with the frame on an inclined roller and a support settlement:
/// the iteration reads the displacements in global axes, and the prescribed
/// displacement enters the solve. Which braces go slack is checked against
/// the reference itself: every brace kept is in tension there, every brace
/// removed would be shortened by it.
#[test]
fn contact_2d_slack_braces_inclined_roller_and_settlement() {
    let (mut model, compressed) = braced_frame(2, "pinned", 100.0);
    for s in model.supports.values_mut() {
        if s.node_id == 2 {
            s.support_type = "inclinedRoller".into();
            s.angle = Some(0.35);
        }
        if s.node_id == 1 {
            s.dz = Some(-0.004);
        }
    }
    let mut ci = contact_input(model.clone());
    tension_only(&mut ci, &compressed);
    let r = solve_contact_2d(&ci).unwrap();
    assert!(r.converged);
    let slack: Vec<usize> = r.element_status.iter().filter(|e| e.status == "inactive").map(|e| e.element_id).collect();
    assert!(!slack.is_empty());
    let reference = linear::solve_2d(&without(&model, &slack)).unwrap();
    same_results_2d(&r.results, &reference, 1e-6);
    for e in model.elements.values().filter(|e| e.elem_type == "truss") {
        let elong = elongation_2d(&model, &reference, e.id);
        if slack.contains(&e.id) {
            assert!(elong < 0.0, "slack brace {} is stretched by the reference ({elong})", e.id);
        } else {
            assert!(elong > 0.0, "active brace {} is shortened by the reference ({elong})", e.id);
        }
    }
}

fn elongation_2d(model: &SolverInput, res: &AnalysisResults, eid: usize) -> f64 {
    let e = model.elements.values().find(|e| e.id == eid).unwrap();
    let node = |id: usize| model.nodes.values().find(|n| n.id == id).unwrap();
    let disp = |id: usize| res.displacements.iter().find(|d| d.node_id == id).unwrap();
    let (ni, nj) = (node(e.node_i), node(e.node_j));
    let (dx, dz) = (nj.x - ni.x, nj.z - ni.z);
    let l = (dx * dx + dz * dz).sqrt();
    ((disp(e.node_j).ux - disp(e.node_i).ux) * dx + (disp(e.node_j).uz - disp(e.node_i).uz) * dz) / l
}

/// Bar 1–2 pushed into a gap (opening g0) to a fixed node 3. With the gap
/// closed, node 2 carries the bar (EA/L) and the gap spring k from the point
/// the gap closes: u = (F + k·g0)/(EA/L + k). The solver used to stiffen
/// the gap from zero displacement (no g0 offset) and then report the linear
/// solve without the gap.
#[test]
fn contact_2d_gap_closes_at_its_opening() {
    let g0 = 0.002;
    let k = 5000.0;
    let f = 500.0;
    let model = make_input(
        vec![(1, 0.0, 0.0), (2, 1.0, 0.0), (3, 1.0 + g0, 0.0)],
        vec![(1, 200.0, 0.3)],
        vec![(1, 0.01, 1e-4)],
        vec![(1, "frame", 1, 2, 1, 1, false, false)],
        vec![(1, 1, "fixed"), (2, 3, "fixed")],
        vec![SolverLoad::Nodal(SolverNodalLoad { node_id: 2, fx: f, fz: 0.0, my: 0.0 })],
    );
    let mut ci = contact_input(model);
    ci.gap_elements.push(GapElement {
        id: 1, node_i: 2, node_j: 3, direction: 0, initial_gap: g0, stiffness: k,
        friction: None, friction_direction: None, friction_coefficient: None,
    });
    let r = solve_contact_2d(&ci).unwrap();
    assert!(r.converged);
    assert_eq!(r.gap_status[0].status, "closed");
    let ea_l = 200.0 * 1000.0 * 0.01 / 1.0;
    let u = (f + k * g0) / (ea_l + k);
    let d2 = r.results.displacements.iter().find(|d| d.node_id == 2).unwrap();
    close(d2.ux, u, 1e-9, "node 2 ux");
    let gap_force = k * (u - g0);
    close(r.gap_status[0].force, -gap_force, 1e-9, "gap force");
    let r1 = r.results.reactions.iter().find(|r| r.node_id == 1).unwrap();
    let r3 = r.results.reactions.iter().find(|r| r.node_id == 3).unwrap();
    close(r1.rx, -ea_l * u, 1e-9, "bar support reaction");
    close(r3.rx, -gap_force, 1e-9, "gap support reaction");
    let bar = r.results.element_forces.iter().find(|e| e.element_id == 1).unwrap();
    close(bar.n_start.abs(), ea_l * u, 1e-9, "bar force");
}

/// A slave node hanging g0 above a fixed master segment, pushed down: the
/// penalty acts from the moment of contact, u = −(F + k·g0)/(EA/L + k).
#[test]
fn contact_2d_node_to_surface_closes_at_its_opening() {
    let g0 = 0.01;
    let k = 5000.0;
    let f = 100.0;
    let l = 2.0 - g0;
    let model = make_input(
        vec![(1, 0.0, 2.0), (2, 0.0, g0), (3, -1.0, 0.0), (4, 1.0, 0.0)],
        vec![(1, 200.0, 0.3)],
        vec![(1, 0.01, 1e-4)],
        vec![(1, "frame", 1, 2, 1, 1, false, false)],
        vec![(1, 1, "fixed"), (2, 3, "fixed"), (3, 4, "fixed")],
        vec![SolverLoad::Nodal(SolverNodalLoad { node_id: 2, fx: 0.0, fz: -f, my: 0.0 })],
    );
    let mut ci = contact_input(model);
    ci.contact_type = ContactType::NodeToSurface;
    ci.node_to_surface_pairs.push(NodeToSurfacePair {
        slave_node: 2,
        master_segment: MasterSegment { id: 1, node_a: 3, node_b: 4 },
        stiffness: k,
        friction_coefficient: None,
    });
    let r = solve_contact_2d(&ci).unwrap();
    assert!(r.converged);
    assert_eq!(r.node_to_surface_status[0].status, "closed");
    let ea_l = 200.0 * 1000.0 * 0.01 / l;
    let u = -(f + k * g0) / (ea_l + k);
    let d2 = r.results.displacements.iter().find(|d| d.node_id == 2).unwrap();
    close(d2.uz, u, 1e-9, "slave uz");
    close(r.node_to_surface_status[0].normal_force, k * (-u - g0), 1e-9, "contact force");
    let rz: f64 = r.results.reactions.iter().map(|r| r.rz).sum();
    close(rz, f, 1e-9, "vertical equilibrium");
}

/// Uplift (compression-only) supports were kept as two-way restraints. An
/// overturned portal lifts off its windward base: the answer is the linear
/// solve with that base free to rise (fixed → guidedZ), whose own vertical
/// reaction at the lifted base is zero and whose base is above its seat.
#[test]
fn contact_2d_uplift_support_lifts_off() {
    let model = make_input(
        vec![(1, 0.0, 0.0), (2, 6.0, 0.0), (3, 0.0, 4.0), (4, 6.0, 4.0)],
        vec![(1, 200_000.0, 0.3)],
        vec![(1, 0.01, 1e-4)],
        vec![
            (1, "frame", 1, 3, 1, 1, false, false),
            (2, "frame", 2, 4, 1, 1, false, false),
            (3, "frame", 3, 4, 1, 1, false, false),
        ],
        vec![(1, 1, "fixed"), (2, 2, "fixed")],
        vec![
            SolverLoad::Nodal(SolverNodalLoad { node_id: 3, fx: 200.0, fz: -10.0, my: 0.0 }),
            SolverLoad::Nodal(SolverNodalLoad { node_id: 4, fx: 0.0, fz: -10.0, my: 0.0 }),
        ],
    );
    let linear_rz1 = linear::solve_2d(&model).unwrap().reactions.iter().find(|r| r.node_id == 1).unwrap().rz;
    assert!(linear_rz1 < 0.0, "the two-way support pulls down ({linear_rz1})");
    let mut ci = contact_input(model.clone());
    ci.uplift_supports = vec![1, 2];
    let r = solve_contact_2d(&ci).unwrap();
    assert!(r.converged);
    let mut lifted = model.clone();
    for s in lifted.supports.values_mut() {
        if s.node_id == 1 {
            s.support_type = "guidedZ".into();
        }
    }
    let want = linear::solve_2d(&lifted).unwrap();
    same_results_2d(&r.results, &want, 1e-9);
    assert!(r.results.displacements.iter().find(|d| d.node_id == 1).unwrap().uz > 0.0);
    let rz: f64 = r.results.reactions.iter().map(|r| r.rz).sum();
    close(rz, 20.0, 1e-9, "vertical equilibrium");
}

/// 3D uplift supports are refused by name rather than kept rigid.
#[test]
fn contact_3d_refuses_uplift_supports() {
    let solver = make_3d_beam(2, 4.0, 200_000.0, 0.3, 0.01, 1e-4, 1e-4, 1e-4,
        vec![true; 6], Some(vec![true; 6]), vec![]);
    let ci = ContactInput3D {
        solver, element_behaviors: HashMap::new(), gap_elements: vec![], uplift_supports: vec![1],
        max_iter: None, tolerance: None, augmented_lagrangian: None, max_flips: None,
        damping_coefficient: None, al_max_iter: None,
    };
    let err = solve_contact_3d(&ci).unwrap_err();
    assert!(err.contains("uplift"), "{err}");
}

/// A closed gap at an inclined roller: the gap acts along global X, and the
/// roller's DOFs are numbered in its own frame, so the gap's spring has to
/// be rotated with K. Reference: the gap (no opening) replaced by a truss
/// along X with EA/d equal to the gap stiffness.
#[test]
fn contact_2d_gap_at_inclined_roller() {
    let k = 20_000.0;
    let d = 0.5;
    let (mut model, _) = braced_frame(1, "pinned", 100.0);
    for s in model.supports.values_mut() {
        if s.node_id == 2 {
            s.support_type = "inclinedRoller".into();
            s.angle = Some(0.35);
        }
    }
    model.nodes.insert("5".into(), SolverNode { id: 5, x: 4.0 + d, z: 0.0 });
    model.supports.insert("3".into(), SolverSupport {
        id: 3, node_id: 5, support_type: "fixed".into(),
        kx: None, ky: None, kz: None, dx: None, dz: None, dry: None, angle: None,
    });
    let mut ci = contact_input(model.clone());
    ci.gap_elements.push(GapElement {
        id: 1, node_i: 2, node_j: 5, direction: 0, initial_gap: 0.0, stiffness: k,
        friction: None, friction_direction: None, friction_coefficient: None,
    });
    let r = solve_contact_2d(&ci).unwrap();
    assert!(r.converged);
    assert_eq!(r.gap_status[0].status, "closed");

    let mut reference = model.clone();
    reference.sections.insert("3".into(), SolverSection { id: 3, a: k * d / 200_000_000.0, iz: 1e-8, as_y: None });
    reference.elements.insert("99".into(), SolverElement {
        id: 99, elem_type: "truss".into(), node_i: 2, node_j: 5, material_id: 1, section_id: 3,
        hinge_start: false, hinge_end: false,
    });
    let want = linear::solve_2d(&reference).unwrap();
    same_results_2d(&r.results, &want, 1e-9);
}

/// Coulomb sliding of a closed gap: node 2 (top of a 0.5 m cantilever
/// post) is pushed up into a gap to a fixed node and sideways past the
/// friction limit. Sliding, the gap passes μ·N sideways and the post takes
/// the rest: u_x = (F_x − μ·N)/(3EI/L³), with N = k·(u_z − g0) and
/// u_z = (F_z + k·g0)/(EA/L + k). The friction was a stick spring of
/// stiffness μ·k at any load, and its reported force was clamped to μ·N
/// after the fact, so the displacements were those of a stuck contact.
#[test]
fn contact_2d_gap_friction_slides_at_coulomb_limit() {
    let (g0, k, mu, fx, fz) = (0.002, 5000.0, 0.3, 200.0, 500.0);
    let model = make_input(
        vec![(1, 0.0, 0.0), (2, 0.0, 0.5), (3, 0.0, 0.5 + g0)],
        vec![(1, 200.0, 0.3)],
        vec![(1, 0.01, 1e-4)],
        vec![(1, "frame", 1, 2, 1, 1, false, false)],
        vec![(1, 1, "fixed"), (2, 3, "fixed")],
        vec![SolverLoad::Nodal(SolverNodalLoad { node_id: 2, fx, fz, my: 0.0 })],
    );
    let mut ci = contact_input(model);
    ci.gap_elements.push(GapElement {
        id: 1, node_i: 2, node_j: 3, direction: 1, initial_gap: g0, stiffness: k,
        friction: Some(mu), friction_direction: Some(0), friction_coefficient: None,
    });
    let r = solve_contact_2d(&ci).unwrap();
    assert!(r.converged);
    let (ea_l, k_lat) = (200.0 * 1000.0 * 0.01 / 0.5, 3.0 * 200.0 * 1000.0 * 1e-4 / 0.125);
    let uz = (fz + k * g0) / (ea_l + k);
    let n = k * (uz - g0);
    let ux = (fx - mu * n) / k_lat;
    let d2 = r.results.displacements.iter().find(|d| d.node_id == 2).unwrap();
    close(d2.uz, uz, 1e-9, "uz");
    close(d2.ux, ux, 1e-9, "ux (sliding)");
    close(r.gap_status[0].friction_force.abs(), mu * n, 1e-9, "friction force");
    let r3 = r.results.reactions.iter().find(|r| r.node_id == 3).unwrap();
    // The gap pulls node 3 along with the slip; its support holds it back.
    close(r3.rx, -mu * n, 1e-9, "friction reaction at the fixed side");
    let rx: f64 = r.results.reactions.iter().map(|r| r.rx).sum();
    close(rx, -fx, 1e-9, "horizontal equilibrium");
}
