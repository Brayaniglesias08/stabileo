//! Checks written while documenting the frame family (docs/solver/frames.md).
//! Each test pins a convention or a formula that the document states.

mod common;
use common::*;
use dedaliano_engine::postprocess::diagrams::compute_diagram_value_at;
use dedaliano_engine::postprocess::diagrams_3d::evaluate_diagram_3d_at;
use dedaliano_engine::solver::linear::{solve_2d, solve_3d};
use dedaliano_engine::types::*;

#[test]
fn probe_moment_signs() {
    let inp = make_ss_beam_udl(2, 6.0, 200_000.0, 0.01, 1e-4, -10.0);
    let r = solve_2d(&inp).unwrap();
    let ef = r.element_forces.iter().find(|e| e.element_id == 1).unwrap();
    let m_mid = compute_diagram_value_at("moment", 1.0, ef);
    let v0 = compute_diagram_value_at("shear", 0.0, ef);
    println!("2D: M(mid)={m_mid} V(0)={v0} ef={:?}", (ef.v_start, ef.m_start, ef.v_end, ef.m_end));

    let pinned = vec![true, true, true, true, false, false];
    let roller = vec![false, true, true, false, false, false];
    let loads = vec![
        SolverLoad3D::Distributed(SolverDistributedLoad3D { q_xi: 0.0, q_xj: 0.0, element_id: 1, q_yi: -10.0, q_yj: -10.0, q_zi: -10.0, q_zj: -10.0, a: None, b: None }),
        SolverLoad3D::Distributed(SolverDistributedLoad3D { q_xi: 0.0, q_xj: 0.0, element_id: 2, q_yi: -10.0, q_yj: -10.0, q_zi: -10.0, q_zj: -10.0, a: None, b: None }),
    ];
    let inp3 = make_3d_beam(2, 6.0, 200_000.0, 0.3, 0.01, 1e-4, 1e-4, 1e-4, pinned, Some(roller), loads);
    let r3 = solve_3d(&inp3).unwrap();
    let ef3 = r3.element_forces.iter().find(|e| e.element_id == 1).unwrap();
    println!("3D: My(mid)={} Mz(mid)={} vy0={} vz0={}", evaluate_diagram_3d_at(ef3, "momentY", 1.0), evaluate_diagram_3d_at(ef3, "momentZ", 1.0), ef3.vy_start, ef3.vz_start);
    let n = r3.displacements.iter().find(|d| d.node_id == 2).unwrap();
    println!("3D mid disp {:?}", n);
}

#[test]
fn probe_bimoment() {
    use std::collections::HashMap;
    let (e, nu, a, iy, iz, j, cw): (f64, f64, f64, f64, f64, f64, f64) = (200_000.0, 0.3, 7.61e-3, 18.4e-6, 128.0e-6, 0.5e-6, 300.0e-9);
    let l = 3.0;
    let torque = 1.0;
    let ee = e * 1000.0;
    let g = ee / (2.0 * (1.0 + nu));
    let k = (g * j / (ee * cw)).sqrt();
    for n in [1usize, 4, 16] {
        let mut inp = make_3d_beam(n, l, e, nu, a, iy, iz, j, vec![true; 6], None, vec![SolverLoad3D::Nodal(SolverNodalLoad3D { node_id: n + 1, fx: 0.0, fy: 0.0, fz: 0.0, mx: torque, my: 0.0, mz: 0.0, bw: None })]);
        for s in inp.sections.values_mut() { s.cw = Some(cw); }
        for s in inp.supports.values_mut() { s.rw = Some(true); }
        let _ = HashMap::<u8, u8>::new();
        let r = solve_3d(&inp).unwrap();
        let ef = r.element_forces.iter().find(|e| e.element_id == 1).unwrap();
        let efl = r.element_forces.iter().find(|e| e.element_id == n).unwrap();
        let reac = r.reactions.iter().find(|x| x.node_id == 1).unwrap();
        println!("n={n} B0={:?} BL={:?} Mx0={} reacB={:?} exact |B0|={}", ef.bimoment_start, efl.bimoment_end, ef.mx_start, reac.bimoment, torque / k * (k * l).tanh());
    }
}

#[test]
fn probe_left_hand_physics() {
    // Same skew cantilever, left-handed vs right-handed local axes.
    let mk = |lh: bool| {
        let mut inp = make_3d_input(
            vec![(1, 0.0, 0.0, 0.0), (2, 3.0, 1.0, 2.0), (3, 5.0, 3.0, 2.5)],
            vec![(1, 200_000.0, 0.3)],
            vec![(1, 0.01, 2e-5, 8e-5, 1e-5)],
            vec![(1, "frame", 1, 2, 1, 1), (2, "frame", 2, 3, 1, 1)],
            vec![(1, vec![true; 6])],
            vec![SolverLoad3D::Nodal(SolverNodalLoad3D { node_id: 3, fx: 1.0, fy: -2.0, fz: -5.0, mx: 0.3, my: 0.7, mz: -0.4, bw: None })],
        );
        inp.left_hand = Some(lh);
        solve_3d(&inp).unwrap()
    };
    let (r, l) = (mk(false), mk(true));
    let dr = r.displacements.iter().find(|d| d.node_id == 3).unwrap();
    let dl = l.displacements.iter().find(|d| d.node_id == 3).unwrap();
    println!("RH {:?}\nLH {:?}", (dr.ux, dr.uy, dr.uz, dr.rx, dr.ry, dr.rz), (dl.ux, dl.uy, dl.uz, dl.rx, dl.ry, dl.rz));
}
