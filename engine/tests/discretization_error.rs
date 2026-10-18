//! Discretization-error estimation (docs/verification/DISCRETIZATION.md): the
//! Zienkiewicz–Zhu/SPR estimator's effectivity index and the Richardson-extrapolated
//! convergence check, on benchmarks with known solutions.
//!
//! The effectivity index is θ = η / ‖u − u_h‖_E, with the true energy-norm error from the
//! energy identity ‖u − u_h‖²_E = ‖u‖²_E − ‖u_h‖²_E (Galerkin orthogonality), ‖u_h‖²_E = u_hᵀ K u_h
//! summed element by element, and ‖u‖²_E either closed form (Navier series) or the
//! Richardson-extrapolated limit of ‖u_h‖²_E over a refinement sequence.

use dedaliano_engine::postprocess::discretization::convergence::{convergence_study_3d, richardson, ConvergenceOptions};
use dedaliano_engine::postprocess::discretization::refine::{refine_uniform, GeometryMode};
use dedaliano_engine::postprocess::discretization::{estimate_3d, DiscretizationEstimate};
use dedaliano_engine::solver::linear;
use dedaliano_engine::types::*;
use std::collections::HashMap;
use std::f64::consts::PI;

// ==================== Model building ====================

#[derive(Clone, Copy, PartialEq, Debug)]
enum Fam {
    Dkt,
    Mitc4,
    Mitc9,
    Curved,
    Solid,
}

#[derive(Clone, Copy)]
enum Load {
    /// Pressure on every element (force per area, along the element normal).
    Pressure(f64),
    /// Gravity: force per unit area of mid-surface along global (gx, gy, gz).
    Weight([f64; 3]),
    /// Point forces at corner-grid nodes (i, j).
    None,
}

struct Model {
    input: SolverInput3D,
    /// Mid-surface node ids on the corner grid (i in 0..=nx, j in 0..=ny); for solid-shells the
    /// (bottom, top) pair.
    grid: Vec<Vec<(usize, usize)>>,
}

fn support(node_id: usize, f: [bool; 6]) -> SolverSupport3D {
    SolverSupport3D {
        node_id, rx: f[0], ry: f[1], rz: f[2], rrx: f[3], rry: f[4], rrz: f[5],
        kx: None, ky: None, kz: None, krx: None, kry: None, krz: None,
        dx: None, dy: None, dz: None, drx: None, dry: None, drz: None,
        normal_x: None, normal_y: None, normal_z: None, is_inclined: None, rw: None, kw: None,
    }
}

fn empty_input() -> SolverInput3D {
    SolverInput3D {
        solver_options: None, nodes: HashMap::new(), materials: HashMap::new(), sections: HashMap::new(),
        elements: HashMap::new(), supports: HashMap::new(), loads: vec![], constraints: vec![], left_hand: None,
        plates: HashMap::new(), quads: HashMap::new(), quad9s: HashMap::new(), solid_shells: HashMap::new(),
        curved_shells: HashMap::new(), curved_beams: vec![], connectors: HashMap::new(),
    }
}

