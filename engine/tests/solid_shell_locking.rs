//! Solid-shell (SHB8-ANS-EAS) locking and convergence benchmarks (issue #400).
//!
//! The solid shell keeps the full 3D constitutive law, so through-thickness Poisson
//! coupling makes bending stiffer than Kirchhoff/Mindlin theory unless the thickness
//! strain may vary linearly through the thickness. With the trilinear field alone
//! ε_ζζ is constant in ζ, the bending stresses carry a spurious σ_zz, and the plate
//! rigidity becomes E(1 − ν)/((1 + ν)(1 − 2ν))·t³/12 instead of E/(1 − ν²)·t³/12:
//! (1 − ν)²/(1 − 2ν) = 1.225 too stiff at ν = 0.3, a centre deflection of 0.816 of
//! Navier's however fine the mesh ("Poisson thickness locking"). These tests pin the
//! cure (one EAS thickness mode, see element/solid_shell.rs) against closed-form and
//! MacNeal–Harder references, with tolerances set by what each reference supports.
//!
//! References:
//! - Navier series (Timoshenko & Woinowsky-Krieger §30) for the simply supported plate,
//!   Kirchhoff; the Mindlin version adds the shear term D·α²/(κGt) per harmonic.
//! - MacNeal & Harder (1985), "A proposed standard set of problems to test finite
//!   element accuracy", FEA&D 1: straight cantilever, Scordelis–Lo, pinched cylinder,
//!   hemisphere with an 18° hole.
//! - Exact 3D elasticity for pure bending of a prismatic strip (St Venant).

use dedaliano_engine::solver::linear;
use dedaliano_engine::types::*;
use std::collections::HashMap;
use std::f64::consts::PI;

// ==================== Model building ====================

fn support(node_id: usize, f: [bool; 3]) -> SolverSupport3D {
    SolverSupport3D {
        node_id, rx: f[0], ry: f[1], rz: f[2], rrx: false, rry: false, rrz: false,
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

/// A one-element-thick solid-shell mesh: nx × ny elements on the map
/// (u, v) ∈ [0,1]² → (mid-surface point, unit normal), faces at ±t/2 along the normal.
/// `bc(i, j, nx, ny)` restrains the translations of BOTH nodes of grid point (i, j).
struct Model {
    input: SolverInput3D,
    /// (bottom, top) node ids of grid point (i, j).
    grid: Vec<Vec<(usize, usize)>>,
}

fn solid_surface(
    nx: usize, ny: usize, t: f64, e_mpa: f64, nu: f64,
    map: &dyn Fn(f64, f64) -> ([f64; 3], [f64; 3]),
    bc: &dyn Fn(usize, usize, usize, usize) -> Option<[bool; 3]>,
) -> Model {
    let mut input = empty_input();
    input.materials.insert("1".into(), SolverMaterial { id: 1, e: e_mpa, nu });
    let mut grid = vec![vec![(0usize, 0usize); ny + 1]; nx + 1];
    let mut nid = 1;
    for i in 0..=nx {
        for j in 0..=ny {
            let (p, n) = map(i as f64 / nx as f64, j as f64 / ny as f64);
            for (k, s) in [-0.5, 0.5].into_iter().enumerate() {
                let id = nid + k;
                input.nodes.insert(id.to_string(), SolverNode3D { id, x: p[0] + s * t * n[0], y: p[1] + s * t * n[1], z: p[2] + s * t * n[2] });
            }
            grid[i][j] = (nid, nid + 1);
            nid += 2;
        }
    }
    let mut eid = 1;
    for i in 0..nx {
        for j in 0..ny {
            let c = [grid[i][j], grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]];
            let nodes = [c[0].0, c[1].0, c[2].0, c[3].0, c[0].1, c[1].1, c[2].1, c[3].1];
            input.solid_shells.insert(eid.to_string(), SolverSolidShellElement { id: eid, nodes, material_id: 1 });
            eid += 1;
        }
    }
    let mut sid = 1;
    for i in 0..=nx {
        for j in 0..=ny {
            if let Some(f) = bc(i, j, nx, ny) {
                for n in [grid[i][j].0, grid[i][j].1] {
                    input.supports.insert(sid.to_string(), support(n, f));
                    sid += 1;
                }
            }
        }
    }
    Model { input, grid }
}

fn nodal(node_id: usize, f: [f64; 3]) -> SolverLoad3D {
    SolverLoad3D::Nodal(SolverNodalLoad3D { node_id, fx: f[0], fy: f[1], fz: f[2], mx: 0.0, my: 0.0, mz: 0.0, bw: None })
}

/// Point force at grid point (i, j), shared equally by its two face nodes.
fn point_load(m: &mut Model, i: usize, j: usize, f: [f64; 3]) {
    let (b, t) = m.grid[i][j];
    for id in [b, t] {
        m.input.loads.push(nodal(id, [0.5 * f[0], 0.5 * f[1], 0.5 * f[2]]));
    }
}

fn pressure_all(m: &mut Model, q: f64) {
    let ids: Vec<usize> = m.input.solid_shells.values().map(|s| s.id).collect();
    for id in ids {
        m.input.loads.push(SolverLoad3D::SolidShellPressure(SolverPressureLoad { element_id: id, pressure: q }));
    }
}

fn solve(m: &Model) -> AnalysisResults3D {
    linear::solve_3d(&m.input).expect("solve")
}

fn disp(r: &AnalysisResults3D, id: usize) -> [f64; 3] {
    let d = r.displacements.iter().find(|d| d.node_id == id).expect("node");
    [d.ux, d.uy, d.uz]
}

/// Mid-surface displacement at grid point (i, j): mean of the two face nodes.
fn mid(r: &AnalysisResults3D, m: &Model, i: usize, j: usize) -> [f64; 3] {
    let (a, b) = (disp(r, m.grid[i][j].0), disp(r, m.grid[i][j].1));
    [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1]), 0.5 * (a[2] + b[2])]
}

