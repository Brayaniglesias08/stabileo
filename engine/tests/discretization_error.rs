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

/// Kirchhoff moments of the simply supported unit square plate under unit load (Navier series),
/// as a global tensor (Mxx, Myy, Mxy), sagging-positive.
fn navier_moments(x: f64, y: f64, nu: f64) -> [f64; 3] {
    let mut m = [0.0; 3];
    for i in (1..120).step_by(2) {
        for j in (1..120).step_by(2) {
            let (a, b) = (i as f64, j as f64);
            // w = Σ A sin(aπx) sin(bπy), A·D = 16/(π⁶ a b (a² + b²)²)
            let ad = 16.0 / (PI.powi(6) * a * b * (a * a + b * b).powi(2));
            let (sx, sy) = ((a * PI * x).sin(), (b * PI * y).sin());
            let (cx, cy) = ((a * PI * x).cos(), (b * PI * y).cos());
            m[0] += ad * PI * PI * (a * a + nu * b * b) * sx * sy;
            m[1] += ad * PI * PI * (b * b + nu * a * a) * sx * sy;
            m[2] -= ad * PI * PI * (1.0 - nu) * a * b * cx * cy;
        }
    }
    m
}

/// True bending-energy error ‖σ − σ_h‖ of a Navier plate solution, against the exact Kirchhoff
/// moments, over the same integration points and components the estimator uses. The load
/// direction and the twisting-moment sign convention are fixed by taking the sign choice that
/// fits the computed field best.
fn navier_true_error(m: &SolverInput3D, r: &AnalysisResults3D, t: f64, nu: f64) -> f64 {
    use dedaliano_engine::postprocess::discretization::samples::{energy_density, sample_elements, to_local};
    let elems = sample_elements(m, r).unwrap();
    let mut best = f64::INFINITY;
    for (s_m, s_t) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        let mut err = 0.0;
        for e in &elems {
            for smp in &e.samples {
                let mm = navier_moments(smp.pos[0], smp.pos[1], nu);
                // σ(z) = −12 M z / t³ (sagging-positive moments: tension at the bottom).
                let k = -12.0 * smp.pos[2] / t.powi(3);
                let g = [s_m * k * mm[0], s_m * k * mm[1], 0.0, s_m * s_t * k * mm[2], 0.0, 0.0];
                let ex = to_local(&smp.frame, &g);
                let d = [ex[0] - smp.sigma[0], ex[1] - smp.sigma[1], ex[2] - smp.sigma[2]];
                err += energy_density(&d, e.e, e.nu) * smp.weight;
            }
        }
        best = best.min(err);
    }
    best.sqrt()
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
    // Each load lies on a symmetry plane: the quarter carries half of it.
    m.input.loads.push(nodal(m.grid[0][0].0, [1.0, 0.0, 0.0]));
    m.input.loads.push(nodal(m.grid[0][n].0, [0.0, -1.0, 0.0]));
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

// ==================== Effectivity ====================

/// Effectivity on the Navier plate against the exact Kirchhoff moments. The Reissner–Mindlin
/// elements (MITC4, MITC9, curved shell) converge to the Mindlin solution, whose soft simple
/// support has a twisting-moment boundary layer of width ~t the Kirchhoff moments lack: once the
/// discretization error falls to that model difference (MITC9 on fine meshes) θ drops, rightly
/// (the estimator measures the mesh, not the theory). The solid-shell's centre deflection is
/// ~18 % low (its 3D constitutive law stiffens bending by 1/(1 − ν²)·(1 − ν)/((1 + ν)(1 − 2ν))
/// in thin plates): a model error no recovery can see, so its θ is reported, not asserted.
#[test]
fn zz_effectivity_navier_plate() {
    let d = plate_rigidity();
    let (w_exact, wc_exact) = navier(d);
    let norm = w_exact.sqrt();
    let mut table = Vec::new();
    for fam in [Fam::Mitc4, Fam::Dkt, Fam::Mitc9, Fam::Curved, Fam::Solid] {
        let mut prev_eta = f64::INFINITY;
        for n in [4, 8, 16] {
            let m = square_plate(fam, n, false);
            let r = solve(&m.input);
            let est = estimate(&m.input, &r);
            assert!(!est.certified);
            let true_err = navier_true_error(&m.input, &r, 0.01, 0.3);
            let theta = est.error_norm / true_err;
            let wc = mid_disp(&r, m.grid[n / 2][n / 2])[2].abs();
            println!("{fam:?} n={n:2}: w_c/ref={:.4} ‖e‖/‖u‖={:.4} η/‖u‖={:.4} θ={theta:.3}", wc / wc_exact, true_err / norm, est.error_norm / norm);
            table.push((fam, n, theta));
            assert!(est.error_norm < prev_eta, "{fam:?}: the estimate falls under refinement");
            prev_eta = est.error_norm;
        }
    }
    for (fam, n, theta) in table {
        let band = match (fam, n) {
            (Fam::Mitc4 | Fam::Curved, 8 | 16) => Some((0.9, 1.15)),
            (Fam::Mitc4 | Fam::Curved, _) => Some((0.9, 1.25)),
            (Fam::Dkt, 16) => Some((0.9, 1.2)),
            (Fam::Dkt, 8) => Some((0.9, 1.35)),
            (Fam::Mitc9, 4) => Some((0.85, 1.15)),
            _ => None,
        };
        if let Some((lo, hi)) = band {
            assert!(theta > lo && theta < hi, "{fam:?} n={n}: effectivity {theta:.3} outside [{lo}, {hi}]");
        }
    }
}

