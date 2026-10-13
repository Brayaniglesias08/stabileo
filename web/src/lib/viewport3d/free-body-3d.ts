/**
 * The 3D despiece's actions, for `drawFreeBody`: the end action on every member and joint, the
 * support reactions and the applied loads, in scene space (Z up), ready to be projected.
 *
 * The end actions are `memberEndAction3D` (tested for ΣF = 0 and ΣM = 0 on real solves), split
 * along the member's own axes (N, Vy, Vz; T, My, Mz) or the world's (Fx, Fy, Fz), or composed
 * into one force and one moment. A joint's action is the member's, reversed.
 */
import { computeLocalAxes3D } from '../engine/local-axes-3d';
import { projectNodeToScene } from '../geometry/coordinate-system';
import { DESPIECE_COL, memberEndAction3D, type DespieceBasis } from '../three/despiece-3d';
import type { ElementForces3D, Reaction3D } from '../engine/types-3d';
import type { Element, Node, Section, Load } from '../store/model.svelte';
import type { FBAction, FBEnd, FBLoad, FBSupport, P3 } from '../canvas/free-body';

const EPS = 1e-3;
type V = [number, number, number];
const unit = (v: V): P3 => { const l = Math.hypot(v[0], v[1], v[2]) || 1; return { x: v[0] / l, y: v[1] / l, z: v[2] / l }; };
const dot = (a: V, b: V) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const scale = (a: V, k: number): V => [a[0] * k, a[1] * k, a[2] * k];

