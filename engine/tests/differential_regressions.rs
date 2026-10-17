//! Regressions found by differential testing against Frame3DD, PyNite,
//! Mystran, Kratos and CalculiX (tools/differential/). Each case is the
//! shrunk model the harness dumped, checked against a closed form or a
//! property every reference program agrees on.

use dedaliano_engine::solver::linear::solve_3d;
use dedaliano_engine::types::{DiagnosticCode, SolverInput3D};
use serde_json::json;

fn input_3d(v: serde_json::Value) -> SolverInput3D {
    serde_json::from_value(v).unwrap()
}

fn warns_singular(input: &SolverInput3D) -> bool {
    match solve_3d(input) {
        Err(_) => true, // refusing to solve is also a correct answer
        Ok(res) => res
            .structured_diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::StiffnessNotPositiveDefinite),
    }
}

/// A member hinged (My released) at the tip of a skew member and free at its
/// other end spins about the hinge: the stiffness is singular (scaled
/// condition number 9e16 in PyNite and OpenSees). Rounding left the
/// mechanism's Cholesky pivot at ~1e-14 of its diagonal, above the 64 eps
/// singularity threshold, so stabileo solved it with no warning and returned
/// arbitrary displacements (found by tools/differential, PyNite, rich3d-184).
/// Over 993 random frames the singular models' pivots reached 5e-12 of their
/// diagonal while no regular model's (condition number < 1e10) went below
/// 2.4e-5.
#[test]
fn dangling_hinged_member_is_flagged_singular() {
    let input = input_3d(json!({
        "nodes": {"2": {"id": 2, "x": 2.702, "y": 7.419, "z": 6.804},
                  "3": {"id": 3, "x": 4.262, "y": 1.662, "z": 3.603},
                  "4": {"id": 4, "x": 1.046, "y": 2.241, "z": 1.38}},
        "materials": {"2": {"id": 2, "e": 30000.0, "nu": 0.15266999300297987}},
        "sections": {"2": {"id": 2, "a": 0.12183545760538358, "iy": 0.0025300424619031973,
                           "iz": 0.0006047898489029847, "j": 0.0018375321524981133},
                     "3": {"id": 3, "a": 0.3560112246033974, "iy": 0.017583013735482916,
                           "iz": 0.006344522712223157, "j": 0.021401535363011273}},
        "elements": {"2": {"id": 2, "type": "frame", "nodeI": 2, "nodeJ": 3, "materialId": 2,
                           "sectionId": 3, "rollAngle": -170.94014954363033},
                     "4": {"id": 4, "type": "frame", "nodeI": 4, "nodeJ": 3, "materialId": 2,
                           "sectionId": 2, "rollAngle": -116.35070278367762, "releaseMyEnd": true}},
        "supports": {"2": {"nodeId": 2, "rx": false, "ry": true, "rz": false, "rrx": true,
                           "rry": false, "rrz": true, "kx": 991035.6463398683,
                           "kz": 935700.4969410569, "kry": 415157.6074546659}},
        "loads": [{"type": "nodal", "data": {"nodeId": 3, "fx": 10.0, "fy": -5.0, "fz": 3.0,
                                             "mx": 0.0, "my": 0.0, "mz": 0.0}}]
    }));
    assert!(warns_singular(&input), "singular stiffness solved without a warning");
}