/// A uniform membrane stretch on a distorted mesh is reproduced exactly by every element and by
/// the recovery: the estimate is zero.
#[test]
fn zz_estimate_vanishes_for_uniform_stress() {
    for fam in [Fam::Mitc4, Fam::Dkt, Fam::Mitc9] {
        let map = |u: f64, v: f64| {
            // A smooth, non-affine distortion of the unit square.
            let (x, y) = (u + 0.08 * (PI * u).sin() * (PI * v).sin(), v + 0.05 * (2.0 * PI * u).sin() * v);
            ([x, y, 0.0], [0.0, 0.0, 1.0])
        };
        let bc = |i: usize, j: usize, _mx: usize, _my: usize| -> Option<[bool; 6]> {
            Some([true, i == 0 && j == 0, true, true, true, true])
        };
        let mut m = surface(fam, 4, 4, 0.1, 200_000.0, 0.3, &map, &bc, Load::None);
        // Prescribed stretch ux = 1e-3·x at every node, uy free: a uniform stress field.
        for s in m.input.supports.values_mut() {
            let n = &m.input.nodes[&s.node_id.to_string()];
            s.rx = true;
            s.dx = Some(1e-3 * n.x);
        }
        let r = solve(&m.input);
        let est = estimate(&m.input, &r);
        assert!(est.solution_norm > 0.0);
        assert!(est.relative_error.unwrap() < 1e-6, "{fam:?}: uniform stress, estimate {:?}", est.relative_error);
    }
}

// ==================== Convergence: classic benchmarks ====================

fn richardson_of(m: &SolverInput3D, levels: usize, read: &dyn Fn(&AnalysisResults3D) -> f64) -> (Vec<f64>, f64, Option<f64>) {
    let v: Vec<f64> = sequence(m, levels).iter().map(|(_, r)| read(r)).collect();
    let (ext, p) = extrapolate(&v[v.len() - 3..]);
    (v, ext, p)
}

/// Scordelis–Lo from a 4 × 4 mesh refined three times (smooth geometry): the extrapolated
/// midspan free-edge deflection against 0.3024, and the effectivity of the curved shell, whose
/// refinement keeps the energy identity (MITC4 facets change the geometry from mesh to mesh, so
/// ‖u‖²_E − ‖u_h‖²_E is not the error there).
#[test]
fn convergence_scordelis_lo() {
    for (fam, tol) in [(Fam::Mitc4, 0.01), (Fam::Curved, 0.01), (Fam::Mitc9, 0.015)] {
        let base = scordelis(fam, 4);
        let target = base.grid[0][4].0;
        let seq = sequence(&base.input, 3);
        let uz: Vec<f64> = seq.iter().map(|(_, r)| disp(r, target)[2].abs()).collect();
        let (p, ext, _, status) = richardson(&uz[1..]);
        let ext = ext.unwrap_or(uz[3]);
        println!("{fam:?} Scordelis–Lo uz: {uz:?} → {ext:.5} (p={p:?}, {status}), ref 0.3024");
        assert!((ext / 0.3024 - 1.0).abs() < tol, "{fam:?}: extrapolated {ext:.5} vs 0.3024");
        if fam == Fam::Curved {
            let w: Vec<f64> = seq.iter().map(|(m, r)| energy_norm_sq(m, r)).collect();
            let (w_ext, _) = extrapolate(&w[1..]);
            for (k, (m, r)) in seq.iter().enumerate() {
                let theta = estimate(m, r).error_norm / (w_ext - w[k]).sqrt();
                println!("   level {k}: θ = {theta:.3}");
                assert!(theta > 0.8 && theta < 1.15, "curved shell Scordelis–Lo level {k}: θ = {theta:.3}");
            }
        }
        if fam == Fam::Mitc9 {
            // On the model as drawn the energy identity holds against the refined sequence.
            let w: Vec<f64> = seq.iter().map(|(m, r)| energy_norm_sq(m, r)).collect();
            let theta = estimate(&seq[0].0, &seq[0].1).error_norm / (w[3] - w[0]).sqrt();
            println!("   MITC9 level 0: θ = {theta:.3}");
            assert!(theta > 0.8 && theta < 1.2);
        }
    }
}

