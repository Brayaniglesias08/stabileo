//! Shared parameter validation for dynamic-analysis entry points.

use crate::types::{SolverInput, SolverInput3D};
use std::collections::{HashMap, HashSet};

/// Material ids referenced by any 2D element — the only ids whose density
/// actually contributes mass to the assembled model.
pub(crate) fn referenced_material_ids_2d(input: &SolverInput) -> HashSet<usize> {
    input.elements.values().map(|e| e.material_id).collect()
}

/// Material ids referenced by any 3D element, including shell families
/// (plates/quads/quad9s/solid-shells/curved-shells), which carry mass too.
pub(crate) fn referenced_material_ids_3d(input: &SolverInput3D) -> HashSet<usize> {
    let mut ids: HashSet<usize> = input.elements.values().map(|e| e.material_id).collect();
    ids.extend(input.plates.values().map(|p| p.material_id));
    ids.extend(input.quads.values().map(|q| q.material_id));
    ids.extend(input.quad9s.values().map(|q| q.material_id));
    ids.extend(input.solid_shells.values().map(|s| s.material_id));
    ids.extend(input.curved_shells.values().map(|c| c.material_id));
    ids.extend(input.curved_beams.iter().map(|c| c.material_id));
    ids
}

/// Densities must be finite and >= 0 for every entry present (missing entries
/// for individual materials are legitimate — massless members are a valid
/// modeling choice and default to 0). At least one material that is actually
/// *referenced by an element in the model* must have a density > 0, otherwise
/// the assembled mass matrix is all-zero and dynamic analysis is meaningless
/// (mass assembly looks up `densities.get(&material_id.to_string())`, so a
/// density keyed to a material no element uses silently contributes nothing).
/// `other_mass`: the model carries positive nodal, line or area mass, so it
/// has a mass matrix without any density.
fn validate_densities(
    densities: &HashMap<String, f64>,
    referenced_material_ids: &HashSet<usize>,
    other_mass: bool,
) -> Result<(), String> {
    for (id, &rho) in densities {
        if !rho.is_finite() || rho < 0.0 {
            return Err(format!(
                "Density for material {}: must be finite and >= 0 (got {})", id, rho
            ));
        }
    }
    let any_positive = referenced_material_ids.iter().any(|id| {
        densities.get(&id.to_string()).copied().unwrap_or(0.0) > 0.0
    });
    if !any_positive && !other_mass {
        return Err(
            "No positive density on any material used by elements and no nodal, line or area \
             mass — dynamic analysis needs mass (check density map keys match material ids)".to_string(),
        );
    }
    Ok(())
}

fn check_mass_value(what: &str, v: f64) -> Result<(), String> {
    if !v.is_finite() || v < 0.0 {
        return Err(format!("{what}: must be finite and >= 0 (got {v})"));
    }
    Ok(())
}

/// Nodal and line masses of a 2D model: on existing nodes and members,
/// finite and non-negative. Returns whether any of them is positive.
fn validate_masses_2d(input: &SolverInput) -> Result<bool, String> {
    let nodes: HashSet<usize> = input.nodes.values().map(|n| n.id).collect();
    let elems: HashMap<usize, &crate::types::SolverElement> = input.elements.values().map(|e| (e.id, e)).collect();
    let mut any = false;
    for nm in &input.masses.nodal {
        if !nodes.contains(&nm.node_id) {
            return Err(format!("Nodal mass on node {}, which does not exist", nm.node_id));
        }
        let what = format!("Nodal mass on node {}", nm.node_id);
        for v in [nm.mass, nm.mx(), nm.mz(), nm.rotary_inertia] {
            check_mass_value(&what, v)?;
        }
        any |= nm.mx() > 0.0 || nm.mz() > 0.0 || nm.rotary_inertia > 0.0;
    }
    for lm in &input.masses.line {
        if !elems.contains_key(&lm.element_id) {
            return Err(format!("Line mass on element {}, which does not exist", lm.element_id));
        }
        check_mass_value(&format!("Line mass on element {}", lm.element_id), lm.mass_per_length)?;
        any |= lm.mass_per_length > 0.0;
    }
    Ok(any)
}

