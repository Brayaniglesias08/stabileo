/**
 * The selection panel's operations answer in the kind armed above them: plates give plates,
 * supports give supports, loads give loads, and a kind that is not armed is never handed back.
 */
import { describe, it, expect } from 'vitest';
import { likeSelection, likeSeedCount, shellsParallelToGlobal, focusNodeIds, shellPlane, allOfKind, selectShownByIds, LEVEL_TOL, type LikeSeeds, type SelKind } from '../select-like';
import { parallelToGlobal, loadedInCase } from '../select-ops';
import { selectByIds } from '../select-ops';

/*
 * Two slabs at z = 3 and z = 6 (quads 1 and 2, the first 0.2 thick, the second 0.25), a wall in
 * x = 0 (triangle 1), a column 1-5 and a beam 5-6, two supports and three loads.
 */
function model() {
  const nodes = new Map<number, { x: number; y: number; z: number }>([
    [1, { x: 0, y: 0, z: 0 }], [2, { x: 4, y: 0, z: 0 }],
    [5, { x: 0, y: 0, z: 3 }], [6, { x: 4, y: 0, z: 3 }], [7, { x: 4, y: 4, z: 3 }], [8, { x: 0, y: 4, z: 3 }],
    [9, { x: 0, y: 0, z: 6 }], [10, { x: 4, y: 0, z: 6 }], [11, { x: 4, y: 4, z: 6 }], [12, { x: 0, y: 4, z: 6 }],
  ]);
  const elements = new Map([
    [1, { nodeI: 1, nodeJ: 5, sectionId: 1, materialId: 1 }],
    [2, { nodeI: 5, nodeJ: 6, sectionId: 2, materialId: 1 }],
  ]);
  const quads = new Map([
    [1, { nodes: [5, 6, 7, 8], materialId: 2, thickness: 0.2 }],
    [2, { nodes: [9, 10, 11, 12], materialId: 2, thickness: 0.25 }],
  ]);
  const plates = new Map([[1, { nodes: [1, 5, 8], materialId: 3, thickness: 0.2 }]]);
  const supports = new Map([[1, { nodeId: 1, type: 'fixed' }], [2, { nodeId: 2, type: 'pinned' }], [3, { nodeId: 6, type: 'fixed' }]]);
  const loads = [
    { type: 'nodal', data: { id: 1, nodeId: 6, caseId: 1 } },
    { type: 'distributed', data: { id: 2, elementId: 2, caseId: 2 } },
    { type: 'nodal', data: { id: 3, nodeId: 7, caseId: 2 } },
  ];
  return { nodes, elements, quads, plates, supports, loads };
}
const seeds = (s: Partial<LikeSeeds>): LikeSeeds => ({ nodes: [], elements: [], shells: [], supports: [], loads: [], ...s });
const kinds = (...k: SelKind[]) => new Set<SelKind>(k);

describe('like what is selected, by kind', () => {
  it('plates: same thickness, same material, same plane, parallel, touching, same level', () => {
    const m = model();
    const one = (op: Parameters<typeof likeSelection>[3]) => [...likeSelection(m, kinds('shells'), seeds({ shells: ['q1'] }), op)[0]!.ids].sort();
    expect(one('section')).toEqual(['p1', 'q1']);
    expect(one('material')).toEqual(['q1', 'q2']);
    expect(one('plane')).toEqual(['q1']);
    expect(one('parallel')).toEqual(['q1', 'q2']);
    expect(one('connected')).toEqual(['p1', 'q1']);
    expect(one('level')).toEqual(['q1']);
  });

  it('nodes: same level, and joined by a member or a plate', () => {
    const m = model();
    const [lvl] = likeSelection(m, kinds('nodes'), seeds({ nodes: [5] }), 'level');
    expect([...lvl!.ids].sort((a, b) => (a as number) - (b as number))).toEqual([5, 6, 7, 8]);
    const [con] = likeSelection(m, kinds('nodes'), seeds({ nodes: [1] }), 'connected');
    expect([...con!.ids].sort((a, b) => (a as number) - (b as number))).toEqual([1, 5, 8]);
  });

  it('supports: same type and same level; loads: same type and same case', () => {
    const m = model();
    expect([...likeSelection(m, kinds('supports'), seeds({ supports: [1] }), 'kind')[0]!.ids]).toEqual([1, 3]);
    expect([...likeSelection(m, kinds('supports'), seeds({ supports: [1] }), 'level')[0]!.ids]).toEqual([1, 2]);
    expect([...likeSelection(m, kinds('loads'), seeds({ loads: [1] }), 'kind')[0]!.ids]).toEqual([1, 3]);
    expect([...likeSelection(m, kinds('loads'), seeds({ loads: [2] }), 'case')[0]!.ids]).toEqual([2, 3]);
  });

  it('answers only in armed kinds, and only where something of them is selected', () => {
    const m = model();
    // Members are selected, but only plates are armed: no member comes back.
    const a = likeSelection(m, kinds('shells'), seeds({ elements: [2], shells: ['q1'] }), 'material');
    expect(a.map((x) => x.kind)).toEqual(['shells']);
    // An operation a kind does not have gives no answer for it.
    expect(likeSelection(m, kinds('supports'), seeds({ supports: [1] }), 'parallel')).toEqual([]);
    expect(likeSeedCount(kinds('supports', 'loads'), seeds({ supports: [1, 2], loads: [1] }), 'case')).toBe(1);
  });
});

