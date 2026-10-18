//! Regressions for the places the 2D axial span load (`SolverLoad::AxialDistributed`) met
//! code added by the safety wave after #274 was written. Both compiled and answered wrongly:
//!
//! - the event-to-event plastic solve cuts a member at an interior hinge; an axial span load
//!   on it stayed on the left piece with its old extent, so part of the load vanished;
//! - the 2D diagram certificate took N as constant between axial point loads and certified a
//!   zero sampling gap for it, which an axial span load makes false.
mod common;
use common::make_input;
use dedaliano_engine::certify::stations::diagram_gaps_2d;
use dedaliano_engine::postprocess::diagrams::{compute_diagram_value_at, diagram_positions_2d};
use dedaliano_engine::solver::{linear::solve_2d, plastic::solve_plastic_2d};
use dedaliano_engine::types::*;
use std::collections::HashMap;

fn axial(element_id: usize, q_i: f64, q_j: f64, a: Option<f64>, b: Option<f64>) -> SolverLoad {
    SolverLoad::AxialDistributed(SolverAxialDistributedLoad { element_id, q_i, q_j, a, b })
}

/// Fixed-base portal 4 m × 6 m: a lateral load at the windward knee, a UDL across the beam
/// and an axial UDL along it (+x). The beam takes an interior hinge before collapse.
fn portal_with_axial_beam_load() -> PlasticInput {
    let (h, w, qx, hx) = (4.0, 6.0, 5.0, 20.0);
    let solver = make_input(
        vec![(1, 0.0, 0.0), (2, 0.0, h), (3, w, h), (4, w, 0.0)],
        vec![(1, 200_000.0, 0.3)],
        vec![(1, 0.045, 3.375e-4)],
        vec![
            (1, "frame", 1, 2, 1, 1, false, false),
            (2, "frame", 2, 3, 1, 1, false, false),
            (3, "frame", 3, 4, 1, 1, false, false),
        ],
        vec![(1, 1, "fixed"), (2, 4, "fixed")],
        vec![
            SolverLoad::Nodal(SolverNodalLoad { node_id: 2, fx: hx, fz: 0.0, my: 0.0 }),
            SolverLoad::Distributed(SolverDistributedLoad { element_id: 2, q_i: -60.0, q_j: -60.0, a: None, b: None }),
            axial(2, qx, qx, None, None),
        ],
    );
    let mut sections = HashMap::new();
    sections.insert("1".to_string(), PlasticSectionData { a: 0.045, iz: 3.375e-4, material_id: 1, b: Some(0.15), h: Some(0.3) });
    let mut materials = HashMap::new();
    materials.insert("1".to_string(), PlasticMaterialData { fy: Some(250.0) });
    PlasticInput { solver, sections, materials, max_hinges: Some(10), mp_overrides: None }
}

#[test]
fn plastic_split_at_an_interior_hinge_keeps_the_whole_axial_span_load() {
    let input = portal_with_axial_beam_load();
    let r = solve_plastic_2d(&input).unwrap();
    // The beam (element 2) takes a hinge inside its span, and a step is solved after it.
    let interior = r
        .hinges
        .iter()
        .find(|hg| hg.element_id == 2 && hg.position > 1e-6 && hg.position < 1.0 - 1e-6)
        .expect("an interior hinge in the beam");
    assert!(interior.step + 1 < r.steps.len(), "a step after the interior hinge: {:?}", r.hinges);

    // Horizontal equilibrium of every step increment: Σ Rx = −Δλ·(H + q_x·L).
    let total_h = 20.0 + 5.0 * 6.0;
    let mut lambda_prev = 0.0;
    for (k, s) in r.steps.iter().enumerate() {
        let dl = s.load_factor - lambda_prev;
        lambda_prev = s.load_factor;
        let rx: f64 = s.results.reactions.iter().map(|x| x.rx).sum();
        assert!((rx + dl * total_h).abs() < 1e-6 * (1.0 + dl * total_h), "step {k}: ΣRx = {rx}, expected {}", -dl * total_h);
    }
}

/// A bar fixed at both ends under an axial load that changes sign along a part of its span:
/// N is quadratic there with an extremum between the diagram's stations.
fn bar_with_partial_trapezoid() -> SolverInput {
    make_input(
        vec![(1, 0.0, 0.0), (2, 4.0, 0.0)],
        vec![(1, 200_000.0, 0.3)],
        vec![(1, 0.01, 1e-4)],
        vec![(1, "frame", 1, 2, 1, 1, false, false)],
        vec![(1, 1, "fixed"), (2, 2, "fixed")],
        vec![axial(1, 30.0, -50.0, Some(0.37), Some(2.91))],
    )
}

#[test]
fn diagram_certificate_bounds_n_between_stations_under_an_axial_span_load() {
    let r = solve_2d(&bar_with_partial_trapezoid()).unwrap();
    let ef = &r.element_forces[0];
    let gap = diagram_gaps_2d(ef).axial;
    assert!(gap > 0.0, "N is not piecewise constant here; the certified gap cannot be 0");

    // The certified claim: between consecutive stations, N stays within the station values ± gap.
    let ts = diagram_positions_2d(ef, "axial");
    let mut worst: f64 = 0.0;
    for w in ts.windows(2) {
        let (n0, n1) = (compute_diagram_value_at("axial", w[0], ef), compute_diagram_value_at("axial", w[1], ef));
        let (lo, hi) = (n0.min(n1), n0.max(n1));
        for i in 1..200 {
            let t = w[0] + (w[1] - w[0]) * i as f64 / 200.0;
            let n = compute_diagram_value_at("axial", t, ef);
            worst = worst.max(n - hi).max(lo - n);
        }
    }
    assert!(worst > 1e-9, "the case must exercise the gap (worst excursion {worst})");
    assert!(worst <= gap + 1e-9, "N leaves the station envelope by {worst} > certified gap {gap}");
}