export function freeBodyActions3D(opts: {
  elements: Map<number, Element>;
  nodes: Map<number, Node>;
  forces: ElementForces3D[];
  reactions: Reaction3D[];
  sections?: Map<number, Section>;
  project2D: boolean;
  leftHand: boolean;
  basis: DespieceBasis;
  resultant: boolean;
  showReactions: boolean;
  loads?: Load[];
  /** Write a load intensity or force with its unit. */
  loadText: (v: number, q: 'force' | 'distributedLoad' | 'moment') => string;
  /**
   * Components below this share of the largest force (or moment) in the model are left out:
   * a 3D solve leaves hundredths of a kN where the structure carries nothing, and writing
   * "T = 0.0 kN·m" at every cut buries what it does carry. 0 shows everything (the tests).
   */
  negligible?: number;
}): {
  members: Array<{ id: number; nodeI: number; nodeJ: number; i: P3; j: P3 }>;
  nodes: (id: number) => P3 | undefined;
  ends: FBEnd[];
  supports: FBSupport[];
  loads: FBLoad[];
} {
  const scene = new Map<number, P3>();
  const at = (id: number) => {
    if (!scene.has(id)) { const n = opts.nodes.get(id); if (n) scene.set(id, projectNodeToScene(n, opts.project2D)); }
    return scene.get(id);
  };
  const forceOf = new Map(opts.forces.map((f) => [f.elementId, f]));
  const members: Array<{ id: number; nodeI: number; nodeJ: number; i: P3; j: P3 }> = [];
  const ends: FBEnd[] = [];
  const loads: FBLoad[] = [];
  const axesOf = new Map<number, { ex: V; ey: V; ez: V; len: number }>();
  let maxF = 0, maxM = 0;
  for (const f of opts.forces) {
    maxF = Math.max(maxF, Math.abs(f.nStart), Math.abs(f.nEnd), Math.abs(f.vyStart), Math.abs(f.vyEnd), Math.abs(f.vzStart), Math.abs(f.vzEnd));
    maxM = Math.max(maxM, Math.abs(f.mxStart), Math.abs(f.mxEnd), Math.abs(f.myStart), Math.abs(f.myEnd), Math.abs(f.mzStart), Math.abs(f.mzEnd));
  }
  const share = opts.negligible ?? 0;
  const floor = (kind: 'force' | 'moment') => Math.max(EPS, share * (kind === 'force' ? maxF : maxM));

  for (const [, e] of opts.elements) {
    const pI = at(e.nodeI), pJ = at(e.nodeJ);
    if (!pI || !pJ) continue;
    members.push({ id: e.id, nodeI: e.nodeI, nodeJ: e.nodeJ, i: pI, j: pJ });
    let axes;
    try {
      const localY = (!opts.project2D && e.localYx !== undefined && e.localYy !== undefined && e.localYz !== undefined)
        ? { x: e.localYx, y: e.localYy, z: e.localYz } : undefined;
      const roll = opts.project2D ? undefined : ((e.rollAngle ?? 0) + (opts.sections?.get(e.sectionId)?.rotation ?? 0));
      axes = computeLocalAxes3D({ id: 0, x: pI.x, y: pI.y, z: pI.z ?? 0 }, { id: 0, x: pJ.x, y: pJ.y, z: pJ.z ?? 0 }, localY, roll, false);
    } catch { continue; }
    const ex = axes.ex as V, ey = axes.ey as V, ez = axes.ez as V;
    axesOf.set(e.id, { ex, ey, ez, len: Math.hypot(pJ.x - pI.x, pJ.y - pI.y, (pJ.z ?? 0) - (pI.z ?? 0)) });
    const f = forceOf.get(e.id);
    if (!f) continue;
    for (const [end, nodeId, out, n, vy, vz, mx, my, mz] of [
      ['I', e.nodeI, 1, f.nStart, f.vyStart, f.vzStart, f.mxStart, f.myStart, f.mzStart],
      ['J', e.nodeJ, -1, f.nEnd, f.vyEnd, f.vzEnd, f.mxEnd, f.myEnd, f.mzEnd],
    ] as Array<['I' | 'J', number, 1 | -1, number, number, number, number, number, number]>) {
      const act = memberEndAction3D(ex, ey, ez, out, n, vy, vz, mx, my, mz);
      const F = act.force as V, M = act.moment as V;
      const comps = (sign: 1 | -1): FBAction[] => {
        const list: FBAction[] = [];
        const push = (name: string, kind: 'force' | 'moment', axis: V, comp: number, color: string) => {
          if (Math.abs(comp) > floor(kind)) list.push({ name, kind, dir: unit(scale(axis, Math.sign(comp) * sign)), magnitude: Math.abs(comp), color });
        };
        if (opts.resultant) {
          const fm = Math.hypot(...F), mm = Math.hypot(...M);
          if (fm > EPS) list.push({ name: 'F', kind: 'force', dir: unit(scale(F, sign)), magnitude: fm, color: DESPIECE_COL.axial });
          if (mm > EPS) list.push({ name: 'M', kind: 'moment', dir: unit(scale(M, sign)), magnitude: mm, color: DESPIECE_COL.moment });
          return list;
        }
        if (opts.basis === 'global') {
          push('Fx', 'force', [1, 0, 0], F[0], DESPIECE_COL.axial);
          push('Fy', 'force', [0, 1, 0], F[1], DESPIECE_COL.axial);
          push('Fz', 'force', [0, 0, 1], F[2], DESPIECE_COL.axial);
        } else {
          push('N', 'force', ex, dot(F, ex), DESPIECE_COL.axial);
          push('Vy', 'force', ey, dot(F, ey), DESPIECE_COL.shear);
          push('Vz', 'force', ez, dot(F, ez), DESPIECE_COL.shear);
        }
        push('T', 'moment', ex, dot(M, ex), DESPIECE_COL.moment);
        push('My', 'moment', ey, dot(M, ey), DESPIECE_COL.moment);
        push('Mz', 'moment', ez, dot(M, ez), DESPIECE_COL.moment);
        return list;
      };
      ends.push({ memberId: e.id, end, nodeId, side: 'member', actions: comps(1) });
      ends.push({ memberId: e.id, end, nodeId, side: 'node', actions: comps(-1) });
    }
  }

  const supports: FBSupport[] = [];
  if (opts.showReactions) {
    for (const r of opts.reactions) {
      const list: FBAction[] = [];
      const push = (name: string, kind: 'force' | 'moment', axis: V, v: number) => {
        if (Math.abs(v) > floor(kind)) list.push({ name, kind, dir: unit(scale(axis, Math.sign(v))), magnitude: Math.abs(v), color: DESPIECE_COL.reaction });
      };
      push('Rx', 'force', [1, 0, 0], r.fx); push('Ry', 'force', [0, 1, 0], r.fy); push('Rz', 'force', [0, 0, 1], r.fz);
      push('Mx', 'moment', [1, 0, 0], r.mx ?? 0); push('My', 'moment', [0, 1, 0], r.my ?? 0); push('Mz', 'moment', [0, 0, 1], r.mz ?? 0);
      if (list.length) supports.push({ nodeId: r.nodeId, actions: list });
    }
  }

  for (const ld of opts.loads ?? []) {
    if (ld.type === 'nodal3d') {
      const d = ld.data, f: V = [d.fx, d.fy, d.fz];
      if (Math.hypot(...f) > EPS) loads.push({ kind: 'nodal', nodeId: d.nodeId, dir: unit(f), label: opts.loadText(Math.hypot(...f), 'force') });
    } else if (ld.type === 'distributed3d' || ld.type === 'pointOnElement3d') {
      const ax = axesOf.get(ld.data.elementId);
      if (!ax) continue;
      // Member loads are typed along the y the user sees: negated under the left-hand convention.
      const eyUser = opts.leftHand ? scale(ax.ey, -1) : ax.ey;
      if (ld.type === 'distributed3d') {
        const d = ld.data;
        const qy = (d.qYI + d.qYJ) / 2, qz = (d.qZI + d.qZJ) / 2;
        const v: V = [eyUser[0] * qy + ax.ez[0] * qz, eyUser[1] * qy + ax.ez[1] * qz, eyUser[2] * qy + ax.ez[2] * qz];
        const q = Math.hypot(...v);
        if (q > EPS) loads.push({ kind: 'dist', memberId: ld.data.elementId, t0: (d.a ?? 0) / ax.len, t1: (d.b ?? ax.len) / ax.len, dir: unit(v), label: opts.loadText(q, 'distributedLoad') });
      } else {
        const d = ld.data;
        const v: V = (d.frame ?? 'local') === 'global'
          ? [d.px ?? 0, d.py, d.pz]
          : [ax.ex[0] * (d.px ?? 0) + eyUser[0] * d.py + ax.ez[0] * d.pz, ax.ex[1] * (d.px ?? 0) + eyUser[1] * d.py + ax.ez[1] * d.pz, ax.ex[2] * (d.px ?? 0) + eyUser[2] * d.py + ax.ez[2] * d.pz];
        if (Math.hypot(...v) > EPS) loads.push({ kind: 'point', memberId: d.elementId, t: (d.a ?? 0) / ax.len, dir: unit(v), label: opts.loadText(Math.hypot(...v), 'force') });
      }
    }
  }

  return { members, nodes: at, ends, supports, loads };
}