describe('plates parallel to a global axis or plane', () => {
  it('slabs lie in XY, the wall holds Z', () => {
    const m = model();
    expect([...shellsParallelToGlobal(m, 'XY')].sort()).toEqual(['q1', 'q2']);
    expect([...shellsParallelToGlobal(m, 'Z')]).toEqual(['p1']);
    expect(shellPlane(m, 'p1')!.n.map((v) => Math.abs(v))).toEqual([1, 0, 0]);
  });
});

describe('by id, every kind the panel arms', () => {
  it('plates are one kind: an id is looked up among triangles and quadrilaterals', () => {
    const m = model();
    const r = selectByIds(m, 'shells', '1, 2, 9');
    expect([...r.selection.shells].sort()).toEqual(['p1', 'q1', 'q2']);
    expect(r.both).toEqual([1]);
    expect(r.missing).toEqual([9]);
  });

  it('supports and loads by their own ids', () => {
    const m = model();
    expect([...selectByIds(m, 'supports', '2, 3').selection.supports!]).toEqual([2, 3]);
    const r = selectByIds(m, 'loads', '1-3, 5');
    expect([...r.selection.loads!]).toEqual([1, 2, 3]);
    expect(r.missing).toEqual([5]);
    expect(r.selection.nodes.size + r.selection.elements.size + r.selection.shells.size).toBe(0);
  });
});

describe('zoom to the selection frames every kind', () => {
  it('by the nodes each one sits on', () => {
    const m = model();
    expect([...focusNodeIds(m, { shells: ['p1'] })].sort((a, b) => a - b)).toEqual([1, 5, 8]);
    expect([...focusNodeIds(m, { supports: [3] })]).toEqual([6]);
    expect([...focusNodeIds(m, { loads: [2, 3] })].sort((a, b) => a - b)).toEqual([5, 6, 7]);
  });

  it('a load on a triangle frames the triangle, not the quadrilateral of the same number', () => {
    const m = { ...model(), loads: [{ type: 'surface3d', data: { id: 4, quadId: 1, on: 'plate' } }] };
    expect([...focusNodeIds(m, { loads: [4] })].sort((a, b) => a - b)).toEqual([1, 5, 8]);
  });
});

describe('the same plane is the same signed offset', () => {
  /** Walls at x = 5 and x = −5, slabs at z = 3 and z = −3: parallel pairs, never coplanar. */
  function mirrored() {
    const nodes = new Map<number, { x: number; y: number; z: number }>();
    const quads = new Map<number, { nodes: number[]; materialId: number; thickness: number }>();
    let n = 0;
    const quad = (id: number, ps: Array<[number, number, number]>) => {
      quads.set(id, { nodes: ps.map(([x, y, z]) => { nodes.set(++n, { x, y, z }); return n; }), materialId: 1, thickness: 0.2 });
    };
    quad(1, [[5, 0, 0], [5, 4, 0], [5, 4, 3], [5, 0, 3]]);
    quad(2, [[-5, 0, 0], [-5, 4, 0], [-5, 4, 3], [-5, 0, 3]]);
    quad(3, [[0, 0, 3], [4, 0, 3], [4, 4, 3], [0, 4, 3]]);
    quad(4, [[0, 0, -3], [4, 0, -3], [4, 4, -3], [0, 4, -3]]);
    // Two walls through the line x = 5, z = 0, leaning half a degree either way: their normals
    // fall on either side of the horizontal, which the plane's orientation flips.
    quad(5, [[5, 0, 0], [5, 4, 0], [5.03, 4, 3], [5.03, 0, 3]]);
    quad(6, [[5, 0, 0], [5, 4, 0], [4.97, 4, 3], [4.97, 0, 3]]);
    return { nodes, elements: new Map(), quads };
  }
  const plane = (m: ReturnType<typeof mirrored>, key: string) => [...likeSelection(m, kinds('shells'), seeds({ shells: [key] }), 'plane')[0]!.ids].sort();

  it('walls at x = ±5 and slabs at z = ±3 are not coplanar', () => {
    const m = mirrored();
    expect(plane(m, 'q1')).not.toContain('q2');
    expect(plane(m, 'q3')).toEqual(['q3']);
    expect(plane(m, 'q4')).toEqual(['q4']);
  });

  it('two walls on either side of vertical are one plane, whichever way their normals were flipped', () => {
    const m = mirrored();
    expect(plane(m, 'q5')).toContain('q6');
    expect(plane(m, 'q6')).toContain('q5');
  });
});