/// Observed convergence order from errors on meshes h, h/2.
fn order(e_coarse: f64, e_fine: f64) -> f64 {
    (e_coarse / e_fine).log2()
}

// ==================== Plates ====================

const E_PLATE: f64 = 200_000.0; // MPa (kN/m² × 1e-3)
const NU: f64 = 0.3;

/// Square plate, side 1, uniform load q = 1 (downwards), simply supported on all edges.
/// `hard`: also restrain the edge-tangential displacement of both faces (hard support,
/// θ_s = 0, as in the Navier/Mindlin series); otherwise only w = 0 (soft support) plus
/// in-plane pins at two corners.
fn ss_plate(n: usize, t: f64, hard: bool) -> Model {
    let map = |u: f64, v: f64| ([u, v, 0.0], [0.0, 0.0, 1.0]);
    let bc = move |i: usize, j: usize, nx: usize, ny: usize| -> Option<[bool; 3]> {
        let (xe, ye) = (i == 0 || i == nx, j == 0 || j == ny);
        if !(xe || ye) {
            return None;
        }
        if hard {
            // Edge x = const: tangential is y; edge y = const: tangential is x.
            return Some([ye || (i == 0 && j == 0), xe, true]);
        }
        let pin = (i == 0 && j == 0, i == nx && j == 0);
        Some([pin.0, pin.0 || pin.1, true])
    };
    let mut m = solid_surface(n, n, t, E_PLATE, NU, &map, &bc);
    pressure_all(&mut m, 1.0);
    m
}

