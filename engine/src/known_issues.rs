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

/// The warnings an analysis carries whatever the model (the solver itself is affected).
pub fn for_analysis(analysis: Analysis) -> Vec<StructuredDiagnostic> {
    match analysis {
        Analysis::MaterialNonlinear => vec![hit("NL-5", 428, NL_MATERIAL)],
        Analysis::Cable => vec![hit("SP-12", 416, CABLE)],
        _ => vec![],
    }
}

/// The warnings a 2D load set carries (frame temperature loads).
pub fn loads_2d(loads: &[SolverLoad]) -> Vec<StructuredDiagnostic> {
    let thermal: Vec<usize> = loads.iter().filter_map(|l| match l { SolverLoad::Thermal(t) => Some(t.element_id), _ => None }).collect();
    if thermal.is_empty() { vec![] } else { vec![hit("FR-4", 407, THERMAL).with_elements(thermal)] }
}

/// The warnings a 3D load set carries (frame temperature loads).
pub fn loads_3d(loads: &[SolverLoad3D]) -> Vec<StructuredDiagnostic> {
    let thermal: Vec<usize> = loads.iter().filter_map(|l| match l { SolverLoad3D::Thermal(t) => Some(t.element_id), _ => None }).collect();
    if thermal.is_empty() { vec![] } else { vec![hit("FR-4", 407, THERMAL).with_elements(thermal)] }
}

/// The warnings a 3D model carries whatever its loads: curved shells, warped MITC9, arches.
/// Read from the model as given, before its curved beams are expanded into members.
pub fn model_3d(input: &SolverInput3D) -> Vec<StructuredDiagnostic> {
    let mut out = Vec::new();
    if !input.curved_shells.is_empty() {
        let mut d = hit("SH-6", 410, "Curved shells report zero bending moments, and their stresses ignore thermal strain; do not design from curved-shell moments or stresses");
        d.element_ids = crate::ordered::values(&input.curved_shells).map(|c| c.id).collect();
        d.element_kind = Some(ElementKind::CurvedShell);
        out.push(d);
    }
    // One id index for every element check: a lookup per node made the pass O(elements × nodes).
    let xyz: std::collections::HashMap<usize, [f64; 3]> = crate::ordered::values(&input.nodes).map(|n| (n.id, [n.x, n.y, n.z])).collect();
    let warped: Vec<usize> = crate::ordered::values(&input.quad9s).filter(|q| !planar(&q.nodes, &xyz)).map(|q| q.id).collect();
    if !warped.is_empty() {
        let mut d = hit("SH-7", 411, "MITC9 elements on non-planar geometry do not reproduce rigid-body motion, so curved MITC9 meshes carry spurious strains; refine or use flat elements");
        d.element_ids = warped;
        d.element_kind = Some(ElementKind::Quad9);
        out.push(d);
    }
    if input.curved_beams.iter().any(|c| !horizontal_plane(c, &xyz)) {
        out.push(hit("SP-15", 418, "Curved beams outside a horizontal plane (arches) take their segment axes from the plan orientation, so Iy and Iz swap on near-vertical segments; check the segments' stiffness and forces"));
    }
    out
}

/// The known-issue warnings that apply to a 2D result of `analysis` on `input`.
pub fn for_2d(input: &SolverInput, analysis: Analysis) -> Vec<StructuredDiagnostic> {
    let mut out = for_analysis(analysis);
    out.extend(loads_2d(&input.loads));
    out
}

/// The known-issue warnings that apply to a 3D result of `analysis` on `input`.
pub fn for_3d(input: &SolverInput3D, analysis: Analysis) -> Vec<StructuredDiagnostic> {
    let mut out = for_analysis(analysis);
    out.extend(model_3d(input));
    out.extend(loads_3d(&input.loads));
    out
}

/// Add warnings to a result's diagnostics, once each: a path that already attached a finding
/// (a prepared solve under the solve that called it) does not repeat it.
pub fn attach(target: &mut Vec<StructuredDiagnostic>, new: Vec<StructuredDiagnostic>) {
    for d in new {
        let seen = target.iter().any(|t| t.code == DiagnosticCode::KnownIssue && t.phase == d.phase);
        if !seen {
            target.push(d);
        }
    }
}

/// The 3D plastic-collapse warning (#511): the collapse factor depends on the order the solver
/// visits members, so the same structure can report different factors, some unconservative.
pub const PLASTIC_3D: &str = "3D plastic collapse: the collapse factor depends on the order the solver visits the members (renumbering or hash order), so the same structure can report different factors, some well above the true one; take the lowest of several runs only as a check, and verify the governing mechanism by hand (known issue #511, PC-3D; being fixed)";

/// Nine nodes on one plane, to 1e-6 of the element's size.
fn planar(nodes: &[usize; 9], xyz: &std::collections::HashMap<usize, [f64; 3]>) -> bool {
    let p: Vec<[f64; 3]> = nodes.iter().filter_map(|n| xyz.get(n).copied()).collect();
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
fn horizontal_plane(c: &CurvedBeamInput, xyz: &std::collections::HashMap<usize, [f64; 3]>) -> bool {
    let (Some(&a), Some(&b), Some(&m)) = (xyz.get(&c.node_start), xyz.get(&c.node_end), xyz.get(&c.node_mid)) else {
        return true;
    };
    let span = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt().max(1e-12);
    (a[2] - b[2]).abs() <= 1e-6 * span && (a[2] - m[2]).abs() <= 1e-6 * span
}
