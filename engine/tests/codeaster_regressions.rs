//! Regressions found by differential testing against Code_Aster
//! (tools/differential/codeaster/). Each case is a shrunk model the harness
//! flagged, checked against a property Code_Aster and the closed form agree on.

use dedaliano_engine::solver::linear::solve_3d;
use dedaliano_engine::types::{DiagnosticCode, SolverInput3D};
use serde_json::json;

fn warns_singular(input: &SolverInput3D) -> bool {
    match solve_3d(input) {
        Err(_) => true, // refusing to solve is also a correct answer
        Ok(res) => res
            .structured_diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::StiffnessNotPositiveDefinite),
    }
}

/// Member 1 hangs from the fixed node 1 with its local-z moment released
/// there and nothing at its free end: it spins about the hinge, so the
/// stiffness is singular (Code_Aster: FACTOR_11, null pivot on node 2).
/// The load sits on another member and does not excite the mechanism, so
/// the solve produced plausible small displacements with no warning:
/// the mechanism's Cholesky pivot was 1.6e-13 of its diagonal, above the
/// old 64 eps threshold (found by tools/differential/codeaster, rich3d-33;
/// the same threshold change fixes PyNite's rich3d-184 on #294).
#[test]
fn unloaded_hinged_cantilever_is_flagged_singular() {
    let input: SolverInput3D = serde_json::from_value(json!({
        "nodes": {"1": {"id": 1, "x": 1.338, "y": 1.866, "z": 6.726},
                  "2": {"id": 2, "x": 7.042, "y": 4.261, "z": 4.219},
                  "7": {"id": 7, "x": 2.244, "y": 2.556, "z": 6.118}},
        "materials": {"1": {"id": 1, "e": 200000.0, "nu": 0.2138625159561968},
                      "2": {"id": 2, "e": 200000.0, "nu": 0.34810212835172016}},
        "sections": {"1": {"id": 1, "a": 0.14015308429139656, "iy": 0.0017564648758115389,
                           "iz": 0.0015254875812946561, "j": 0.004977081239822353},
                     "2": {"id": 2, "a": 0.11021676640733986, "iy": 0.002145499463249692,
                           "iz": 0.0004776389764732468, "j": 0.0009229851058335072}},
        "elements": {"1": {"id": 1, "type": "frame", "nodeI": 1, "nodeJ": 2, "materialId": 2,
                           "sectionId": 2, "releaseMzStart": true},
                     "7": {"id": 7, "type": "frame", "nodeI": 7, "nodeJ": 1, "materialId": 1,
                           "sectionId": 1}},
        "supports": {"1": {"nodeId": 1, "rx": true, "ry": true, "rz": true,
                           "rrx": true, "rry": true, "rrz": true}},
        "loads": [{"type": "pointOnElement", "data": {"elementId": 7, "a": 0.2649635001931591,
                   "py": -31.615632983808585, "pz": 35.656982781343274}}]
    }))
    .unwrap();
    assert!(warns_singular(&input), "singular stiffness solved without a warning");
}