#[test]
fn convergence_pinched_cylinder_and_hemisphere() {
    for fam in [Fam::Mitc4, Fam::Curved] {
        let m = pinched_cylinder(fam, 8);
        let id = m.grid[0][0].0;
        let (v, ext, p) = richardson_of(&m.input, 3, &|r| disp(r, id)[1].abs());
        println!("{fam:?} pinched cylinder: {v:?} → {ext:.5e} (p={p:?}), ref 1.8248e-5");
        assert!((ext / 1.8248e-5 - 1.0).abs() < 0.03, "{fam:?} pinched cylinder {ext:.4e}");

        let m = hemisphere(fam, 8);
        let id = m.grid[0][0].0;
        let (v, ext, p) = richardson_of(&m.input, 3, &|r| disp(r, id)[0].abs());
        println!("{fam:?} hemisphere: {v:?} → {ext:.5} (p={p:?}), ref 0.093 (0.0924–0.094)");
        assert!(ext > 0.0924 * 0.98 && ext < 0.094 * 1.02, "{fam:?} hemisphere {ext:.5}");
    }
}

/// Plates: Morley's skew plate (DKT; the Mindlin elements are not in the asymptotic range on
/// these meshes, which the check reports instead of extrapolating) and the simply supported and
/// clamped squares against Timoshenko's coefficients.
#[test]
fn convergence_plates() {
    let (m, wref) = morley(Fam::Dkt, 4);
    let id = m.grid[2][2].0;
    let (v, ext, p) = richardson_of(&m.input, 3, &|r| disp(r, id)[2].abs());
    println!("DKT Morley: {:?} → {:.4} (p={p:?})", v.iter().map(|x| x / wref).collect::<Vec<_>>(), ext / wref);
    assert!((ext / wref - 1.0).abs() < 0.02);

    let (m, wref) = morley(Fam::Mitc4, 4);
    let id = m.grid[2][2].0;
    let v: Vec<f64> = sequence(&m.input, 3).iter().map(|(_, r)| disp(r, id)[2].abs() / wref).collect();
    let (_, _, _, status) = richardson(&v[1..]);
    println!("MITC4 Morley: {v:?} ({status})");
    assert_ne!(status, "converging", "MITC4 on Morley is not asymptotic at these meshes and must say so");

    for fam in [Fam::Mitc4, Fam::Dkt, Fam::Mitc9] {
        for clamped in [false, true] {
            let m = square_plate(fam, 4, clamped);
            let id = m.grid[2][2].0;
            // Timoshenko & Woinowsky-Krieger: 0.00406 (simply supported), 0.00126 (clamped).
            let wref = if clamped { 0.00126 } else { 0.00406 } / plate_rigidity();
            let (v, ext, p) = richardson_of(&m.input, 3, &|r| disp(r, id)[2].abs() / wref);
            println!("{fam:?} square plate clamped={clamped}: {v:?} → {ext:.4} (p={p:?})");
            assert!((ext - 1.0).abs() < 0.012, "{fam:?} clamped={clamped}: {ext:.4}");
        }
    }
}

// ==================== Refinement ====================

fn total_reaction(r: &AnalysisResults3D) -> [f64; 3] {
    r.reactions.iter().fold([0.0; 3], |a, x| [a[0] + x.fx, a[1] + x.fy, a[2] + x.fz])
}