/// Nodal, line and area masses of a 3D model (see `validate_masses_2d`).
/// An area mass names a plate, quad, quad9 or curved shell with a positive
/// thickness (it is spread over that thickness).
fn validate_masses_3d(input: &SolverInput3D) -> Result<bool, String> {
    let nodes: HashSet<usize> = input.nodes.values().map(|n| n.id).collect();
    let elems: HashSet<usize> = input.elements.values().map(|e| e.id).collect();
    let mut any = false;
    for nm in &input.masses.nodal {
        if !nodes.contains(&nm.node_id) {
            return Err(format!("Nodal mass on node {}, which does not exist", nm.node_id));
        }
        let what = format!("Nodal mass on node {}", nm.node_id);
        let t = nm.translational();
        for v in [nm.mass, t[0], t[1], t[2], nm.ixx, nm.iyy, nm.izz] {
            check_mass_value(&what, v)?;
        }
        any |= t.iter().chain([nm.ixx, nm.iyy, nm.izz].iter()).any(|&v| v > 0.0);
    }
    for lm in &input.masses.line {
        if !elems.contains(&lm.element_id) {
            return Err(format!(
                "Line mass on element {}, which is not a member (curved beams take their mass from density)",
                lm.element_id
            ));
        }
        check_mass_value(&format!("Line mass on element {}", lm.element_id), lm.mass_per_length)?;
        any |= lm.mass_per_length > 0.0;
    }
    for am in &input.masses.area {
        let thickness = match am.kind.as_str() {
            "quad" => input.quads.values().find(|q| q.id == am.element_id).map(|q| q.thickness),
            "plate" => input.plates.values().find(|q| q.id == am.element_id).map(|q| q.thickness),
            "quad9" => input.quad9s.values().find(|q| q.id == am.element_id).map(|q| q.thickness),
            "curvedShell" => input.curved_shells.values().find(|q| q.id == am.element_id).map(|q| q.thickness),
            other => return Err(format!(
                "Area mass on element {}: unknown kind \"{other}\" (quad, plate, quad9 or curvedShell)",
                am.element_id
            )),
        };
        let Some(t) = thickness else {
            return Err(format!("Area mass on {} {}, which does not exist", am.kind, am.element_id));
        };
        if !(t > 0.0) {
            return Err(format!("Area mass on {} {}: the shell has no thickness to carry it", am.kind, am.element_id));
        }
        check_mass_value(&format!("Area mass on {} {}", am.kind, am.element_id), am.mass_per_area)?;
        any |= am.mass_per_area > 0.0;
    }
    Ok(any)
}

/// The mass of a 2D model: densities (see `validate_densities`) and nodal and
/// line masses. Some mass must be positive.
pub(crate) fn validate_mass_2d(input: &SolverInput, densities: &HashMap<String, f64>) -> Result<(), String> {
    let other = validate_masses_2d(input)?;
    validate_densities(densities, &referenced_material_ids_2d(input), other)
}

/// The mass of a 3D model: densities and nodal, line and area masses.
pub(crate) fn validate_mass_3d(input: &SolverInput3D, densities: &HashMap<String, f64>) -> Result<(), String> {
    let other = validate_masses_3d(input)?;
    validate_densities(densities, &referenced_material_ids_3d(input), other)
}

pub(crate) fn validate_time_params(time_step: f64, n_steps: usize) -> Result<(), String> {
    if !time_step.is_finite() || time_step <= 0.0 {
        return Err(format!("time_step must be finite and > 0 (got {})", time_step));
    }
    if n_steps == 0 {
        return Err("n_steps must be >= 1".to_string());
    }
    if n_steps > MAX_TIME_STEPS {
        return Err(format!("n_steps must be <= {} (got {})", MAX_TIME_STEPS, n_steps));
    }
    Ok(())
}

/// Most time steps one time-history run may take. The count comes from the
/// model and sizes the stored response history (steps × DOFs).
pub const MAX_TIME_STEPS: usize = 200_000;
