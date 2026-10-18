//! Automatic convergence check: re-solve on uniformly refined meshes (h → h/2 → h/4) and see
//! how the governing quantities move.
//!
//! For each quantity q₀ (the model as drawn), q₁, q₂ the observed order is
//! p = log₂((q₁ − q₀)/(q₂ − q₁)) and the Richardson extrapolation q∞ = q₂ + (q₂ − q₁)/(2ᵖ − 1);
//! the estimated discretization error of the model as drawn is |q₀ − q∞| / |q∞|. That holds only
//! in the asymptotic range, so the result says when the sequence is not monotone (oscillating),
//! when it is not converging at all (a singularity: point load, re-entrant corner — peak
//! stresses there grow without bound under refinement), and when the order is implausible.
//! With a single refinement (large models) the order is assumed, p = 1, which over- rather than
//! under-states the error of quantities that converge faster.
//!
//! Governing quantities: the largest translation of any node; the largest principal bending
//! moment and the largest surface von Mises stress at element centres (shells); the resultant
//! reaction of each group of connected supported nodes (a line support's nodal reactions depend
//! on the mesh, their sum does not); the largest end moment of any frame member.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::refine::{check_refinable, refine_uniform, GeometryMode};
use super::{estimate_3d, DEFAULT_THRESHOLD};
use crate::types::*;

fn default_levels() -> usize { 2 }
fn default_max_dofs() -> usize { 250_000 }
fn default_threshold() -> f64 { DEFAULT_THRESHOLD }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvergenceOptions {
    /// Refinement levels beyond the model as drawn (1 or 2).
    #[serde(default = "default_levels")]
    pub levels: usize,
    #[serde(default)]
    pub geometry: GeometryMode,
    /// Refinements whose model would exceed this many DOFs are not solved.
    #[serde(default = "default_max_dofs")]
    pub max_dofs: usize,
    #[serde(default = "default_threshold")]
    pub threshold: f64,
}

