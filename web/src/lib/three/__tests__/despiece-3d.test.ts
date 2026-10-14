/**
 * The 3D despiece: the actions the view shows (`freeBodyActions3D`) and the inspection helpers.
 * Equilibrium on real solves is in despiece-3d-equilibrium.test.ts.
 */
import { describe, expect, it } from 'vitest';
import { inspectMember3D, inspectNode3D, type DespieceBasis } from '../despiece-3d';
import { freeBodyActions3D, pointResultants } from '../../viewport3d/free-body-3d';
import type { Element, Node, Section, Load } from '../../store/model.svelte';
import type { ElementForces3D, Reaction3D } from '../../engine/types-3d';

function rel() { return { my: false, mz: false, t: false }; }
function el(id: number, i: number, j: number): Element {
  return { id, type: 'frame', nodeI: i, nodeJ: j, materialId: 1, sectionId: 1, releaseI: rel(), releaseJ: rel() } as Element;
}
function ef(id: number, n = 10): ElementForces3D {
  return { elementId: id, length: 4, nStart: n, nEnd: -n, vyStart: 5, vyEnd: -5, vzStart: 0, vzEnd: 0,
    mxStart: 0, mxEnd: 0, myStart: 0, myEnd: 0, mzStart: 8, mzEnd: -8 } as ElementForces3D;
}
const SEC = new Map<number, Section>([[1, { id: 1, a: 0.01, iz: 1e-4 } as Section]]);

function actions(o: { basis?: DespieceBasis; resultant?: boolean; showReactions?: boolean; reactions?: Reaction3D[]; loads?: Load[]; loadMode?: 'all' | 'resultant' } = {}) {
  const nodes = new Map<number, Node>([[1, { id: 1, x: 0, y: 0, z: 0 }], [2, { id: 2, x: 4, y: 0, z: 0 }]]);
  const elements = new Map<number, Element>([[1, el(1, 1, 2)]]);
  return freeBodyActions3D({
    elements, nodes, forces: [ef(1)], reactions: o.reactions ?? [], sections: SEC, project2D: false, leftHand: false,
    basis: o.basis ?? 'local', resultant: o.resultant ?? false, showReactions: o.showReactions ?? false,
    loads: o.loads, loadText: (v) => v.toFixed(1), loadMode: o.loadMode,
  });
}

describe('3D despiece actions', () => {
  it('each end shows N, Vy and Mz along the member axes, as magnitudes with a sense', () => {
    const d = actions();
    const I = d.ends.find((e) => e.end === 'I' && e.side === 'member')!;
    expect(I.actions.map((a) => a.name)).toEqual(['N', 'Vy', 'Mz']);
    for (const a of I.actions) {
      expect(a.magnitude).toBeGreaterThan(0);
      expect(Math.hypot(a.dir.x, a.dir.y, a.dir.z ?? 0)).toBeCloseTo(1);
    }
    // Tension pulls the face: at I the axial action points away from the member (−x).
    expect(I.actions[0]!.dir.x).toBeCloseTo(-1);
  });

  it('the joint gets the member\'s action reversed', () => {
    const d = actions();
    const m = d.ends.find((e) => e.end === 'J' && e.side === 'member')!;
    const n = d.ends.find((e) => e.end === 'J' && e.side === 'node')!;
    m.actions.forEach((a, k) => {
      expect(n.actions[k]!.dir.x).toBeCloseTo(-a.dir.x);
      expect(n.actions[k]!.dir.y).toBeCloseTo(-a.dir.y);
      expect(n.actions[k]!.magnitude).toBeCloseTo(a.magnitude);
    });
  });

  it('global basis gives world components; resultant gives one force and one moment', () => {
    const g = actions({ basis: 'global' }).ends.find((e) => e.end === 'I' && e.side === 'member')!;
    expect(g.actions.filter((a) => a.kind === 'force').map((a) => a.name)).toEqual(['Fx', 'Fy']);
    const r = actions({ resultant: true }).ends.find((e) => e.end === 'I' && e.side === 'member')!;
    expect(r.actions.map((a) => a.name)).toEqual(['F', 'M']);
    expect(r.actions[0]!.magnitude).toBeCloseTo(Math.hypot(10, 5));
  });

  it('reactions only when asked for, and the loads as directions with their values', () => {
    const reaction = { nodeId: 1, fx: 0, fy: 0, fz: 12, mx: 0, my: 0, mz: 0 } as Reaction3D;
    expect(actions({ reactions: [reaction] }).supports).toEqual([]);
    const s = actions({ reactions: [reaction], showReactions: true }).supports[0]!;
    expect(s.actions.map((a) => a.name)).toEqual(['Rz']);
    const loads = [
      { type: 'distributed3d', data: { id: 1, elementId: 1, qYI: 0, qYJ: 0, qZI: -6, qZJ: -6 } },
      { type: 'nodal3d', data: { id: 2, nodeId: 2, fx: 0, fy: 0, fz: -3, mx: 0, my: 0, mz: 0 } },
    ] as unknown as Load[];
    const l = actions({ loads }).loads;
    expect(l.map((x) => x.kind)).toEqual(['dist', 'nodal']);
    expect(l[0]!.label).toBe('6.0');
  });
});

