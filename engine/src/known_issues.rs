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
const THERMAL: &str = "Frame and truss temperature loads use α = 12·10⁻⁶ /°C and a depth of √(12·I/A) whatever the material and section; for other materials or real section depths the thermal forces are off in proportion";

/// The warnings an analysis carries whatever the model (the solver itself is affected).
pub fn for_analysis(analysis: Analysis) -> Vec<StructuredDiagnostic> {
    match analysis {
        Analysis::MaterialNonlinear => vec![hit("NL-5", 428, NL_MATERIAL)],
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
    let mut out = Vec::new();
    let thermal: Vec<usize> = loads.iter().filter_map(|l| match l { SolverLoad3D::Thermal(t) => Some(t.element_id), _ => None }).collect();
    if !thermal.is_empty() {
        out.push(hit("FR-4", 407, THERMAL).with_elements(thermal));
    }
    out
}

/// The warnings a 3D model carries whatever its loads: warped MITC9.
/// The warnings a 3D model carries whatever its loads: curved shells, arches.
/// Read from the model as given, before its curved beams are expanded into members.
pub fn model_3d(input: &SolverInput3D) -> Vec<StructuredDiagnostic> {
    let mut out = Vec::new();
    // One id index for every element check: a lookup per node made the pass O(elements × nodes).
    let xyz: std::collections::HashMap<usize, [f64; 3]> = crate::ordered::values(&input.nodes).map(|n| (n.id, [n.x, n.y, n.z])).collect();
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