/// Navier centre deflection of the simply supported unit square plate under q = 1.
/// `shear`: Mindlin (hard support) with κ = 5/6, else Kirchhoff.
fn navier_centre(t: f64, shear: bool) -> f64 {
    let e = E_PLATE * 1000.0;
    let d = e * t.powi(3) / (12.0 * (1.0 - NU * NU));
    let kgt = 5.0 / 6.0 * e / (2.0 * (1.0 + NU)) * t;
    let mut w = 0.0;
    for m in (1..600).step_by(2) {
        for n in (1..600).step_by(2) {
            let (mf, nf) = (m as f64, n as f64);
            let a2 = PI * PI * (mf * mf + nf * nf);
            let qmn = 16.0 / (PI * PI * mf * nf);
            let sign = if (m / 2 + n / 2) % 2 == 0 { 1.0 } else { -1.0 };
            let phi = if shear { 1.0 + d * a2 / kgt } else { 1.0 };
            w += sign * qmn / (d * a2 * a2) * phi;
        }
    }
    w
}

fn plate_ratio(n: usize, t: f64, hard: bool, shear_ref: bool) -> f64 {
    let m = ss_plate(n, t, hard);
    let r = solve(&m);
    -mid(&r, &m, n / 2, n / 2)[2] / navier_centre(t, shear_ref)
}

/// Issue #400 reproduction: thin simply supported plate, t/a = 0.01 and 0.001, must
/// converge to the Kirchhoff (Navier) centre deflection, not stall at 0.80–0.82.
/// Thin limit: Mindlin − Kirchhoff is O((t/a)²) ≈ 2e-4 at t/a = 0.01, below the tolerance.
#[test]
fn thin_plate_converges_to_kirchhoff() {
    for t in [0.01, 0.001] {
        let ratios: Vec<f64> = [4, 8, 16, 32].iter().map(|&n| plate_ratio(n, t, false, false)).collect();
        let errs: Vec<f64> = ratios.iter().map(|r| (r - 1.0).abs()).collect();
        eprintln!("thin SS plate t/a = {t}: ratios {ratios:.5?}, orders {:.2?}",
            errs.windows(2).map(|w| order(w[0], w[1])).collect::<Vec<_>>());
        assert!(errs[3] < 0.005, "t/a = {t}, 32×32: w/w_Kirchhoff = {:.5}", ratios[3]);
        assert!(errs[2] < 0.015, "t/a = {t}, 16×16: w/w_Kirchhoff = {:.5}", ratios[2]);
        // Monotone, second-order convergence of the centre deflection (bilinear field).
        for k in 1..3 {
            assert!(errs[k + 1] < errs[k], "t/a = {t}: error does not decrease: {errs:?}");
        }
        assert!(order(errs[2], errs[3]) > 1.7, "t/a = {t}: observed order {:.2}", order(errs[2], errs[3]));
    }
}

/// Thick plates (t/a = 0.1, 0.2), hard simple support, against the Mindlin Navier series
/// (κ = 5/6). The solid shell solves 3D elasticity, which differs from Mindlin by the
/// transverse normal strain and the shear-correction approximation: at t/a = 0.1 that
/// model difference is ~0.5 %, at 0.2 ~1.5 % (Srinivas & Rao 1970 3D solutions).
#[test]
fn thick_plate_converges_to_mindlin() {
    for (t, tol) in [(0.1, 0.01), (0.2, 0.025)] {
        let ratios: Vec<f64> = [8, 16, 32].iter().map(|&n| plate_ratio(n, t, true, true)).collect();
        eprintln!("thick SS plate t/a = {t}: w/w_Mindlin {ratios:.5?}, w/w_Kirchhoff (32) {:.4}",
            ratios[2] * navier_centre(t, true) / navier_centre(t, false));
        assert!((ratios[2] - 1.0).abs() < tol, "t/a = {t}, 32×32: w/w_Mindlin = {:.5}", ratios[2]);
    }
}

