//! What the end-to-end certificate says about models where the engine's
//! answer is not the model's (docs/verification/CERTIFY.md, "End-to-end
//! certificates", findings). Each test pins the current behaviour.

use dedaliano_engine::certify::reassembly::EndToEnd;
use dedaliano_engine::certify::{certify_linear_2d_full, certify_linear_3d_full};
use dedaliano_engine::types::*;

fn input_3d(json: &str) -> SolverInput3D {
    serde_json::from_str(json).unwrap()
}

/// Two columns fixed at the base, tops tied by a constraint in the vertical
/// XZ plane; a horizontal load at the slave top.
fn xz_pair(constraint: &str) -> SolverInput3D {
    input_3d(&format!(
        r#"{{
      "nodes": {{"1": {{"id":1,"x":0,"y":0,"z":0}}, "2": {{"id":2,"x":0,"y":0,"z":3}},
                 "3": {{"id":3,"x":4,"y":0,"z":0}}, "4": {{"id":4,"x":4,"y":0,"z":3}}}},
      "materials": {{"1": {{"id":1,"e":200000,"nu":0.3}}}},
      "sections": {{"1": {{"id":1,"a":0.01,"iy":0.0001,"iz":0.0001,"j":0.00005}}}},
      "elements": {{
        "1": {{"id":1,"type":"frame","nodeI":1,"nodeJ":2,"materialId":1,"sectionId":1}},
        "2": {{"id":2,"type":"frame","nodeI":3,"nodeJ":4,"materialId":1,"sectionId":1}}
      }},
      "supports": {{
        "1": {{"nodeId":1,"rx":true,"ry":true,"rz":true,"rrx":true,"rry":true,"rrz":true}},
        "2": {{"nodeId":3,"rx":true,"ry":true,"rz":true,"rrx":true,"rry":true,"rrz":true}}
      }},
      "loads": [{{"type":"nodal","data":{{"nodeId":4,"fx":10,"fy":0,"fz":-5,"mx":0,"my":0,"mz":0}}}}],
      "constraints": [{constraint}]
    }}"#
    ))
}