/// A structured nx × ny surface mesh of `fam` on the map (u, v) ∈ [0,1]² → (point, unit normal).
/// `bc(i, j)` gives the restraints of the corner-grid node (i, j) (and of the in-between nodes of
/// MITC9 meshes, by the nearest even index along a restrained edge).
fn surface(
    fam: Fam, nx: usize, ny: usize, t: f64, e_mpa: f64, nu: f64,
    map: &dyn Fn(f64, f64) -> ([f64; 3], [f64; 3]),
    bc: &dyn Fn(usize, usize, usize, usize) -> Option<[bool; 6]>,
    load: Load,
) -> Model {
    let mut input = empty_input();
    input.materials.insert("1".into(), SolverMaterial { id: 1, e: e_mpa, nu });
    // Node lattice: MITC9 needs (2nx+1)×(2ny+1).
    let k = if fam == Fam::Mitc9 { 2 } else { 1 };
    let (mx, my) = (k * nx, k * ny);
    let mut lat = vec![vec![(0usize, 0usize); my + 1]; mx + 1];
    let mut nid = 1;
    for i in 0..=mx {
        for j in 0..=my {
            let (p, n) = map(i as f64 / mx as f64, j as f64 / my as f64);
            if fam == Fam::Solid {
                let b = [p[0] - 0.5 * t * n[0], p[1] - 0.5 * t * n[1], p[2] - 0.5 * t * n[2]];
                let tp = [p[0] + 0.5 * t * n[0], p[1] + 0.5 * t * n[1], p[2] + 0.5 * t * n[2]];
                input.nodes.insert(nid.to_string(), SolverNode3D { id: nid, x: b[0], y: b[1], z: b[2] });
                input.nodes.insert((nid + 1).to_string(), SolverNode3D { id: nid + 1, x: tp[0], y: tp[1], z: tp[2] });
                lat[i][j] = (nid, nid + 1);
                nid += 2;
            } else {
                input.nodes.insert(nid.to_string(), SolverNode3D { id: nid, x: p[0], y: p[1], z: p[2] });
                lat[i][j] = (nid, nid);
                nid += 1;
            }
        }
    }
    let mut eid = 1;
    let mut eids = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            match fam {
                Fam::Dkt => {
                    let (a, b, c, d) = (lat[i][j].0, lat[i + 1][j].0, lat[i + 1][j + 1].0, lat[i][j + 1].0);
                    for nodes in [[a, b, c], [a, c, d]] {
                        input.plates.insert(eid.to_string(), SolverPlateElement { id: eid, nodes, material_id: 1, thickness: t });
                        eids.push(eid);
                        eid += 1;
                    }
                }
                Fam::Mitc4 | Fam::Curved => {
                    let nodes = [lat[i][j].0, lat[i + 1][j].0, lat[i + 1][j + 1].0, lat[i][j + 1].0];
                    if fam == Fam::Mitc4 {
                        input.quads.insert(eid.to_string(), SolverQuadElement { id: eid, nodes, material_id: 1, thickness: t });
                    } else {
                        let normals = Some([(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)]
                            .map(|(a, b)| map(a as f64 / nx as f64, b as f64 / ny as f64).1));
                        input.curved_shells.insert(eid.to_string(), SolverCurvedShellElement { id: eid, nodes, material_id: 1, thickness: t, normals });
                    }
                    eids.push(eid);
                    eid += 1;
                }
                Fam::Mitc9 => {
                    let (a, b) = (2 * i, 2 * j);
                    let at = |di: usize, dj: usize| lat[a + di][b + dj].0;
                    let nodes = [at(0, 0), at(2, 0), at(2, 2), at(0, 2), at(1, 0), at(2, 1), at(1, 2), at(0, 1), at(1, 1)];
                    input.quad9s.insert(eid.to_string(), SolverQuad9Element { id: eid, nodes, material_id: 1, thickness: t });
                    eids.push(eid);
                    eid += 1;
                }
                Fam::Solid => {
                    let c = [lat[i][j], lat[i + 1][j], lat[i + 1][j + 1], lat[i][j + 1]];
                    let nodes = [c[0].0, c[1].0, c[2].0, c[3].0, c[0].1, c[1].1, c[2].1, c[3].1];
                    input.solid_shells.insert(eid.to_string(), SolverSolidShellElement { id: eid, nodes, material_id: 1 });
                    eids.push(eid);
                    eid += 1;
                }
            }
        }
    }
    let mut sid = 1;
    for i in 0..=mx {
        for j in 0..=my {
            if let Some(f) = bc(i, j, mx, my) {
                let (b, tp) = lat[i][j];
                for n in if b == tp { vec![b] } else { vec![b, tp] } {
                    input.supports.insert(sid.to_string(), support(n, f));
                    sid += 1;
                }
            }
        }
    }
    for &id in &eids {
        let l = match load {
            Load::Pressure(q) => {
                let p = SolverPressureLoad { element_id: id, pressure: q };
                match fam {
                    Fam::Dkt => SolverLoad3D::Pressure(p),
                    Fam::Mitc4 => SolverLoad3D::QuadPressure(p),
                    Fam::Mitc9 => SolverLoad3D::Quad9Pressure(p),
                    Fam::Curved => SolverLoad3D::CurvedShellPressure(p),
                    Fam::Solid => SolverLoad3D::SolidShellPressure(p),
                }
            }
            Load::Weight(g) => {
                // Force per area w = ρ/1000 · t · g  ⇒  ρ = 1000 / t per unit of g.
                let rho = 1000.0 / t;
                let sw = SolverQuadSelfWeightLoad { element_id: id, density: rho, gx: g[0], gy: g[1], gz: g[2] };
                match fam {
                    Fam::Mitc4 => SolverLoad3D::QuadSelfWeight(sw),
                    Fam::Mitc9 => SolverLoad3D::Quad9SelfWeight(sw),
                    Fam::Curved => SolverLoad3D::CurvedShellSelfWeight(sw),
                    _ => panic!("self-weight not built for {fam:?}"),
                }
            }
            Load::None => continue,
        };
        input.loads.push(l);
    }
    let grid = (0..=nx).map(|i| (0..=ny).map(|j| lat[k * i][k * j]).collect()).collect();
    Model { input, grid }
}

