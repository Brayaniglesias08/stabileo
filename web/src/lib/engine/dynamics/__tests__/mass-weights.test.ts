/**
 * Mass weights, after review: a floor weight on the whole model goes on every floor (it was
 * dropped), and a slab weight on the slabs, not the walls.
 */
import { describe, it, expect } from 'vitest';
import { massWeightLoads } from '../mass-weights';

/** A frame 5 × 4 m in plan, storeys of 3 m; with `shells`, a slab on top and a wall along y = 0 in the first storey. */
function frame(storeys: number, shells = false) {
  const nodes = new Map<number, { id: number; x: number; y: number; z: number }>();
  const elements = new Map<number, { id: number; nodeI: number; nodeJ: number; type: string; sectionId: number }>();
  const quads = new Map<number, { id: number; nodes: number[] }>();
  let nid = 1, eid = 1;
  const lv: number[][] = [];
  for (let s = 0; s <= storeys; s++) {
    lv.push([[0, 0], [5, 0], [5, 4], [0, 4]].map(([x, y]) => { nodes.set(nid, { id: nid, x: x!, y: y!, z: 3 * s }); return nid++; }));
  }
  const bar = (a: number, b: number) => { elements.set(eid, { id: eid, nodeI: a, nodeJ: b, type: 'frame', sectionId: 1 }); eid++; };
  for (let s = 1; s <= storeys; s++) for (let i = 0; i < 4; i++) { bar(lv[s - 1]![i]!, lv[s]![i]!); bar(lv[s]![i]!, lv[s]![(i + 1) % 4]!); }
  if (shells) {
    quads.set(1, { id: 1, nodes: [...lv[1]!] });                               // the slab, 5 × 4 m
    quads.set(2, { id: 2, nodes: [lv[0]![0]!, lv[0]![1]!, lv[1]![1]!, lv[1]![0]!] }); // the wall, 5 × 3 m
  }
  return { nodes, elements, quads, plates: new Map(), sections: new Map([[1, {}]]), groups: new Map(), loadCases: [] } as never;
}
/** The vertical load the weights stand for, kN. */
function total(loads: ReturnType<typeof massWeightLoads>, m: ReturnType<typeof frame>) {
  const model = m as unknown as { nodes: Map<number, { x: number; y: number; z: number }>; elements: Map<number, { nodeI: number; nodeJ: number }> };
  let s = 0;
  for (const l of loads) {
    const d = l.data as { elementId?: number; qZI?: number; qZJ?: number; a?: number; b?: number; fz?: number };
    if (l.type === 'distributed3d') {
      const e = model.elements.get(d.elementId!)!;
      const a = model.nodes.get(e.nodeI)!, b = model.nodes.get(e.nodeJ)!;
      const L = Math.hypot(b.x - a.x, b.y - a.y, b.z - a.z);
      s += -((d.qZI! + d.qZJ!) / 2) * ((d.b ?? L) - (d.a ?? 0));
    } else if (l.type === 'nodal3d') s += -(d.fz ?? 0);
  }
  return s;
}

describe('mass weights on a region', () => {
  it('a floor weight on the whole model goes on every floor', () => {
    const m = frame(2);
    const all = massWeightLoads(m, [{ on: 'floor', region: { kind: 'all' }, w: 1 }], false);
    // It was dropped: no target for "all".
    expect(all.length).toBeGreaterThan(0);
    // 1 kN/m² on each 5 × 4 m floor, two floors.
    expect(total(all, m)).toBeCloseTo(40, 6);
    const one = massWeightLoads(m, [{ on: 'floor', region: { kind: 'box', z: [3, 3] }, w: 1 }], false);
    expect(total(one, m)).toBeCloseTo(20, 6);
  });

  it('a slab weight goes on the slabs, not on a wall', () => {
    const m = frame(1, true);
    const loads = massWeightLoads(m, [{ on: 'slabs', region: { kind: 'all' }, w: 2 }], false);
    // The wall took it too: 2 × (20 + 15) kN, 7,14 t, for the slab's 40 kN, 4,08 t.
    expect(loads.map((l) => (l.data as { quadId: number }).quadId)).toEqual([1]);
  });
});
