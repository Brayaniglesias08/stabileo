//! Discretization (mesh) error: estimated, never certified.
//!
//! The certificates (`crate::certify`, docs/verification/CERTIFY.md) bound how far the computed
//! displacements are from the exact solution of the *assembled* system `K u = f`. They say
//! nothing about how far that system's solution is from the solution of the structure the mesh
//! stands for. A coarse shell mesh can be certified to twelve digits and still be wrong by 30 %.
//! This module estimates that second error (docs/verification/DISCRETIZATION.md):
//!
//! * `estimate_3d`: a recovery-based a posteriori estimate (Zienkiewicz–Zhu with
//!   superconvergent patch recovery) of the energy-norm error of every shell-type element and of
//!   the whole model, from one solve.
//! * `convergence::convergence_study_3d`: re-solves on uniformly refined meshes and reports how
//!   the governing quantities move, with a Richardson extrapolation and its observed order.
//!
//! Frame members carry no discretization error of this kind: their stiffness is the exact
//! (Euler–Bernoulli or Timoshenko) beam stiffness and member loads enter through exact fixed-end
//! forces, so nodal displacements and member forces are exact for beam theory, however few the
//! members. The estimate says so (`framesExact`) only when it holds for every member: members
//! with warping torsion (Cw > 0, cubic Hermite warping field) and curved beams (straight
//! segments) are approximate, and are listed as not estimated instead.

pub mod convergence;
pub mod recovery;
pub mod refine;
pub mod samples;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::*;
use samples::{energy_density, to_local};

/// Above this estimated relative error a result is flagged (energy norm, or change in a
/// governing quantity).
pub const DEFAULT_THRESHOLD: f64 = 0.05;

/// Error indicator of one element.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementErrorIndicator {
    /// `plate`, `quad`, `quad9`, `curvedShell` or `solidShell`.
    pub kind: String,
    pub element_id: usize,
    /// Estimated energy-norm error of the element, ‖σ* − σ_h‖ (√(kN·m)).
    pub eta: f64,
    /// The element's own energy norm ‖σ_h‖ over the same components.
    pub energy_norm: f64,
    /// η_e relative to the mean element share of the total energy, √((U + η²)/N), in percent.
    /// Uniformly spread error gives every element the global percentage; an element above the
    /// threshold is where refinement pays.
    pub local_percent: f64,
}

/// Recovery-based estimate of the discretization error of a linear static solution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscretizationEstimate {
    /// Always false: this is an estimate, not a bound. Kept in the payload so no consumer can
    /// mistake it for a certificate.
    pub certified: bool,
    /// `zz-spr`: Zienkiewicz–Zhu estimator with superconvergent patch recovery.
    pub method: String,
    /// Estimated relative energy-norm error η / √(‖u_h‖² + η²) of the shell-type elements;
    /// None when the model has none, or when it cannot be estimated (`unestimable_reason`):
    /// a 0/0 is never reported as "no error".
    pub relative_error: Option<f64>,
    /// Why the shells' error could not be estimated: `unloaded` (no load, nothing to estimate)
    /// or `noStrainEnergy` (loaded, but the mesh carries no strain above round-off — e.g. a
    /// single simply supported element whose load goes straight into its supports: the mesh is
    /// too coarse to say anything, refine it) or `tooCoarse` (no element shares a node with
    /// another of its kind, so no patch is larger than the element itself). None when estimated
    /// or when there are no shells.
    pub unestimable_reason: Option<String>,
    /// Estimated energy-norm error η (√(kN·m)).
    pub error_norm: f64,
    /// Energy norm of the computed solution over the same elements and components.
    pub solution_norm: f64,
    pub threshold: f64,
    pub exceeds_threshold: bool,
    pub elements: Vec<ElementErrorIndicator>,
    /// Number of frame members.
    pub frame_members: usize,
    /// Members whose discretization error is neither zero nor estimated: members with warping
    /// torsion (section Cw > 0), whose warping field is a cubic Hermite approximation.
    pub inexact_members: Vec<usize>,
    /// Codes of what the estimate does and does not cover (see DISCRETIZATION.md):
    /// `notCertified`, `transverseShearExcluded`, `framesExact` (every member exact),
    /// `warpingMembersNotEstimated`, `curvedBeamsSegmented`, `noShellElements`.
    pub notes: Vec<String>,
}

/// Element and global energy-norm error of the computed stress field against the recovered one.
pub(crate) struct RawEstimate {
    pub per_element: Vec<(samples::Family, usize, f64, f64)>, // (family, id, η², U)
    /// Σ E·V over the sampled elements: the energy of a unit strain everywhere.
    pub energy_scale: f64,
    /// Elements that share a node with another element of their recovery group: only those
    /// have a patch the recovered field can be smoother than their own.
    pub with_neighbours: usize,
}

/// Whether anything loads the model: a nonzero reaction or displacement (loads that go straight
/// into the supports still show as reactions).
fn is_loaded(results: &AnalysisResults3D) -> bool {
    let nonzero = |v: &[f64]| v.iter().any(|x| *x != 0.0);
    results.reactions.iter().any(|r| nonzero(&[r.fx, r.fy, r.fz, r.mx, r.my, r.mz]))
        || results.displacements.iter().any(|d| nonzero(&[d.ux, d.uy, d.uz, d.rx, d.ry, d.rz]))
}

