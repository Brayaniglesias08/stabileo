//! End-to-end certificates: re-assemble `K` and `f` from the model, not from
//! the engine. See "End-to-end certificates" in `docs/verification/CERTIFY.md`.
//!
//! The linear certificate (`certify::solver`) proves that the returned
//! displacements solve the f64 `K` and `f` the engine assembled. This module
//! closes the gap between the model and that system: it re-derives the
//! stiffness and loads of every member from the model's JSON fields, in
//! interval arithmetic, from closed-form formulas written for this checker
//! (`element.rs`: cantilever flexibility + equilibrium, static condensation,
//! work-equivalent loads on the exact Timoshenko shapes), with its own DOF
//! numbering, supports, inclined supports and constraints (`u = C q + p`).
//! Nothing in `crate::element` or `crate::solver` is called to build it.
//!
//! Two checks, both required for `EndToEnd::Certified`:
//!
//! 1. **Bound against the model.** With `A = Cᵀ K C` enclosed in intervals
//!    for the exact model `K`, the Gram-residual + Gershgorin test certifies
//!    `λ_min(A) ≥ μ > 0` (so the model has a unique solution), the residual
//!    `r = Cᵀ(f − K(C q̃ + p))` is enclosed, and
//!    `‖u* − ũ‖₂ ≤ ‖C‖₂·‖r‖₂/μ + ‖C q̃ + p − ũ‖₂` over every DOF the model
//!    determines. This is a statement about the exact solution of the model
//!    as written, independent of the engine's assembly.
//! 2. **Agreement.** The engine's assembled `K` and `f` (in global axes) must
//!    lie within `1e-10` of the scale of each entry of the re-assembled
//!    enclosures. A disagreement means the engine assembled a different
//!    model (an assembly bug, or a convention this checker reads
//!    differently), and the end-to-end status is `Failed`, whatever the bound.
//!
//! Scope: 2D and 3D frames (Euler–Bernoulli and Timoshenko), trusses and
//! cables (linear), releases, local axes (reference vector, roll angle,
//! left-handed axes), supports with springs and settlements, inclined
//! supports, rigid links, diaphragms, equal-DOF, eccentric connections and
//! linear MPCs, nodal, distributed, point and thermal member loads. Anything
//! else (shells, connectors, curved beams, warping, …) is `OutOfScope`.

mod element;
pub(crate) mod ia;
mod model2d;
mod model3d;
mod system;

use serde::{Deserialize, Serialize};

use crate::types::*;
pub use system::Agreement;
use system::{agreement, certify, EngineAssembly, Model};

/// Discrepancy the agreement check accepts, relative to the entry's scale.
pub const AGREEMENT_TOL: f64 = 1e-10;

/// A DOF named by node and local index (2D: ux, uz, θ; 3D: ux, uy, uz, rx, ry, rz).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DofRef {
    pub node_id: usize,
    pub dof: usize,
}

/// End-to-end status of a certified linear solve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EndToEnd {
    /// The displacements are within `abs_bound` (‖·‖₂ over every DOF the
    /// model determines, all DOFs in global axes) of the exact solution of the
    /// model as written, and the engine's assembly agrees with it.
    Certified {
        abs_bound: f64,
        rel_bound: f64,
        /// Certified λ_min(CᵀKC) ≥ mu > 0 for the model's exact K.
        mu: f64,
        /// Upper bound on ‖Cᵀ(f − K(C q̃ + p))‖₂.
        residual_norm: f64,
        /// Largest relative discrepancy of the engine's K / f from the re-assembly.
        k_discrepancy: f64,
        f_discrepancy: f64,
        /// DOFs nothing stiffens (rotations of truss-only or fully released
        /// nodes): the model leaves them undetermined; not covered.
        excluded_dofs: Vec<DofRef>,
    },
    /// Checked, and the engine's result or assembly does not match the model.
    Failed { reason: String },
    /// The model uses something the re-assembly does not cover.
    OutOfScope { reason: String },
    /// Not attempted (e.g. the assembled-level certificate already failed).
    #[default]
    NotChecked,
}

impl EndToEnd {
    pub fn is_certified(&self) -> bool {
        matches!(self, EndToEnd::Certified { .. })
    }
    pub fn abs_bound(&self) -> Option<f64> {
        match self {
            EndToEnd::Certified { abs_bound, .. } => Some(*abs_bound),
            _ => None,
        }
    }
}

/// Error from model building: out of scope, or a model with no unique solution.
enum BuildErr {
    Scope(String),
    Fail(String),
}

fn classify(e: String) -> BuildErr {
    if e.starts_with("out of scope") || e.starts_with("unsupported") {
        BuildErr::Scope(e)
    } else {
        BuildErr::Fail(e)
    }
}