fn nodal(node_id: usize, f: [f64; 3]) -> SolverLoad3D {
    SolverLoad3D::Nodal(SolverNodalLoad3D { node_id, fx: f[0], fy: f[1], fz: f[2], mx: 0.0, my: 0.0, mz: 0.0, bw: None })
}

// ==================== Energy ====================

/// ‖u_h‖²_E = Σ_e u_eᵀ K_e u_e over the shell-type elements, with each family's own stiffness.
fn energy_norm_sq(input: &SolverInput3D, r: &AnalysisResults3D) -> f64 {
    use dedaliano_engine::element::{self, curved_shell, quad, quad9, solid_shell};
    let c: HashMap<usize, [f64; 3]> = input.nodes.values().map(|n| (n.id, [n.x, n.y, n.z])).collect();
    let u: HashMap<usize, [f64; 6]> = r.displacements.iter().map(|d| (d.node_id, [d.ux, d.uy, d.uz, d.rx, d.ry, d.rz])).collect();
    let mat = &input.materials["1"];
    let (e, nu) = (mat.e * 1000.0, mat.nu);
    let quadform = |k: &[f64], v: &[f64]| -> f64 {
        let n = v.len();
        let mut s = 0.0;
        for i in 0..n {
            for j in 0..n {
                s += v[i] * k[i * n + j] * v[j];
            }
        }
        s
    };
    let ug = |ids: &[usize]| -> Vec<f64> { ids.iter().flat_map(|id| u[id]).collect() };
    let mut w = 0.0;
    for p in input.plates.values() {
        let x = [c[&p.nodes[0]], c[&p.nodes[1]], c[&p.nodes[2]]];
        let t = element::plate_transform_3d(&x);
        let ul = dedaliano_engine::linalg::transform_displacement(&ug(&p.nodes), &t, 18);
        w += quadform(&element::plate_local_stiffness(&x, e, nu, p.thickness), &ul);
    }
    for q in input.quads.values() {
        let x = [c[&q.nodes[0]], c[&q.nodes[1]], c[&q.nodes[2]], c[&q.nodes[3]]];
        let t = quad::quad_transform_3d(&x);
        let ul = dedaliano_engine::linalg::transform_displacement(&ug(&q.nodes), &t, 24);
        w += quadform(&quad::mitc4_local_stiffness(&x, e, nu, q.thickness), &ul);
    }
    for q in input.quad9s.values() {
        let mut x = [[0.0; 3]; 9];
        for (i, n) in q.nodes.iter().enumerate() { x[i] = c[n]; }
        let t = quad9::quad9_transform_3d(&x);
        let ul = dedaliano_engine::linalg::transform_displacement(&ug(&q.nodes), &t, 54);
        w += quadform(&quad9::mitc9_local_stiffness(&x, e, nu, q.thickness), &ul);
    }
    for q in input.curved_shells.values() {
        let x = [c[&q.nodes[0]], c[&q.nodes[1]], c[&q.nodes[2]], c[&q.nodes[3]]];
        let dirs = q.normals.unwrap_or_else(|| curved_shell::compute_element_directors(&x));
        w += quadform(&curved_shell::curved_shell_stiffness(&x, &dirs, e, nu, q.thickness), &ug(&q.nodes));
    }
    for s in input.solid_shells.values() {
        let mut x = [[0.0; 3]; 8];
        for (i, n) in s.nodes.iter().enumerate() { x[i] = c[n]; }
        let v: Vec<f64> = s.nodes.iter().flat_map(|id| u[id][..3].to_vec()).collect();
        w += quadform(&solid_shell::solid_shell_stiffness(&x, e, nu), &v);
    }
    w
}

fn solve(m: &SolverInput3D) -> AnalysisResults3D {
    linear::solve_3d(m).expect("solve")
}

fn disp(r: &AnalysisResults3D, id: usize) -> [f64; 3] {
    let d = r.displacements.iter().find(|d| d.node_id == id).expect("node");
    [d.ux, d.uy, d.uz]
}

/// Mid-surface displacement at a grid point (average of the faces for solid-shells).
fn mid_disp(r: &AnalysisResults3D, ids: (usize, usize)) -> [f64; 3] {
    let (a, b) = (disp(r, ids.0), disp(r, ids.1));
    [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1]), 0.5 * (a[2] + b[2])]
}