impl Default for ConvergenceOptions {
    fn default() -> Self {
        ConvergenceOptions { levels: 2, geometry: GeometryMode::Smooth, max_dofs: default_max_dofs(), threshold: DEFAULT_THRESHOLD }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshLevel {
    pub nodes: usize,
    pub shell_elements: usize,
    /// Recovery-based relative energy-norm error estimate on this mesh.
    pub energy_estimate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuantityConvergence {
    /// `maxDisplacement`, `maxMoment`, `maxStress`, `reaction`, `maxFrameMoment`.
    pub quantity: String,
    /// For `reaction`: the smallest node id of the support group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<usize>,
    /// One value per mesh, coarsest (the model as drawn) first.
    pub values: Vec<f64>,
    /// |q₁ − q₀| / |q₁|.
    pub relative_change: f64,
    pub observed_order: Option<f64>,
    pub extrapolated: Option<f64>,
    /// Estimated relative discretization error of the model as drawn, |q₀ − q∞| / |q∞|.
    pub estimated_error: Option<f64>,
    /// `converging`, `converged`, `assumedOrder`, `irregularOrder`, `oscillating`,
    /// `notConverging`, `zero`.
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvergenceStudy {
    /// Always false: an estimate, not a bound.
    pub certified: bool,
    pub geometry: GeometryMode,
    pub levels: Vec<MeshLevel>,
    pub quantities: Vec<QuantityConvergence>,
    /// Largest estimated error (or relative change, where no extrapolation exists) over the
    /// governing quantities.
    pub max_estimated_error: Option<f64>,
    pub threshold: f64,
    pub exceeds_threshold: bool,
    /// `springsNotDistributed`, `inclinedSupportsDropped`, `singleRefinement`,
    /// `dofLimit`, `centroidStresses`.
    pub caveats: Vec<String>,
}

/// Governing quantities of one solution, keyed for matching across meshes.
pub(crate) fn governing(input: &SolverInput3D, r: &AnalysisResults3D, original_max_node: usize) -> BTreeMap<(String, usize), f64> {
    let mut q = BTreeMap::new();
    let umax = r.displacements.iter().map(|d| (d.ux * d.ux + d.uy * d.uy + d.uz * d.uz).sqrt()).fold(0.0, f64::max);
    q.insert(("maxDisplacement".to_string(), 0), umax);

    let thick: HashMap<usize, f64> = input.quads.values().map(|e| (e.id, e.thickness))
        .chain(input.quad9s.values().map(|e| (e.id, e.thickness)))
        .chain(input.curved_shells.values().map(|e| (e.id, e.thickness)))
        .chain(input.plates.values().map(|e| (e.id, e.thickness)))
        .collect();
    let (mut mmax, mut smax) = (0.0_f64, 0.0_f64);
    let mut visit = |id: usize, s: [f64; 3], m: [f64; 3]| {
        let pm = 0.5 * (m[0] + m[1]);
        let rm = (0.25 * (m[0] - m[1]).powi(2) + m[2] * m[2]).sqrt();
        mmax = mmax.max(pm.abs() + rm);
        let t = thick.get(&id).copied().unwrap_or(0.0);
        let k = if t > 0.0 { 6.0 / (t * t) } else { 0.0 };
        for sign in [-1.0, 1.0] {
            let (a, b, c) = (s[0] + sign * k * m[0], s[1] + sign * k * m[1], s[2] + sign * k * m[2]);
            smax = smax.max((a * a - a * b + b * b + 3.0 * c * c).max(0.0).sqrt());
        }
    };
    for p in &r.plate_stresses {
        visit(p.element_id, [p.sigma_xx, p.sigma_yy, p.tau_xy], [p.mx, p.my, p.mxy]);
    }
    for p in &r.quad_stresses {
        visit(p.element_id, [p.sigma_xx, p.sigma_yy, p.tau_xy], [p.mx, p.my, p.mxy]);
    }
    if mmax > 0.0 {
        q.insert(("maxMoment".to_string(), 0), mmax);
    }
    if smax > 0.0 {
        q.insert(("maxStress".to_string(), 0), smax);
    }

    // Reaction groups: supported nodes connected by element edges.
    let supported: std::collections::HashSet<usize> = input.supports.values().map(|s| s.node_id).collect();
    let mut parent: HashMap<usize, usize> = supported.iter().map(|&n| (n, n)).collect();
    fn find(p: &mut HashMap<usize, usize>, x: usize) -> usize {
        let mut r = x;
        while p[&r] != r { r = p[&r]; }
        let mut c = x;
        while p[&c] != r { let n = p[&c]; p.insert(c, r); c = n; }
        r
    }
    let mut link = |a: usize, b: usize, p: &mut HashMap<usize, usize>| {
        if p.contains_key(&a) && p.contains_key(&b) {
            let (ra, rb) = (find(p, a), find(p, b));
            if ra != rb { p.insert(ra.max(rb), ra.min(rb)); }
        }
    };
    let ring = |nodes: &[usize], p: &mut HashMap<usize, usize>, link: &mut dyn FnMut(usize, usize, &mut HashMap<usize, usize>)| {
        for i in 0..nodes.len() { link(nodes[i], nodes[(i + 1) % nodes.len()], p); }
    };
    for e in input.plates.values() { ring(&e.nodes, &mut parent, &mut link); }
    for e in input.quads.values() { ring(&e.nodes, &mut parent, &mut link); }
    for e in input.curved_shells.values() { ring(&e.nodes, &mut parent, &mut link); }
    for e in input.quad9s.values() {
        let n = e.nodes;
        ring(&[n[0], n[4], n[1], n[5], n[2], n[6], n[3], n[7]], &mut parent, &mut link);
    }
    for e in input.solid_shells.values() {
        let n = e.nodes;
        ring(&n[..4], &mut parent, &mut link);
        ring(&n[4..], &mut parent, &mut link);
        for k in 0..4 { link(n[k], n[k + 4], &mut parent); }
    }
    for e in input.elements.values() { link(e.node_i, e.node_j, &mut parent); }
    let mut groups: HashMap<usize, ([f64; 3], usize)> = HashMap::new();
    for rx in &r.reactions {
        if !parent.contains_key(&rx.node_id) { continue; }
        let root = find(&mut parent, rx.node_id);
        let g = groups.entry(root).or_insert(([0.0; 3], usize::MAX));
        g.0[0] += rx.fx;
        g.0[1] += rx.fy;
        g.0[2] += rx.fz;
        if rx.node_id <= original_max_node { g.1 = g.1.min(rx.node_id); }
    }
    for (_, (f, key)) in groups {
        if key != usize::MAX {
            *q.entry(("reaction".to_string(), key)).or_insert(0.0) += (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
        }
    }

    if !r.element_forces.is_empty() {
        let fm = r.element_forces.iter()
            .flat_map(|f| [f.my_start, f.my_end, f.mz_start, f.mz_end])
            .fold(0.0_f64, |a, v| a.max(v.abs()));
        q.insert(("maxFrameMoment".to_string(), 0), fm);
    }
    q
}

/// Richardson analysis of one quantity over the meshes.
pub fn richardson(values: &[f64]) -> (Option<f64>, Option<f64>, Option<f64>, &'static str) {
    let scale = values.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
    if !(scale > 1e-300) {
        return (None, None, None, "zero");
    }
    let rel = |x: f64, y: f64| (x - y).abs() / y.abs().max(1e-300);
    match values {
        [q0, q1] => {
            let p = 1.0;
            let ext = q1 + (q1 - q0) / (2f64.powf(p) - 1.0);
            (None, Some(ext), Some(rel(*q0, ext)), "assumedOrder")
        }
        [q0, q1, q2, ..] => {
            let (d1, d2) = (q1 - q0, q2 - q1);
            let tiny = 1e-10 * scale;
            if d1.abs() <= tiny && d2.abs() <= tiny {
                return (None, Some(*q2), Some(rel(*q0, *q2)), "converged");
            }
            if d1 * d2 < 0.0 {
                return (None, None, None, "oscillating");
            }
            if d2.abs() >= d1.abs() {
                return (None, None, None, "notConverging");
            }
            let p = (d1 / d2).log2();
            let ext = q2 + d2 / (2f64.powf(p) - 1.0);
            let status = if (0.5..=6.0).contains(&p) { "converging" } else { "irregularOrder" };
            (Some(p), Some(ext), Some(rel(*q0, ext)), status)
        }
        _ => (None, None, None, "zero"),
    }
}

fn shell_count(i: &SolverInput3D) -> usize {
    i.plates.len() + i.quads.len() + i.quad9s.len() + i.curved_shells.len() + i.solid_shells.len()
}

fn dofs(i: &SolverInput3D) -> usize {
    6 * i.nodes.len()
}

/// Run the convergence study on a 3D linear static model.
pub fn convergence_study_3d(input: &SolverInput3D, opts: &ConvergenceOptions) -> Result<ConvergenceStudy, String> {
    check_refinable(input)?;
    let levels_wanted = opts.levels.clamp(1, 2);
    let original_max_node = input.nodes.values().map(|n| n.id).max().unwrap_or(0);
    let mut caveats: Vec<String> = vec!["centroidStresses".into()];
    let mut models = vec![input.clone()];
    for _ in 0..levels_wanted {
        let refined = refine_uniform(models.last().unwrap(), opts.geometry)?;
        if dofs(&refined.input) > opts.max_dofs {
            caveats.push("dofLimit".into());
            break;
        }
        for c in refined.caveats {
            if !caveats.contains(&c) { caveats.push(c); }
        }
        models.push(refined.input);
    }
    if models.len() < 2 {
        return Err("tooLarge".into());
    }
    if models.len() == 2 {
        caveats.push("singleRefinement".into());
    }
    let mut levels = Vec::new();
    let mut per_level = Vec::new();
    for m in &models {
        let r = crate::solver::linear::solve_3d(m)?;
        let est = estimate_3d(m, &r).ok().and_then(|e| e.relative_error);
        levels.push(MeshLevel { nodes: m.nodes.len(), shell_elements: shell_count(m), energy_estimate: est });
        per_level.push(governing(m, &r, original_max_node));
    }
    let mut quantities = Vec::new();
    for (k, &v0) in &per_level[0] {
        let values: Vec<f64> = std::iter::once(v0)
            .chain(per_level[1..].iter().map(|g| g.get(k).copied().unwrap_or(f64::NAN)))
            .collect();
        if values.iter().any(|v| !v.is_finite()) { continue; }
        let (p, ext, err, status) = richardson(&values);
        quantities.push(QuantityConvergence {
            quantity: k.0.clone(),
            node_id: (k.0 == "reaction").then_some(k.1),
            relative_change: (values[1] - values[0]).abs() / values[1].abs().max(1e-300),
            values,
            observed_order: p,
            extrapolated: ext,
            estimated_error: err,
            status: status.to_string(),
        });
    }
    let max_estimated_error = quantities.iter()
        .filter(|q| q.status != "zero")
        .map(|q| q.estimated_error.unwrap_or(q.relative_change))
        .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v))));
    let exceeds_threshold = max_estimated_error.is_some_and(|v| v > opts.threshold)
        || quantities.iter().any(|q| q.status == "notConverging");
    Ok(ConvergenceStudy {
        certified: false,
        geometry: opts.geometry,
        levels,
        quantities,
        max_estimated_error,
        threshold: opts.threshold,
        exceeds_threshold,
        caveats,
    })
}