/// Pure bending of a thin strip with ν = 0.3 (end couple, one element through the
/// thickness): the 3D elasticity solution has ε_zz = −ν/(1 − ν)·ε_xx (plane strain) or
/// −ν·ε_xx (free lateral faces), linear in z. The trilinear element alone cannot carry it
/// and is too stiff by (1 − ν)²/(1 − 2ν) resp. ~1/(1 − ν²); with the EAS thickness mode
/// the tip deflection is exact for every t/L.
#[test]
fn pure_bending_strip_is_exact_with_poisson() {
    let (l, b, nx) = (1.0, 0.1, 8);
    for t in [0.1, 0.01, 0.001] {
        for plane_strain in [true, false] {
            let map = |u: f64, v: f64| ([u * l, (v - 0.5) * b, 0.0], [0.0, 0.0, 1.0]);
            let bc = move |i: usize, j: usize, _nx: usize, _ny: usize| -> Option<[bool; 3]> {
                // Clamp x = 0 against x only (plus enough to stop rigid motion), so the
                // root section is free to contract — the St Venant end condition.
                let mut f = [i == 0, false, i == 0 && j == 0];
                if plane_strain {
                    f[1] = true;
                } else if i == 0 && j == 0 {
                    f[1] = true;
                }
                f.iter().any(|&x| x).then_some(f)
            };
            let mut m = solid_surface(nx, 1, t, E_PLATE, NU, &map, &bc);
            // Root: w = 0 on the bottom face at y = ±b/2 (equal there by symmetry of the
            // anticlastic term), so no rigid rotation about x; the top nodes stay free.
            let root_b1 = m.grid[0][1].0;
            m.input.supports.insert("rb".into(), support(root_b1, [true, plane_strain, true]));
            // End couple M (per unit width 1): ±F on the tip face nodes, F·t = M·b.
            let mmt = 1e-6 * t.powi(2); // keep displacements small
            let f = mmt * b / t / 2.0;
            for j in 0..2 {
                let (bot, top) = m.grid[nx][j];
                m.input.loads.push(nodal(bot, [f, 0.0, 0.0]));
                m.input.loads.push(nodal(top, [-f, 0.0, 0.0]));
            }
            let r = solve(&m);
            let w_tip = 0.5 * (mid(&r, &m, nx, 0)[2] + mid(&r, &m, nx, 1)[2]);
            let e = E_PLATE * 1000.0;
            let ei = if plane_strain { e / (1.0 - NU * NU) } else { e } * t.powi(3) / 12.0;
            // M per unit width (mmt) with the bottom fibres stretched → w > 0 at the tip.
            let exact = mmt * l * l / (2.0 * ei);
            let ratio = w_tip / exact;
            eprintln!("pure bending t/L = {t}, plane strain {plane_strain}: w/w_exact = {ratio:.6}");
            assert!((ratio - 1.0).abs() < 1e-6, "t/L = {t}, plane strain {plane_strain}: w/w_exact = {ratio:.8}");
        }
    }
}

// ==================== Shells (MacNeal–Harder) ====================

/// Scordelis–Lo roof, one quarter: R = 25, L = 50, 40°, t = 0.25, E = 4.32e8, ν = 0, weight
/// 90 per unit area. Midspan free-edge vertical displacement, reference 0.3024 (MacNeal–Harder;
/// 0.3086 is the deep-shell value — the spread bounds what the reference supports).
fn scordelis(n: usize) -> f64 {
    let (r, half_l, th, t) = (25.0, 25.0, 40.0_f64.to_radians(), 0.25);
    let map = move |u: f64, v: f64| {
        let a = v * th;
        ([u * half_l, r * a.sin(), r * a.cos() - r], [0.0, a.sin(), a.cos()])
    };
    // i: along x (0 = midspan symmetry, n = diaphragm); j: around (0 = crown, n = free edge).
    let bc = |i: usize, j: usize, nx: usize, _ny: usize| -> Option<[bool; 3]> {
        let f = [i == 0, j == 0 || i == nx, i == nx];
        f.iter().any(|&b| b).then_some(f)
    };
    let mut m = solid_surface(n, n, t, 4.32e8 / 1000.0, 0.0, &map, &bc);
    let ids: Vec<usize> = m.input.solid_shells.values().map(|s| s.id).collect();
    for id in ids {
        // Force per unit area 90 = ρ/1000 · t · g with g = 1.
        m.input.loads.push(SolverLoad3D::SolidShellSelfWeight(SolverSolidShellSelfWeightLoad {
            element_id: id, density: 90.0 / t * 1000.0, gx: 0.0, gy: 0.0, gz: -1.0,
        }));
    }
    let r = solve(&m);
    -mid(&r, &m, 0, n)[2]
}