fn estimate(m: &SolverInput3D, r: &AnalysisResults3D) -> DiscretizationEstimate {
    estimate_3d(m, r).expect("estimate")
}

// ==================== Benchmarks ====================

/// Square plate in the xy-plane, side a, simply supported (soft: w = 0 on the edges) or
/// clamped, uniform pressure q.
fn square_plate(fam: Fam, n: usize, clamped: bool) -> Model {
    let a = 1.0;
    let t = 0.01;
    let map = move |u: f64, v: f64| ([u * a, v * a, 0.0], [0.0, 0.0, 1.0]);
    let bc = move |i: usize, j: usize, mx: usize, my: usize| -> Option<[bool; 6]> {
        let edge = i == 0 || j == 0 || i == mx || j == my;
        if !edge {
            return None;
        }
        if clamped {
            return Some([true; 6]);
        }
        // Soft simple support plus in-plane pins at two corners.
        let pin = (i == 0 && j == 0, i == mx && j == 0);
        Some([pin.0, pin.0 || pin.1, true, false, false, false])
    };
    surface(fam, n, n, t, 200_000.0, 0.3, &map, &bc, Load::Pressure(-1.0))
}

fn plate_rigidity() -> f64 {
    let (e, t, nu) = (200_000.0 * 1000.0, 0.01_f64, 0.3);
    e * t.powi(3) / (12.0 * (1.0 - nu * nu))
}

/// Navier series for the simply supported square plate (side 1, q = 1): ‖u‖²_E = ∫ q w dA and
/// the centre deflection.
fn navier(d: f64) -> (f64, f64) {
    let (mut w_energy, mut wc) = (0.0, 0.0);
    for m in (1..400).step_by(2) {
        for n in (1..400).step_by(2) {
            let (mf, nf) = (m as f64, n as f64);
            let den = mf * nf * (mf * mf + nf * nf).powi(2);
            wc += 16.0 / (PI.powi(6) * d) / den * ((m / 2 + n / 2) % 2 == 0).then_some(1.0).unwrap_or(-1.0);
            w_energy += 64.0 / (PI.powi(8) * d) / (mf * mf * nf * nf * (mf * mf + nf * nf).powi(2));
        }
    }
    (w_energy, wc)
}

/// Richardson limit of a sequence on meshes h, h/2, h/4.
fn extrapolate(v: &[f64]) -> (f64, Option<f64>) {
    let (p, ext, _, _) = richardson(v);
    (ext.unwrap_or(*v.last().unwrap()), p)
}

#[test]
fn zz_effectivity_navier_plate() {
    let d = plate_rigidity();
    let (w_exact, wc_exact) = navier(d);
    println!("Navier plate: ‖u‖²_E = {w_exact:.6e}, w_c = {wc_exact:.6e}");
    for fam in [Fam::Mitc4, Fam::Dkt, Fam::Mitc9, Fam::Curved, Fam::Solid] {
        for n in [4, 8, 16] {
            let m = square_plate(fam, n, false);
            let r = solve(&m.input);
            let w_h = energy_norm_sq(&m.input, &r);
            let est = estimate(&m.input, &r);
            let true_err = (w_exact - w_h).max(0.0).sqrt();
            let wc = mid_disp(&r, m.grid[n / 2][n / 2])[2].abs();
            println!(
                "{fam:?} n={n:2}: w_c/ref={:.4} ‖e‖/‖u‖={:.4} η/‖u‖={:.4} θ={:.3}",
                wc / wc_exact, true_err / w_exact.sqrt(), est.error_norm / w_exact.sqrt(), est.error_norm / true_err
            );
        }
    }
}

/// Scordelis–Lo roof (quarter): R = 25, L = 50, 40°, t = 0.25, E = 4.32e8, ν = 0, weight 90
/// per unit area. Reference midspan free-edge vertical displacement 0.3024 (MacNeal–Harder).
fn scordelis(fam: Fam, n: usize) -> Model {
    let (r, half_l, th) = (25.0, 25.0, 40.0_f64.to_radians());
    let map = move |u: f64, v: f64| {
        let a = v * th;
        ([u * half_l, r * a.sin(), r * a.cos() - r], [0.0, a.sin(), a.cos()])
    };
    // i: along x (0 = midspan symmetry, max = diaphragm); j: around (0 = crown, max = free edge).
    let bc = |i: usize, j: usize, mx: usize, _my: usize| -> Option<[bool; 6]> {
        let mut f = [false; 6];
        if i == 0 { f[0] = true; f[4] = true; f[5] = true; }
        if i == mx { f[1] = true; f[2] = true; }
        if j == 0 { f[1] = true; f[3] = true; f[5] = true; }
        f.iter().any(|&b| b).then_some(f)
    };
    surface(fam, n, n, 0.25, 4.32e8 / 1000.0, 0.0, &map, &bc, Load::Weight([0.0, 0.0, -90.0]))
}