pub(crate) fn raw_estimate(input: &SolverInput3D, results: &AnalysisResults3D) -> Result<RawEstimate, String> {
    let elems = samples::sample_elements(input, results)?;
    let coords: HashMap<usize, [f64; 3]> = input.nodes.values().map(|n| (n.id, [n.x, n.y, n.z])).collect();
    let rec = recovery::recover(&elems, &coords);
    let mut per_element = Vec::with_capacity(elems.len());
    let mut by_node: HashMap<(usize, (samples::Family, usize, u64)), usize> = HashMap::new();
    for e in &elems {
        for &n in &e.nodes {
            *by_node.entry((n, e.group_key())).or_default() += 1;
        }
    }
    let with_neighbours = elems.iter().filter(|e| e.nodes.iter().any(|&n| by_node[&(n, e.group_key())] > 1)).count();
    let energy_scale: f64 = elems.iter().map(|e| e.e * e.samples.iter().map(|s| s.weight).sum::<f64>()).sum();
    for (e, nodal) in elems.iter().zip(&rec) {
        let (mut eta2, mut u) = (0.0, 0.0);
        for s in &e.samples {
            let mut g = [0.0; 6];
            for (i, (a0, a1)) in nodal.iter().enumerate() {
                for k in 0..6 {
                    g[k] += s.shape[i] * (a0[k] + a1[k] * s.zeta);
                }
            }
            let star = to_local(&s.frame, &g);
            let d = [star[0] - s.sigma[0], star[1] - s.sigma[1], star[2] - s.sigma[2]];
            eta2 += energy_density(&d, e.e, e.nu) * s.weight;
            u += energy_density(&s.sigma, e.e, e.nu) * s.weight;
        }
        per_element.push((e.family, e.id, eta2, u));
    }
    Ok(RawEstimate { per_element, energy_scale, with_neighbours })
}

/// Estimate the discretization error of a 3D linear static solution of `input`.
pub fn estimate_3d(input: &SolverInput3D, results: &AnalysisResults3D) -> Result<DiscretizationEstimate, String> {
    let raw = raw_estimate(input, results)?;
    let eta2: f64 = raw.per_element.iter().map(|r| r.2).sum();
    let u: f64 = raw.per_element.iter().map(|r| r.3).sum();
    let n = raw.per_element.len();
    let mut notes = vec!["notCertified".to_string()];
    let mut unestimable_reason = None;
    let relative_error = if n > 0 {
        notes.push("transverseShearExcluded".into());
        let denom = u + eta2;
        // Strain energy below what a strain of 1e-12 (round-off) everywhere would store is no
        // energy at all: the estimate would be 0/0.
        let floor = 1e-24 * raw.energy_scale;
        if denom > floor && raw.with_neighbours == 0 {
            // Every element alone in its patch (a one-element mesh): the "recovered" field is the
            // element's own, the estimate would be ~0 by construction and say nothing.
            unestimable_reason = Some("tooCoarse".to_string());
            None
        } else if denom > floor {
            Some((eta2 / denom).sqrt())
        } else {
            unestimable_reason = Some(if is_loaded(results) { "noStrainEnergy" } else { "unloaded" }.to_string());
            None
        }
    } else {
        notes.push("noShellElements".into());
        None
    };
    let frame_members = input.elements.len();
    let sections: HashMap<usize, &SolverSection3D> = input.sections.values().map(|s| (s.id, s)).collect();
    let mut inexact_members: Vec<usize> = input
        .elements
        .values()
        .filter(|e| e.elem_type != "truss" && sections.get(&e.section_id).is_some_and(|s| s.cw.is_some_and(|cw| cw > 0.0)))
        .map(|e| e.id)
        .collect();
    inexact_members.sort_unstable();
    if !inexact_members.is_empty() {
        notes.push("warpingMembersNotEstimated".into());
    }
    if !input.curved_beams.is_empty() {
        notes.push("curvedBeamsSegmented".into());
    }
    if frame_members > 0 && inexact_members.is_empty() && input.curved_beams.is_empty() {
        notes.push("framesExact".into());
    }
    let mean = if n > 0 { ((u + eta2) / n as f64).sqrt() } else { 0.0 };
    let elements = raw
        .per_element
        .iter()
        .map(|&(fam, id, e2, ue)| ElementErrorIndicator {
            kind: fam.name().to_string(),
            element_id: id,
            eta: e2.sqrt(),
            energy_norm: ue.sqrt(),
            local_percent: if mean > 0.0 { 100.0 * e2.sqrt() / mean } else { 0.0 },
        })
        .collect();
    let threshold = DEFAULT_THRESHOLD;
    Ok(DiscretizationEstimate {
        certified: false,
        method: "zz-spr".into(),
        // A loaded mesh too coarse to estimate is flagged like one above the threshold.
        exceeds_threshold: relative_error.is_some_and(|r| r > threshold)
            || matches!(unestimable_reason.as_deref(), Some("noStrainEnergy" | "tooCoarse")),
        relative_error,
        unestimable_reason,
        error_norm: eta2.sqrt(),
        solution_norm: u.sqrt(),
        threshold,
        elements,
        frame_members,
        inexact_members,
        notes,
    })
}

/// Solve a 3D model (linear static) and estimate its discretization error.
pub fn solve_and_estimate_3d(input: &SolverInput3D) -> Result<DiscretizationEstimate, String> {
    let results = crate::solver::linear::solve_3d(input)?;
    estimate_3d(input, &results)
}