/// Pinched cylinder with rigid diaphragms, one eighth: R = 300, L = 600, t = 3, E = 3e6,
/// ν = 0.3, P = 1. Radial displacement under the load, reference 1.8248e-5.
fn pinched_cylinder(n: usize) -> f64 {
    let (r, half_l) = (300.0, 300.0);
    let map = move |u: f64, v: f64| {
        let th = v * PI / 2.0;
        ([u * half_l, r * th.cos(), r * th.sin()], [0.0, th.cos(), th.sin()])
    };
    // i = 0: midspan symmetry (ux = 0); i = n: diaphragm (uy = uz = 0);
    // j = 0: plane z = 0 (uz = 0); j = n: plane y = 0 (uy = 0).
    let bc = |i: usize, j: usize, nx: usize, ny: usize| -> Option<[bool; 3]> {
        let f = [i == 0, i == nx || j == ny, i == nx || j == 0];
        f.iter().any(|&b| b).then_some(f)
    };
    let mut m = solid_surface(n, n, 3.0, 3000.0, 0.3, &map, &bc);
    point_load(&mut m, 0, 0, [0.0, -0.25, 0.0]);
    let r = solve(&m);
    -mid(&r, &m, 0, 0)[1]
}

/// Hemispherical shell with an 18° hole, one quarter: R = 10, t = 0.04, E = 6.825e7, ν = 0.3,
/// ±2 radial point loads at the equator. Reference 0.094 (MacNeal–Harder; 0.0924 converged).
fn hemisphere(n: usize) -> f64 {
    let r = 10.0;
    let map = move |u: f64, v: f64| {
        let (phi, th) = (u * 72.0_f64.to_radians(), v * PI / 2.0);
        let nrm = [phi.cos() * th.cos(), phi.cos() * th.sin(), phi.sin()];
        ([r * nrm[0], r * nrm[1], r * nrm[2]], nrm)
    };
    // j = 0: plane y = 0 (uy = 0); j = n: plane x = 0 (ux = 0); one uz restraint.
    let bc = |i: usize, j: usize, nx: usize, ny: usize| -> Option<[bool; 3]> {
        let f = [j == ny, j == 0, i == nx && j == 0];
        f.iter().any(|&b| b).then_some(f)
    };
    let mut m = solid_surface(n, n, 0.04, 68250.0, 0.3, &map, &bc);
    // Each load lies on a symmetry plane: the quarter carries half of it.
    point_load(&mut m, 0, 0, [1.0, 0.0, 0.0]);
    point_load(&mut m, 0, n, [0.0, -1.0, 0.0]);
    let r = solve(&m);
    mid(&r, &m, 0, 0)[0]
}

#[test]
fn scordelis_lo_roof() {
    let v: Vec<f64> = [4, 8, 16].iter().map(|&n| scordelis(n) / 0.3024).collect();
    eprintln!("Scordelis–Lo w/0.3024: {v:.4?}");
    // ν = 0: the EAS mode is inactive on parallel faces; any change here is a regression.
    assert!((v[2] - 1.0).abs() < 0.03, "16×16: {:.4}", v[2]);
    assert!((v[1] - 1.0).abs() < 0.05, "8×8: {:.4}", v[1]);
}