/// A diaphragm in the XZ plane ties the slave's (ux, uz) to the master's
/// translation and its rotation θy. Rigid-body kinematics about +y give
/// u_x = u_x,m + Δz·θy and u_z = u_z,m − Δx·θy (θ × r); the engine writes
/// u_x,m − Δz·θy and u_z,m + Δx·θy, the rotation with the wrong sign (the XY
/// and YZ planes are right). The end-to-end check rejects the result: the
/// returned displacements violate the model's constraint.
#[test]
fn diaphragm_xz_rotation_sign() {
    let input = xz_pair(r#"{"type":"diaphragm","masterNode":2,"slaveNodes":[4],"plane":"XZ"}"#);
    let out = certify_linear_3d_full(&input).unwrap();
    assert!(out.certificate.is_certified());
    match &out.end_to_end {
        EndToEnd::Failed { reason } => assert!(reason.contains("violate the model's supports or constraints"), "{reason}"),
        other => panic!("{other:?}"),
    }
    // The same tie written as the rigid link's rigid-body kinematics (which
    // the engine gets right) agrees end to end.
    let link = xz_pair(r#"{"type":"rigidLink","masterNode":2,"slaveNode":4,"dofs":[0,2]}"#);
    let out = certify_linear_3d_full(&link).unwrap();
    assert!(out.end_to_end.is_certified(), "{:?}", out.end_to_end);
}

fn input_2d(json: &str) -> SolverInput {
    serde_json::from_str(json).unwrap()
}

/// A nodal moment on a node where every member is hinged: nothing resists it,
/// so the model has no solution. The 2D engine holds such a rotation with an
/// artificial spring (1e-10 × the largest diagonal) and refuses the result
/// only when the rotation exceeds 100 rad; a stiff model (here the spring is
/// 0.24 kN·m/rad, the rotation 21 rad) passes, and the assembled-level
/// certificate certifies the regularised system: the moment disappears into
/// the spring and the reactions do not balance it. (3D refuses the load:
/// `refuse_loads_on_orphan_dofs_3d`.) The end-to-end check says the model has
/// no solution.
#[test]
fn moment_on_fully_hinged_node_2d() {
    let input = input_2d(
        r#"{
      "nodes": {"1": {"id":1,"x":0,"z":0}, "2": {"id":2,"x":1,"z":0}, "3": {"id":3,"x":2,"z":0}},
      "materials": {"1": {"id":1,"e":200000,"nu":0.3}},
      "sections": {"1": {"id":1,"a":1.0,"iz":1.0}},
      "elements": {
        "1": {"id":1,"type":"frame","nodeI":1,"nodeJ":2,"materialId":1,"sectionId":1,"hingeEnd":true},
        "2": {"id":2,"type":"frame","nodeI":2,"nodeJ":3,"materialId":1,"sectionId":1,"hingeStart":true}
      },
      "supports": {"1": {"id":1,"nodeId":1,"type":"fixed"}, "2": {"id":2,"nodeId":3,"type":"fixed"}},
      "loads": [{"type":"nodal","data":{"nodeId":2,"fx":0,"fz":-10,"my":5}}]
    }"#,
    );
    let out = certify_linear_2d_full(&input).unwrap();
    assert!(out.certificate.is_certified(), "{:?}", out.certificate);
    match &out.end_to_end {
        EndToEnd::Failed { reason } => assert!(reason.contains("nothing stiffens"), "{reason}"),
        other => panic!("{other:?}"),
    }
    // Without the moment the same model is certified end to end, the
    // hinged rotation excluded from the bound.
    let mut no_moment = input.clone();
    no_moment.loads = vec![SolverLoad::Nodal(SolverNodalLoad { node_id: 2, fx: 0.0, fz: -10.0, my: 0.0 })];
    let out = certify_linear_2d_full(&no_moment).unwrap();
    match &out.end_to_end {
        EndToEnd::Certified { excluded_dofs, .. } => {
            assert_eq!(excluded_dofs.len(), 1);
            assert_eq!((excluded_dofs[0].node_id, excluded_dofs[0].dof), (2, 2));
        }
        other => panic!("{other:?}"),
    }
}

/// A rigid link from a node where its only member is hinged: the linked pair
/// can rotate about the hinge freely (the truss between them does not
/// resist a rigid rotation), so the model is a mechanism. The 2D engine's
/// all-hinged artificial spring holds the rotation, and a load on the slave
/// drives it to a large, meaningless value that the assembled-level
/// certificate certifies. The end-to-end check cannot bound the model.
#[test]
fn rigid_link_hides_a_mechanism_2d() {
    let input = input_2d(
        r#"{
      "nodes": {"1": {"id":1,"x":0,"z":0}, "2": {"id":2,"x":0,"z":3}, "3": {"id":3,"x":2,"z":3}},
      "materials": {"1": {"id":1,"e":200000,"nu":0.3}},
      "sections": {"1": {"id":1,"a":0.01,"iz":0.0001}},
      "elements": {
        "1": {"id":1,"type":"frame","nodeI":1,"nodeJ":2,"materialId":1,"sectionId":1,"hingeEnd":true},
        "2": {"id":2,"type":"truss","nodeI":2,"nodeJ":3,"materialId":1,"sectionId":1}
      },
      "supports": {"1": {"id":1,"nodeId":1,"type":"fixed"}},
      "loads": [{"type":"nodal","data":{"nodeId":3,"fx":0,"fz":-10,"my":0}}],
      "constraints": [{"type":"rigidLink","masterNode":2,"slaveNode":3,"dofs":[]}]
    }"#,
    );
    let out = certify_linear_2d_full(&input).unwrap();
    eprintln!("{:?}\n{:?}", out.certificate, out.end_to_end);
    assert!(!out.end_to_end.is_certified(), "{:?}", out.end_to_end);
}