describe('3D despiece loads as resultants', () => {
  it('a uniform load is one force, q·L at mid-span; a sign-reversing one is its two triangles', () => {
    expect(pointResultants(-6, -6, 0, 4)).toEqual([{ mag: -24, at: 2 }]);
    const two = pointResultants(-4, 6, 0, 3);
    expect(two).toHaveLength(2);
    expect(two[0]!.mag).toBeCloseTo(-6); expect(two[0]!.at).toBeCloseTo(1);
    expect(two[1]!.mag).toBeCloseTo(9); expect(two[1]!.at).toBeCloseTo(2);
  });

  it('the view draws a point force for the resultant and the spread load for all', () => {
    const loads = [{ type: 'distributed3d', data: { id: 1, elementId: 1, qYI: 0, qYJ: 0, qZI: -6, qZJ: -6 } }] as unknown as Load[];
    const all = actions({ loads, loadMode: 'all' }).loads;
    expect(all.map((l) => l.kind)).toEqual(['dist']);
    const res = actions({ loads, loadMode: 'resultant' }).loads;
    expect(res).toHaveLength(1);
    expect(res[0]!.kind).toBe('point');
    expect(res[0]!.label).toBe('24.0');
    expect(res[0]!.kind === 'point' && res[0]!.t).toBeCloseTo(0.5);
  });
});

describe('despiece 3D inspection helpers', () => {
  const nodes = new Map<number, Node>([
    [1, { id: 1, x: 0, y: 0, z: 0 }], [2, { id: 2, x: 4, y: 0, z: 0 }], [3, { id: 3, x: 0, y: 0, z: 4 }],
  ]);
  const inspectArgs = {
    elements: [{ id: 1, nodeI: 1, nodeJ: 2 }, { id: 2, nodeI: 1, nodeJ: 3 }],
    getNode: (id: number) => { const n = nodes.get(id); return n ? { x: n.x, y: n.y, z: n.z ?? 0 } : undefined; },
    getForces: (id: number) => ef(id),
    basis: 'local' as DespieceBasis,
    leftHand: false,
  };

  it('inspectMember3D returns both ends', () => {
    const r = inspectMember3D(inspectArgs, 1)!;
    expect(r.ends.map(e => e.end)).toEqual(['I', 'J']);
    expect(r.ends[0].components.length).toBeGreaterThan(0);
  });

  it('inspectNode3D returns every connected member-end action', () => {
    const r = inspectNode3D(inspectArgs, 1);
    expect(r.actions.length).toBe(2); // E1·I and E2·I converge at node 1
    expect(new Set(r.actions.map(a => `${a.elementId}${a.end}`))).toEqual(new Set(['1I', '2I']));
  });
});