#[test]
fn pinched_cylinder_with_diaphragms() {
    let v: Vec<f64> = [8, 16, 32].iter().map(|&n| pinched_cylinder(n) / 1.8248e-5).collect();
    eprintln!("pinched cylinder w/1.8248e-5: {v:.4?}");
    assert!((v[2] - 1.0).abs() < 0.05, "32×32: {:.4}", v[2]);
}

#[test]
fn hemisphere_with_hole() {
    let v: Vec<f64> = [8, 16, 32].iter().map(|&n| hemisphere(n) / 0.094).collect();
    eprintln!("hemisphere u/0.094: {v:.4?}");
    assert!((v[2] - 1.0).abs() < 0.05, "32×32: {:.4}", v[2]);
}

// ==================== MacNeal–Harder straight cantilever ====================

/// MacNeal–Harder straight cantilever: L = 6, width 0.2, t = 0.1, E = 1e7, ν = 0.3, 6 × 1
/// elements, unit tip shear. Shapes: regular, trapezoidal (interior edges alternately at ±45°),
/// parallelogram (all interior edges at 45°). Returns the tip displacement along the load,
/// `out_of_plane` = along the thickness (reference 0.4321), else across the width (0.1081).
fn mh_cantilever(shape: &str, out_of_plane: bool) -> f64 {
    let (l, w, t, n) = (6.0, 0.2, 0.1, 6);
    // Plan x of grid point (i, j): j = 0 at y = 0, j = 1 at y = w.
    let x = |i: usize, j: usize| -> f64 {
        let base = l * i as f64 / n as f64;
        if i == 0 || i == n {
            return base;
        }
        let d = 0.5 * w; // 45° across the width w
        match shape {
            "trapezoidal" => base + if i % 2 == 1 { d } else { -d } * if j == 0 { -1.0 } else { 1.0 },
            "parallelogram" => base + if j == 0 { -d } else { d },
            _ => base,
        }
    };
    let mut input = empty_input();
    input.materials.insert("1".into(), SolverMaterial { id: 1, e: 1e7 / 1000.0, nu: 0.3 });
    let id = |i: usize, j: usize, k: usize| 1 + (i * 2 + j) * 2 + k;
    for i in 0..=n {
        for j in 0..2 {
            for k in 0..2 {
                let nid = id(i, j, k);
                input.nodes.insert(nid.to_string(), SolverNode3D { id: nid, x: x(i, j), y: w * j as f64, z: t * k as f64 });
            }
        }
    }
    for i in 0..n {
        let nodes = [id(i, 0, 0), id(i + 1, 0, 0), id(i + 1, 1, 0), id(i, 1, 0), id(i, 0, 1), id(i + 1, 0, 1), id(i + 1, 1, 1), id(i, 1, 1)];
        input.solid_shells.insert((i + 1).to_string(), SolverSolidShellElement { id: i + 1, nodes, material_id: 1 });
    }
    for j in 0..2 {
        for k in 0..2 {
            input.supports.insert(id(0, j, k).to_string(), support(id(0, j, k), [true; 3]));
            let f = if out_of_plane { [0.0, 0.0, 0.25] } else { [0.0, 0.25, 0.0] };
            input.loads.push(nodal(id(n, j, k), f));
        }
    }
    let r = linear::solve_3d(&input).expect("solve");
    let c = if out_of_plane { 2 } else { 1 };
    (0..2).flat_map(|j| (0..2).map(move |k| (j, k))).map(|(j, k)| disp(&r, id(n, j, k))[c]).sum::<f64>() / 4.0
}

#[test]
fn macneal_harder_straight_cantilever() {
    for shape in ["regular", "trapezoidal", "parallelogram"] {
        let out = mh_cantilever(shape, true) / 0.4321;
        let inp = mh_cantilever(shape, false) / 0.1081;
        eprintln!("MH straight cantilever {shape}: out-of-plane {out:.4}, in-plane {inp:.4}");
    }
}
