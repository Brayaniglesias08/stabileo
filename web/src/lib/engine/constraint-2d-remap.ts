/**
 * Translate 3D-authored constraints onto the 2D solver's DOF convention.
 *
 * The app stores ONE constraints array, always authored in 3D semantics
 * (ProConstraintsTab): DOF indices 0..5 = [ux, uy, uz, rx, ry, rz] and
 * eccentric releases as a 6-bool array. The Rust 2D solver uses 3 DOFs per
 * node — [0=ux, 1=uz, 2=ry] — and hard-errors on any dof >= 3, while a 6-bool
 * releases array would be silently misread ([1]=uy interpreted as the uz
 * slot). This module is the single dimension-translation layer between the
 * stored shape and the 2D wire (mirrors how supports and loads are remapped):
 *
 *   3D dof 0 (ux) → 2D 0 (ux)        in-plane horizontal
 *   3D dof 2 (uz) → 2D 1 (uz)        in-plane vertical
 *   3D dof 4 (ry) → 2D 2 (ry)        in-plane rotation
 *   3D dofs 1/3/5 (uy/rx/rz)         out-of-plane — dropped (the 2D solve
 *                                    restrains those implicitly)
 *
 * Constraints left without any in-plane DOF are dropped entirely. A diaphragm
 * keeps what its plane ties in the X–Z plane: an XZ diaphragm stays one (a
 * rigid body in the model plane), a floor (XY) ties ux, a YZ wall ties uz.
 */

import type { Constraint3D } from './types-3d';

const DOF_3D_TO_2D: Record<number, number> = { 0: 0, 2: 1, 4: 2 };

function mapDofs(dofs: number[] | undefined): number[] | undefined {
  if (!dofs) return undefined;
  return dofs
    .map(d => DOF_3D_TO_2D[d])
    .filter((d): d is number => d !== undefined);
}

/**
 * Returns the 2D-wire shape of the given constraints. The result is what both
 * the 2D connectivity preflight and serializeInput2D must consume, so the
 * preflight only credits constraints that actually reach the solver.
 */
export function constraintsTo2D(constraints: Constraint3D[] | undefined): Constraint3D[] {
  if (!constraints || constraints.length === 0) return [];
  const out: Constraint3D[] = [];
  for (const c of constraints) {
    switch (c.type) {
      case 'rigidLink': {
        // Empty dofs = solver default (all in-plane DOFs) — keep empty.
        const dofs = mapDofs(c.dofs);
        if (c.dofs && c.dofs.length > 0 && (!dofs || dofs.length === 0)) break; // only out-of-plane DOFs
        out.push({ ...c, dofs });
        break;
      }
      case 'equalDOF': {
        const dofs = mapDofs(c.dofs) ?? [];
        if (dofs.length === 0) break;
        out.push({ ...c, dofs });
        break;
      }
      case 'linearMPC': {
        // An equation involving an out-of-plane DOF has no 2D meaning — drop whole.
        if (c.terms.some(t => DOF_3D_TO_2D[t.dof] === undefined)) break;
        out.push({ ...c, terms: c.terms.map(t => ({ ...t, dof: DOF_3D_TO_2D[t.dof] })) });
        break;
      }
      case 'eccentricConnection': {
        // 2D solver kinematics read offset_x (horizontal) and offset_y
        // (VERTICAL — its dy slot); the stored 3D shape keeps vertical in
        // offsetZ. The out-of-plane offsetY is dropped, like uy loads.
        out.push({
          ...c,
          offsetX: c.offsetX,
          offsetY: c.offsetZ,
          offsetZ: 0,
          releases: [
            c.releases?.[0] ?? false, // ux
            c.releases?.[2] ?? false, // uz → 2D slot 1
            c.releases?.[4] ?? false, // ry → 2D slot 2
          ],
        });
        break;
      }
      case 'diaphragm': {
        // A diaphragm is a rigid body in ITS plane; the 2D model is the X–Z
        // plane, and a 2D diaphragm is a rigid body in that plane (ux, uz, ry
        // all tied). Only an XZ diaphragm is that. A floor (XY, the default)
        // meets the frame along X: what it ties in 2D is ux alone. Passed
        // through verbatim it made the floor beams rigid in vertical bending
        // too (uz_slave = uz_master + dx·θ, θ_slave = θ_master). A YZ wall
        // meets the frame along Z: uz alone.
        const plane = (c.plane ?? 'XY').toUpperCase();
        if (plane === 'XZ') { out.push(c); break; }
        const dof = plane === 'YZ' ? 1 : 0;
        for (const s of c.slaveNodes) {
          if (s !== c.masterNode) out.push({ type: 'equalDOF', masterNode: c.masterNode, slaveNode: s, dofs: [dof] });
        }
        break;
      }
    }
  }
  return out;
}
