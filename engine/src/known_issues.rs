//! Known issues that can give a wrong result, contained until they are fixed (roadmap Release
//! safety item 7, #447; the triage is docs/verification/DERIVATION_TRIAGE.md).
//!
//! Each entry is a finding with a stable id (its code in the derivation review), the issue that
//! tracks it, a test on the input saying whether this result is affected, and a message. Every
//! solver entry point it concerns attaches a `KnownIssue` warning to the results it affects: the
//! id in `phase`, the issue number in `value`, the affected elements in `element_ids`. The web
//! translates the message by id. The fix of a finding removes its entry in the same pull request,
//! and the tests below fail while an entry names a fixed demonstration.

use crate::types::*;

/// The analyses the entries distinguish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Analysis {
    /// The linear static solve and everything built on it in the same call.
    Linear,
    MaterialNonlinear,
    Cable,
    Ssi,
}

fn hit(id: &str, issue: u32, message: &str) -> StructuredDiagnostic {
    let mut d = StructuredDiagnostic::global(DiagnosticCode::KnownIssue, Severity::Warning, format!("{message} (known issue #{issue}, {id}; being fixed)"));
    d.phase = Some(id.to_string());
    d.value = Some(issue as f64);
    d
}

const NL_MATERIAL: &str = "Material-nonlinear results are approximate: the model is a whole-member secant rather than plasticity, so the response jumps at first yield (NL-5), and in 3D the yield surface is less conservative than in 2D (NL-6). Check the collapse load factor with plastic collapse";
const CABLE: &str = "Cable tensions can be too low in statically indeterminate cable systems (the chord law's tangent is used as a secant); check the governing cable and anchorage forces independently";
const THERMAL: &str = "Frame and truss temperature loads use α = 12·10⁻⁶ /°C and a depth of √(12·I/A) whatever the material and section; for other materials or real section depths the thermal forces are off in proportion";

fn soil(curve: &crate::solver::soil_curves::SoilCurve) -> Option<(&'static str, u32, &'static str)> {
    use crate::solver::soil_curves::SoilCurve as C;
    match curve {
        C::PySoftClay { .. } | C::PySand { .. } => Some(("AC-12", 421, "p-y curves depart from the cited references (soft clay without the J term, sand A = 0.9 and its k table); lateral pile and soil-spring results are off")),
        C::PyStiffClay { .. } => Some(("AC-13", 422, "The \"stiff clay\" p-y curve is discontinuous at y50 and is not Reese's stiff-clay curve; lateral pile and soil-spring results are off")),
        C::Tz { .. } | C::Qz { .. } => Some(("AC-16", 423, "t-z and q-z curves are missing the shaft perimeter or tip area (they return kPa·m, not kN); axial pile springs are off by that factor")),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

/// The known-issue warnings that apply to a 2D result of `analysis`.
pub fn for_2d(input: &SolverInput, analysis: Analysis) -> Vec<StructuredDiagnostic> {
    let mut out = Vec::new();
    match analysis {
        Analysis::MaterialNonlinear => out.push(hit("NL-5", 428, NL_MATERIAL)),
        Analysis::Cable => out.push(hit("SP-12", 416, CABLE)),
        _ => {}
    }
    let thermal_frames: Vec<usize> = input
        .loads
        .iter()
        .filter_map(|l| match l {
            SolverLoad::Thermal(t) => Some(t.element_id),
            _ => None,
        })
        .collect();
    if !thermal_frames.is_empty() {
        out.push(hit("FR-4", 407, THERMAL).with_elements(thermal_frames));
    }
    out
}

/// The known-issue warnings that apply to a 3D result of `analysis`.
pub fn for_3d(input: &SolverInput3D, analysis: Analysis) -> Vec<StructuredDiagnostic> {
    let mut out = Vec::new();
    match analysis {
        Analysis::MaterialNonlinear => out.push(hit("NL-5", 428, NL_MATERIAL)),
        Analysis::Cable => out.push(hit("SP-12", 416, CABLE)),
        _ => {}
    }
    if !input.curved_shells.is_empty() {
        let mut d = hit("SH-6", 410, "Curved shells report zero bending moments, and their stresses ignore thermal strain; do not design from curved-shell moments or stresses");
        d.element_ids = input.curved_shells.values().map(|c| c.id).collect();
        d.element_kind = Some(ElementKind::CurvedShell);
        out.push(d);
    }
    let warped: Vec<usize> = input.quad9s.values().filter(|q| !planar(&q.nodes, input)).map(|q| q.id).collect();
    if !warped.is_empty() {
        let mut d = hit("SH-7", 411, "MITC9 elements on non-planar geometry do not reproduce rigid-body motion, so curved MITC9 meshes carry spurious strains; refine or use flat elements");
        d.element_ids = warped;
        d.element_kind = Some(ElementKind::Quad9);
        out.push(d);
    }
    if input.curved_beams.iter().any(|c| !horizontal_plane(c, input)) {
        out.push(hit("SP-15", 418, "Curved beams outside a horizontal plane (arches) take their segment axes from the plan orientation, so Iy and Iz swap on near-vertical segments; check the segments' stiffness and forces"));
    }
    let thermal_frames: Vec<usize> = input
        .loads
        .iter()
        .filter_map(|l| match l {
            SolverLoad3D::Thermal(t) => Some(t.element_id),
            _ => None,
        })
        .collect();
    if !thermal_frames.is_empty() {
        out.push(hit("FR-4", 407, THERMAL).with_elements(thermal_frames));
    }
    out
}

/// The warnings for an SSI analysis: one per soil-curve family the springs use.
pub fn for_ssi(curves: &[&crate::solver::soil_curves::SoilCurve]) -> Vec<StructuredDiagnostic> {
    let mut seen: Vec<&'static str> = Vec::new();
    let mut out = Vec::new();
    for c in curves {
        if let Some((id, issue, msg)) = soil(c) {
            if !seen.contains(&id) {
                seen.push(id);
                out.push(hit(id, issue, msg));
            }
        }
    }
    out
}

fn node_xyz(input: &SolverInput3D, id: usize) -> Option<[f64; 3]> {
    input.nodes.values().find(|n| n.id == id).map(|n| [n.x, n.y, n.z])
}

/// Nine nodes on one plane, to 1e-6 of the element's size.
fn planar(nodes: &[usize; 9], input: &SolverInput3D) -> bool {
    let p: Vec<[f64; 3]> = nodes.iter().filter_map(|&n| node_xyz(input, n)).collect();
    if p.len() < 9 {
        return true; // a missing node is refused by validation
    }
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let (u, v) = (sub(p[2], p[0]), sub(p[1], p[3]));
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let size = p.iter().map(|q| { let d = sub(*q, p[0]); (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() }).fold(0.0, f64::max);
    if nl <= 0.0 || size <= 0.0 {
        return true;
    }
    p.iter().all(|q| { let d = sub(*q, p[0]); ((d[0] * n[0] + d[1] * n[1] + d[2] * n[2]) / nl).abs() <= 1e-6 * size })
}

/// A curved beam whose three points lie in a horizontal plane (a plan curve).
fn horizontal_plane(c: &CurvedBeamInput, input: &SolverInput3D) -> bool {
    let (Some(a), Some(b), Some(m)) = (node_xyz(input, c.node_start), node_xyz(input, c.node_end), node_xyz(input, c.node_mid)) else {
        return true;
    };
    let span = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt().max(1e-12);
    (a[2] - b[2]).abs() <= 1e-6 * span && (a[2] - m[2]).abs() <= 1e-6 * span
}