#[test]
fn convergence_scordelis_lo() {
    for fam in [Fam::Mitc4, Fam::Mitc9, Fam::Curved] {
        let base = scordelis(fam, 4);
        let target = base.grid[0][4].0;
        let mut models = vec![base.input.clone()];
        for _ in 0..3 {
            models.push(refine_uniform(models.last().unwrap(), GeometryMode::Smooth).unwrap().input);
        }
        let mut uz = Vec::new();
        let mut w = Vec::new();
        let mut eta = Vec::new();
        for m in &models {
            let r = solve(m);
            uz.push(disp(&r, target)[2].abs());
            w.push(energy_norm_sq(m, &r));
            eta.push(estimate(m, &r).error_norm);
        }
        let (uz_ext, p) = extrapolate(&uz[1..]);
        let (w_ext, pw) = extrapolate(&w[1..]);
        println!("{fam:?} Scordelis–Lo uz: {uz:?} → {uz_ext:.5} (p={p:?}), ref 0.3024");
        for k in 0..models.len() {
            let te = (w_ext - w[k]).max(0.0).sqrt();
            println!("   level {k}: η/‖u‖={:.4} ‖e‖/‖u‖={:.4} θ={:.3} (energy p={pw:?})", eta[k] / w_ext.sqrt(), te / w_ext.sqrt(), eta[k] / te);
        }
    }
}

#[test]
fn smoke_convergence_study() {
    let m = square_plate(Fam::Mitc4, 4, false);
    let s = convergence_study_3d(&m.input, &ConvergenceOptions::default()).unwrap();
    println!("{}", serde_json::to_string_pretty(&s).unwrap());
    assert!(!s.certified);
    let _ = nodal;
}

/// Refine `m` `levels` times (smooth geometry) and solve each; returns the inputs and results.
fn sequence(m: &SolverInput3D, levels: usize) -> Vec<(SolverInput3D, AnalysisResults3D)> {
    let mut inputs = vec![m.clone()];
    for _ in 0..levels {
        inputs.push(refine_uniform(inputs.last().unwrap(), GeometryMode::Smooth).unwrap().input);
    }
    inputs.into_iter().map(|i| { let r = solve(&i); (i, r) }).collect()
}

/// Pinched cylinder with rigid diaphragms (MacNeal–Harder), one eighth: R = 300, L = 600, t = 3,
/// E = 3e6, ν = 0.3, P = 1. Reference radial displacement under the load 1.8248e-5.
fn pinched_cylinder(fam: Fam, n: usize) -> Model {
    let (r, half_l) = (300.0, 300.0);
    let map = move |u: f64, v: f64| {
        let th = v * PI / 2.0;
        ([u * half_l, r * th.cos(), r * th.sin()], [0.0, th.cos(), th.sin()])
    };
    let bc = |i: usize, j: usize, mx: usize, my: usize| -> Option<[bool; 6]> {
        let mut f = [false; 6];
        if i == 0 { f[0] = true; f[4] = true; f[5] = true; }
        if i == mx { f[1] = true; f[2] = true; }
        if j == 0 { f[2] = true; f[3] = true; f[4] = true; }
        if j == my { f[1] = true; f[3] = true; f[5] = true; }
        f.iter().any(|&b| b).then_some(f)
    };
    let mut m = surface(fam, n, n, 3.0, 3000.0, 0.3, &map, &bc, Load::None);
    let (a, b) = m.grid[0][0];
    for id in if a == b { vec![a] } else { vec![a, b] } {
        m.input.loads.push(nodal(id, [0.0, -0.25 / if a == b { 1.0 } else { 2.0 }, 0.0]));
    }
    m
}