/// A plate on two edge beams: refinement splits the beams at the new edge nodes (so they stay
/// connected), carries the member and pressure loads over with the same totals, and gives the
/// new edge nodes the supports of their neighbours, but not the diagonal of a corner triangle.
#[test]
fn refinement_keeps_loads_supports_and_members() {
    let mut m = square_plate(Fam::Dkt, 4, false);
    m.input.sections.insert("1".into(), SolverSection3D {
        id: 1, name: None, a: 0.01, iy: 1e-5, iz: 1e-5, j: 2e-5, cw: None, as_y: None, as_z: None,
    });
    let mut eid = 1000;
    for j in 0..4 {
        let (a, b) = (m.grid[2][j].0, m.grid[2][j + 1].0);
        m.input.elements.insert(eid.to_string(), SolverElement3D {
            id: eid, elem_type: "frame".into(), node_i: a, node_j: b, material_id: 1, section_id: 1,
            release_my_start: false, release_my_end: false, release_mz_start: false, release_mz_end: false,
            release_t_start: false, release_t_end: false, local_yx: None, local_yy: None, local_yz: None, roll_angle: None,
        });
        m.input.loads.push(SolverLoad3D::Distributed(SolverDistributedLoad3D {
            element_id: eid, q_xi: 0.0, q_xj: 0.0, q_yi: 0.0, q_yj: 0.0, q_zi: -2.0, q_zj: -2.0, a: None, b: None,
        }));
        eid += 1;
    }
    let r0 = solve(&m.input);
    let refined = refine_uniform(&m.input, GeometryMode::Smooth).unwrap();
    let rf = &refined.input;
    assert_eq!(rf.plates.len(), 4 * m.input.plates.len());
    assert_eq!(rf.elements.len(), 8, "each beam along a split edge is split in two");
    // Every beam end is a node of the refined shell mesh.
    let shell_nodes: std::collections::HashSet<usize> = rf.plates.values().flat_map(|p| p.nodes).collect();
    assert!(rf.elements.values().all(|e| shell_nodes.contains(&e.node_i) && shell_nodes.contains(&e.node_j)));
    let r1 = solve(rf);
    let (t0, t1) = (total_reaction(&r0), total_reaction(&r1));
    assert!((t0[2] - t1[2]).abs() < 1e-9 * t0[2].abs(), "total load preserved: {t0:?} vs {t1:?}");
    // Supported nodes: the boundary of the 8 × 8 mesh, no more.
    let supported: std::collections::HashSet<usize> = rf.supports.values().map(|s| s.node_id).collect();
    for n in rf.nodes.values() {
        let on_edge = n.x.abs() < 1e-9 || n.y.abs() < 1e-9 || (n.x - 1.0).abs() < 1e-9 || (n.y - 1.0).abs() < 1e-9;
        assert_eq!(supported.contains(&n.id), on_edge, "node {} at ({}, {})", n.id, n.x, n.y);
    }
}

#[test]
fn refinement_refuses_what_it_cannot_extend() {
    let mut m = square_plate(Fam::Mitc4, 2, false);
    m.input.constraints.push(Constraint::EqualDOF(EqualDOFConstraint { master_node: m.grid[1][1].0, slave_node: m.grid[1][2].0, dofs: vec![0] }));
    let err = convergence_study_3d(&m.input, &ConvergenceOptions::default()).unwrap_err();
    assert_eq!(err, "constraints");
}

/// A frame-only model has nothing to estimate and says why: exact members.
#[test]
fn frames_only_estimate_says_exact() {
    let mut input = empty_input();
    input.materials.insert("1".into(), SolverMaterial { id: 1, e: 200_000.0, nu: 0.3 });
    input.sections.insert("1".into(), SolverSection3D { id: 1, name: None, a: 0.01, iy: 1e-5, iz: 1e-5, j: 2e-5, cw: None, as_y: None, as_z: None });
    input.nodes.insert("1".into(), SolverNode3D { id: 1, x: 0.0, y: 0.0, z: 0.0 });
    input.nodes.insert("2".into(), SolverNode3D { id: 2, x: 4.0, y: 0.0, z: 0.0 });
    input.elements.insert("1".into(), SolverElement3D {
        id: 1, elem_type: "frame".into(), node_i: 1, node_j: 2, material_id: 1, section_id: 1,
        release_my_start: false, release_my_end: false, release_mz_start: false, release_mz_end: false,
        release_t_start: false, release_t_end: false, local_yx: None, local_yy: None, local_yz: None, roll_angle: None,
    });
    input.supports.insert("1".into(), support(1, [true; 6]));
    input.loads.push(nodal(2, [0.0, 0.0, -10.0]));
    let r = solve(&input);
    let e = estimate(&input, &r);
    assert_eq!(e.relative_error, None);
    assert!(e.notes.contains(&"framesExact".to_string()) && e.notes.contains(&"noShellElements".to_string()));
    let json = serde_json::to_value(&e).unwrap();
    assert_eq!(json["certified"], serde_json::json!(false));
}

/// Curved-shell self-weight takes its density in kg/m³ like every other shell (it used to weigh
/// 1000× too much).
#[test]
fn curved_shell_self_weight_matches_quads() {
    let w = |fam: Fam| {
        let map = |u: f64, v: f64| ([u, v, 0.0], [0.0, 0.0, 1.0]);
        let bc = |i: usize, j: usize, mx: usize, my: usize| (i == 0 || j == 0 || i == mx || j == my).then_some([true; 6]);
        let m = surface(fam, 2, 2, 0.2, 30_000.0, 0.2, &map, &bc, Load::Weight([0.0, 0.0, -9.81]));
        total_reaction(&solve(&m.input))[2]
    };
    let (q, c) = (w(Fam::Mitc4), w(Fam::Curved));
    // ρ = 1000/t kg/m³ with g = 9.81: 9.81 kN/m² over 1 m².
    assert!((q - 9.81).abs() < 1e-9 && (c - 9.81).abs() < 1e-9, "quad {q}, curved {c}");
}