describe('one tolerance for "same level", every kind', () => {
  it('nodes, supports and plates within the storey band design groups members by', () => {
    const m = model();
    // A node 5 cm above the slab at z = 3, a support on it, a plate there lying flat.
    m.nodes.set(20, { x: 8, y: 0, z: 3.05 });
    m.nodes.set(21, { x: 12, y: 0, z: 3.05 });
    m.nodes.set(22, { x: 12, y: 4, z: 3.05 });
    m.supports.set(9, { nodeId: 20, type: 'fixed' });
    m.plates.set(9, { nodes: [20, 21, 22], materialId: 3, thickness: 0.2 });
    expect(likeSelection(m, kinds('nodes'), seeds({ nodes: [5] }), 'level')[0]!.ids).toContain(20);
    expect(likeSelection(m, kinds('supports'), seeds({ supports: [3] }), 'level')[0]!.ids).toContain(9);
    expect(likeSelection(m, kinds('shells'), seeds({ shells: ['q1'] }), 'level')[0]!.ids).toContain('p9');
    // The next storey, 3 m up, stays out.
    expect(likeSelection(m, kinds('nodes'), seeds({ nodes: [5] }), 'level')[0]!.ids).not.toContain(9);
    expect(LEVEL_TOL).toBe(0.15);
  });
});

describe('what the view hides is not taken', () => {
  /** The model as the panel reads it with member 2 hidden (`visibleModel`): its nodes stay, its loads go. */
  function shown() {
    const m = model();
    const elements = new Map([...m.elements].filter(([id]) => id !== 2));
    return { full: m, shown: { ...m, elements, loads: m.loads.filter((l) => (l.data as { elementId?: number }).elementId !== 2) } };
  }

  it('like, parallel and loaded in case read the shown model', () => {
    const { full, shown: s } = shown();
    // A second beam along X, shown, so "parallel" has something to find.
    full.elements.set(3, { nodeI: 6, nodeJ: 7, sectionId: 2, materialId: 1 });
    s.elements.set(3, { nodeI: 9, nodeJ: 10, sectionId: 2, materialId: 1 });
    full.elements.set(4, { nodeI: 9, nodeJ: 10, sectionId: 2, materialId: 1 });
    // The whole model, as the panel read it, hands back the hidden beam 2.
    expect([...likeSelection(full, kinds('elements'), seeds({ elements: [4] }), 'parallel')[0]!.ids].sort()).toEqual([2, 4]);
    expect([...likeSelection(s, kinds('elements'), seeds({ elements: [3] }), 'parallel')[0]!.ids]).not.toContain(2);
    expect([...parallelToGlobal(s.nodes, s.elements, 'X')]).not.toContain(2);
    expect([...loadedInCase(s.loads, 2).loads!]).toEqual([3]);
  });

  it('the walk with nothing selected goes through what is shown', () => {
    const { shown: s } = shown();
    expect(allOfKind(s, 'elements')).toEqual([1]);
    expect(allOfKind(s, 'shells')).toEqual(['p1', 'q1', 'q2']);
    expect(allOfKind(s, 'loads')).toEqual([1, 3]);
  });

  it('by id takes what is shown and names what is hidden', () => {
    const { full, shown: s } = shown();
    const r = selectShownByIds(full, s, 'elements', '1, 2, 9');
    expect([...r.selection.elements]).toEqual([1]);
    expect(r.hidden).toEqual([2]);
    expect(r.missing).toEqual([9]);
  });
});