/// Hemispherical shell with an 18° hole (MacNeal–Harder), one quarter: R = 10, t = 0.04,
/// E = 6.825e7, ν = 0.3, ±2 radial point loads at the equator. Reference 0.093 (0.094 in
/// MacNeal–Harder; 0.0924 from converged shell analyses).
fn hemisphere(fam: Fam, n: usize) -> Model {
    let r = 10.0;
    let map = move |u: f64, v: f64| {
        let (phi, th) = (u * 72.0_f64.to_radians(), v * PI / 2.0);
        let nrm = [phi.cos() * th.cos(), phi.cos() * th.sin(), phi.sin()];
        ([r * nrm[0], r * nrm[1], r * nrm[2]], nrm)
    };
    let bc = |i: usize, j: usize, mx: usize, my: usize| -> Option<[bool; 6]> {
        let mut f = [false; 6];
        if j == 0 { f[1] = true; f[3] = true; f[5] = true; }
        if j == my { f[0] = true; f[4] = true; f[5] = true; }
        if i == mx && j == 0 { f[2] = true; }
        f.iter().any(|&b| b).then_some(f)
    };
    let mut m = surface(fam, n, n, 0.04, 68250.0, 0.3, &map, &bc, Load::None);
    m.input.loads.push(nodal(m.grid[0][0].0, [2.0, 0.0, 0.0]));
    m.input.loads.push(nodal(m.grid[0][n].0, [0.0, -2.0, 0.0]));
    m
}

/// Morley's 30° skew plate, simply supported (w = 0), uniform load. Kirchhoff reference centre
/// deflection w_c = 4.08e-4 q L⁴ / D (Morley 1963).
fn morley(fam: Fam, n: usize) -> (Model, f64) {
    let (l, t, e_mpa, nu) = (100.0, 0.1, 200_000.0, 0.3);
    let (c, s) = (30.0_f64.to_radians().cos(), 30.0_f64.to_radians().sin());
    let map = move |u: f64, v: f64| ([u * l + v * l * c, v * l * s, 0.0], [0.0, 0.0, 1.0]);
    let bc = |i: usize, j: usize, mx: usize, my: usize| -> Option<[bool; 6]> {
        let edge = i == 0 || j == 0 || i == mx || j == my;
        edge.then(|| {
            let pin = (i == 0 && j == 0, i == mx && j == 0);
            [pin.0, pin.0 || pin.1, true, false, false, false]
        })
    };
    let d = e_mpa * 1000.0 * t * t * t / (12.0 * (1.0 - nu * nu));
    let m = surface(fam, n, n, t, e_mpa, nu, &map, &bc, Load::Pressure(-1.0));
    (m, 4.08e-4 * l.powi(4) / d)
}

#[test]
fn convergence_classic_benchmarks() {
    for fam in [Fam::Mitc4, Fam::Curved] {
        let m = pinched_cylinder(fam, 4);
        let id = m.grid[0][0].0;
        let seq = sequence(&m.input, 3);
        let v: Vec<f64> = seq.iter().map(|(_, r)| disp(r, id)[1].abs()).collect();
        let (ext, p) = extrapolate(&v[1..]);
        println!("{fam:?} pinched cylinder: {v:?} → {ext:.5e} (p={p:?}) ref 1.8248e-5");

        let m = hemisphere(fam, 4);
        let id = m.grid[0][0].0;
        let seq = sequence(&m.input, 3);
        let v: Vec<f64> = seq.iter().map(|(_, r)| disp(r, id)[0].abs()).collect();
        let (ext, p) = extrapolate(&v[1..]);
        println!("{fam:?} hemisphere: {v:?} → {ext:.5} (p={p:?}) ref 0.093");
    }
    for fam in [Fam::Mitc4, Fam::Dkt, Fam::Mitc9] {
        let (m, wref) = morley(fam, 4);
        let id = m.grid[2][2].0;
        let seq = sequence(&m.input, 3);
        let v: Vec<f64> = seq.iter().map(|(_, r)| disp(r, id)[2].abs()).collect();
        let (ext, p) = extrapolate(&v[1..]);
        println!("{fam:?} Morley: {:?} → {:.4} (p={p:?}) ref 1", v.iter().map(|x| x / wref).collect::<Vec<_>>(), ext / wref);
        for clamped in [false, true] {
            let m = square_plate(fam, 4, clamped);
            let id = m.grid[2][2].0;
            let seq = sequence(&m.input, 3);
            let wref = if clamped { 0.00126 } else { 0.00406 } / plate_rigidity();
            let v: Vec<f64> = seq.iter().map(|(_, r)| disp(r, id)[2].abs() / wref).collect();
            let (ext, p) = extrapolate(&v[1..]);
            println!("{fam:?} square plate clamped={clamped}: {v:?} → {ext:.4} (p={p:?})");
        }
    }
}
