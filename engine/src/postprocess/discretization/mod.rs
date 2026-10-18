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
//! members. The estimate says so instead of reporting a number for them.

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
    /// None when the model has none.
    pub relative_error: Option<f64>,
    /// Estimated energy-norm error η (√(kN·m)).
    pub error_norm: f64,
    /// Energy norm of the computed solution over the same elements and components.
    pub solution_norm: f64,
    pub threshold: f64,
    pub exceeds_threshold: bool,
    pub elements: Vec<ElementErrorIndicator>,
    /// Number of frame members (no discretization error: exact beam stiffness and loads).
    pub frame_members: usize,
    /// Codes of what the estimate does and does not cover (see DISCRETIZATION.md):
    /// `notCertified`, `transverseShearExcluded`, `framesExact`, `noShellElements`,
    /// `curvedBeamsSegmented`.
    pub notes: Vec<String>,
}

/// Element and global energy-norm error of the computed stress field against the recovered one.
pub(crate) struct RawEstimate {
    pub per_element: Vec<(samples::Family, usize, f64, f64)>, // (family, id, η², U)
}

pub(crate) fn raw_estimate(input: &SolverInput3D, results: &AnalysisResults3D) -> Result<RawEstimate, String> {
    let elems = samples::sample_elements(input, results)?;
    let coords: HashMap<usize, [f64; 3]> = input.nodes.values().map(|n| (n.id, [n.x, n.y, n.z])).collect();
    let rec = recovery::recover(&elems, &coords);
    let mut per_element = Vec::with_capacity(elems.len());
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
    Ok(RawEstimate { per_element })
}

/// Estimate the discretization error of a 3D linear static solution of `input`.
pub fn estimate_3d(input: &SolverInput3D, results: &AnalysisResults3D) -> Result<DiscretizationEstimate, String> {
    let raw = raw_estimate(input, results)?;
    let eta2: f64 = raw.per_element.iter().map(|r| r.2).sum();
    let u: f64 = raw.per_element.iter().map(|r| r.3).sum();
    let n = raw.per_element.len();
    let mut notes = vec!["notCertified".to_string()];
    let relative_error = if n > 0 {
        notes.push("transverseShearExcluded".into());
        let denom = u + eta2;
        Some(if denom > 0.0 { (eta2 / denom).sqrt() } else { 0.0 })
    } else {
        notes.push("noShellElements".into());
        None
    };
    let frame_members = input.elements.len();
    if frame_members > 0 {
        notes.push("framesExact".into());
    }
    if !input.curved_beams.is_empty() {
        notes.push("curvedBeamsSegmented".into());
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
        exceeds_threshold: relative_error.is_some_and(|r| r > threshold),
        relative_error,
        error_norm: eta2.sqrt(),
        solution_norm: u.sqrt(),
        threshold,
        elements,
        frame_members,
        notes,
    })
}

/// Solve a 3D model (linear static) and estimate its discretization error.
pub fn solve_and_estimate_3d(input: &SolverInput3D) -> Result<DiscretizationEstimate, String> {
    let results = crate::solver::linear::solve_3d(input)?;
    estimate_3d(input, &results)
}