fn finish(model: Result<Model, String>, u: impl FnOnce(&Model) -> Result<Vec<f64>, String>, eng: impl FnOnce() -> EngineAssembly) -> EndToEnd {
    let model = match model.map_err(classify) {
        Ok(m) => m,
        Err(BuildErr::Scope(reason)) => return EndToEnd::OutOfScope { reason },
        Err(BuildErr::Fail(reason)) => return EndToEnd::Failed { reason },
    };
    let u = match u(&model) {
        Ok(u) => u,
        Err(reason) => return EndToEnd::Failed { reason },
    };
    let bound = match certify(&model, &u) {
        Ok(b) => b,
        Err(reason) => return EndToEnd::Failed { reason: format!("no bound against the re-assembled model: {reason}") },
    };
    let agr = match agreement(&model, &eng()) {
        Ok(a) => a,
        Err(reason) => return EndToEnd::Failed { reason },
    };
    if !(agr.k_rel <= AGREEMENT_TOL) {
        let w = agr.worst_k.map(|(a, b, e, r)| format!(" at {a:?}–{b:?}: engine {e:.6e}, model [{:.6e}, {:.6e}]", r.lo, r.hi));
        return EndToEnd::Failed {
            reason: format!(
                "the engine's stiffness disagrees with the re-assembled model (relative {:.3e}){}",
                agr.k_rel,
                w.unwrap_or_default()
            ),
        };
    }
    if !(agr.f_rel <= AGREEMENT_TOL) {
        let w = agr.worst_f.map(|(a, e, r)| format!(" at {a:?}: engine {e:.6e}, model [{:.6e}, {:.6e}]", r.lo, r.hi));
        return EndToEnd::Failed {
            reason: format!(
                "the engine's load vector disagrees with the re-assembled model (relative {:.3e}){}",
                agr.f_rel,
                w.unwrap_or_default()
            ),
        };
    }
    EndToEnd::Certified {
        abs_bound: bound.abs_bound,
        rel_bound: bound.rel_bound,
        mu: bound.mu,
        residual_norm: bound.residual_norm,
        k_discrepancy: agr.k_rel,
        f_discrepancy: agr.f_rel,
        excluded_dofs: bound.excluded.into_iter().map(|(node_id, dof)| DofRef { node_id, dof }).collect(),
    }
}

fn lower_csc_to_map(k: &crate::linalg::sparse::CscMatrix) -> std::collections::HashMap<(usize, usize), f64> {
    let mut m = std::collections::HashMap::new();
    for j in 0..k.n {
        for p in k.col_ptr[j]..k.col_ptr[j + 1] {
            let i = k.row_idx[p];
            let v = k.values[p];
            *m.entry((i, j)).or_insert(0.0) += v;
            if i != j {
                *m.entry((j, i)).or_insert(0.0) += v;
            }
        }
    }
    m
}

/// End-to-end check of `results`, claimed to solve `input` with `loads`.
pub fn end_to_end_2d(input: &SolverInput, loads: &[SolverLoad], results: &AnalysisResults) -> EndToEnd {
    let model = model2d::build(input, loads);
    finish(
        model,
        |m| {
            let disp: std::collections::HashMap<usize, &Displacement> =
                results.displacements.iter().map(|d| (d.node_id, d)).collect();
            m.keys
                .iter()
                .map(|&(node, d)| {
                    let x = disp.get(&node).ok_or_else(|| format!("no displacement returned for node {node}"))?;
                    Ok([x.ux, x.uz, x.ry][d])
                })
                .collect()
        },
        || {
            use crate::solver::assembly::assemble_load_vector_2d;
            use crate::solver::dof::DofNumbering;
            use crate::solver::sparse_assembly::assemble_stiffness_sparse_2d;
            let mut case = input.clone();
            case.loads = loads.to_vec();
            let dof_num = DofNumbering::build_2d(&case);
            let stiff = assemble_stiffness_sparse_2d(&case, &dof_num);
            let f = assemble_load_vector_2d(&case, loads, &dof_num, &stiff.inclined_transforms_2d);
            let rotations = stiff
                .inclined_transforms_2d
                .iter()
                .map(|it| (it.dofs.to_vec(), it.r.iter().map(|r| r.to_vec()).collect()))
                .collect();
            EngineAssembly { map: dof_num.map.clone(), k: lower_csc_to_map(&stiff.k_full), f, rotations }
        },
    )
}

/// End-to-end check of 3D `results`, claimed to solve `input` with `loads`.
pub fn end_to_end_3d(input: &SolverInput3D, loads: &[SolverLoad3D], results: &AnalysisResults3D) -> EndToEnd {
    let model = model3d::build(input, loads);
    finish(
        model,
        |m| {
            let disp: std::collections::HashMap<usize, &Displacement3D> =
                results.displacements.iter().map(|d| (d.node_id, d)).collect();
            m.keys
                .iter()
                .map(|&(node, d)| {
                    let x = disp.get(&node).ok_or_else(|| format!("no displacement returned for node {node}"))?;
                    Ok([x.ux, x.uy, x.uz, x.rx, x.ry, x.rz][d])
                })
                .collect()
        },
        || {
            use crate::solver::assembly::{assemble_load_vector_3d_dense, assemble_stiffness_sparse_3d};
            use crate::solver::dof::DofNumbering;
            let mut case = input.clone();
            case.loads = loads.to_vec();
            let dof_num = DofNumbering::build_3d(&case);
            let stiff = assemble_stiffness_sparse_3d(&case, &dof_num, true);
            let f = assemble_load_vector_3d_dense(&case, loads, &dof_num, &stiff.inclined_transforms);
            let rotations = stiff
                .inclined_transforms
                .iter()
                .map(|it| (it.dofs.to_vec(), it.r.iter().map(|r| r.to_vec()).collect()))
                .collect();
            let k = stiff.k_full.as_ref().map(lower_csc_to_map).unwrap_or_default();
            EngineAssembly { map: dof_num.map.clone(), k, f, rotations }
        },
    )
}
