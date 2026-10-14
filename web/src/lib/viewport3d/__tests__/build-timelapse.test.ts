import { describe, it, expect } from 'vitest';
import {
  buildOrder, buildSchedule, buildFront, dropMatrix, growthAt, growMatrix, landAt, landedBetween, phaseAt, piecesStarted,
  type BuildPiece, type P3,
} from '../build-timelapse';

const P = (x: number, y: number, z: number): P3 => ({ x, y, z });
const member = (key: string, a: P3, b: P3, nodes: [number, number]): BuildPiece => ({ key, kind: 'member', a, b, nodes });

// A two-storey frame along X: columns 1–2–3 and 4–5–6, beams at 3 m and 6 m. Listed top first.
const frame: BuildPiece[] = [
  member('beam6', P(0, 0, 6), P(5, 0, 6), [3, 6]),
  member('beam3', P(0, 0, 3), P(5, 0, 3), [2, 5]),
  member('colB2', P(5, 0, 3), P(5, 0, 6), [5, 6]),
  member('colA2', P(0, 0, 3), P(0, 0, 6), [2, 3]),
  member('colB1', P(5, 0, 0), P(5, 0, 3), [4, 5]),
  member('colA1', P(0, 0, 0), P(0, 0, 3), [1, 2]),
];
const keys = (order: number[]) => order.map((i) => frame[i]!.key);

describe('build order', () => {
  it('builds as it would go up: each storey’s columns, then the beam they hold', () => {
    expect(keys(buildOrder(frame, 'construction', new Set([1, 4])))).toEqual(
      ['colA1', 'colB1', 'beam3', 'colA2', 'colB2', 'beam6'],
    );
  });

  it('does not put up a piece nothing holds yet', () => {
    // A girt at 3 m between the columns: it waits for them even at a lower level band.
    const withGirt = [...frame, member('girt', P(0, 0, 1.5), P(5, 0, 1.5), [7, 8])];
    const order = buildOrder(withGirt, 'construction', new Set([1, 4])).map((i) => withGirt[i]!.key);
    // Nodes 7 and 8 are on no column: a floating piece still goes up, not lost.
    expect(order).toHaveLength(7);
    expect(order.slice(0, 2)).toEqual(['colA1', 'colB1']);
  });

  it('without supports, the lowest nodes are the ground', () => {
    expect(keys(buildOrder(frame, 'construction'))[0]).toBe('colA1');
  });

  it('bottom up goes by the height of the middle', () => {
    expect(keys(buildOrder(frame, 'bottomUp'))).toEqual(['colA1', 'colB1', 'beam3', 'colA2', 'colB2', 'beam6']);
  });

  it('along the longest side, each frame from the bottom', () => {
    expect(keys(buildOrder(frame, 'along'))).toEqual(['colA1', 'colA2', 'beam3', 'beam6', 'colB1', 'colB2']);
  });

  it('by type puts up every column first, then the beams', () => {
    expect(keys(buildOrder(frame, 'byType'))).toEqual(['colA1', 'colB1', 'colA2', 'colB2', 'beam3', 'beam6']);
  });

  it('radial starts at the middle of the ground and works outwards', () => {
    // A row of three columns: the middle one first, the far ones after; the beam over them last.
    const row = [
      member('left', P(-6, 0, 0), P(-6, 0, 3), [1, 2]),
      member('mid', P(0, 0, 0), P(0, 0, 3), [3, 4]),
      member('right', P(6.5, 0, 0), P(6.5, 0, 3), [5, 6]),
      member('top', P(-6, 0, 3), P(6.5, 0, 3), [2, 6]),
    ];
    const order = buildOrder(row, 'radial').map((i) => row[i]!.key);
    expect(order[0]).toBe('mid');
    expect(order.slice(1, 3)).toEqual(['top', 'left']);
  });

  it('a phased order without its phases builds as it would go up', () => {
    expect(buildOrder(frame, 'stages', new Set([1, 4]))).toEqual(buildOrder(frame, 'construction', new Set([1, 4])));
  });

  it('list keeps the table order', () => {
    expect(keys(buildOrder(frame, 'list'))).toEqual(frame.map((p) => p.key));
  });
});

describe('build schedule', () => {
  it('fits the whole build in the duration, one after the other', () => {
    const { steps, total } = buildSchedule(frame, 'construction', { duration: 10, supportedNodes: new Set([1, 4]) });
    expect(total).toBe(10);
    expect(steps.at(-1)!.end).toBeCloseTo(10);
    for (let i = 1; i < steps.length; i++) expect(steps[i]!.start).toBeGreaterThan(steps[i - 1]!.start);
  });

  it('a column grows from its foot, whichever end is listed first', () => {
    const down = member('c', P(0, 0, 3), P(0, 0, 0), [1, 2]);
    const [s] = buildSchedule([down], 'list', { duration: 2 }).steps;
    expect(s!.origin).toEqual(P(0, 0, 0));
    expect(s!.dir.z).toBeCloseTo(1);
  });

  it('eases from nothing to in place', () => {
    const [s] = buildSchedule(frame.slice(0, 1), 'list', { duration: 2 }).steps;
    expect(growthAt(s!, -1)).toBe(0);
    expect(growthAt(s!, (s!.start + s!.end) / 2)).toBeGreaterThan(0.5);
    expect(growthAt(s!, s!.end + 1)).toBe(1);
  });
});

describe('phases', () => {
  const sup = new Set([1, 4]);
  const phases = [
    { name: 'Ground floor', add: ['beam3', 'colB1', 'colA1'] },
    { name: 'First floor', add: ['colA2', 'colB2', 'beam6', 'colA1'] },
  ];

  it('goes phase by phase, each as it would be built, with a breath between them', () => {
    const sch = buildSchedule(frame, 'stages', { duration: 10, supportedNodes: sup, phases });
    expect(sch.steps.map((s) => s.key)).toEqual(['colA1', 'colB1', 'beam3', 'colA2', 'colB2', 'beam6']);
    expect(sch.phases.map((p) => p.name)).toEqual(['Ground floor', 'First floor']);
    const gap = sch.steps[3]!.start - sch.steps[2]!.start;
    expect(gap).toBeGreaterThan(sch.steps[1]!.start - sch.steps[0]!.start);
    expect(sch.steps.at(-1)!.end).toBeCloseTo(10);
    expect(phaseAt(sch, -1)).toBeNull();
    expect(phaseAt(sch, sch.steps[1]!.start)).toBe('Ground floor');
    expect(phaseAt(sch, 9.9)).toBe('First floor');
    expect(sch.partial).toBe(false);
  });

  it('a piece no phase names never goes up', () => {
    const sch = buildSchedule(frame, 'stages', { duration: 10, supportedNodes: sup, phases: phases.slice(0, 1) });
    expect(sch.steps).toHaveLength(3);
    expect(sch.partial).toBe(true);
  });

  it('a stage can take a piece down: it shrinks out after its turn', () => {
    const sch = buildSchedule(frame, 'stages', {
      duration: 10, supportedNodes: sup,
      phases: [...phases, { name: 'Strike', add: [], remove: ['beam3', 'ghost'] }],
    });
    const beam = sch.steps.find((s) => s.key === 'beam3')!;
    expect(beam.outStart).toBeGreaterThan(sch.steps.at(-1)!.start);
    expect(growthAt(beam, beam.outStart! - 0.01)).toBe(1);
    expect(growthAt(beam, beam.outEnd! + 0.01)).toBe(0);
    expect(sch.phases.at(-1)!.name).toBe('Strike');
  });
});

describe('landings', () => {
  it('each piece lands once, while it settles', () => {
    const sch = buildSchedule(frame, 'construction', { duration: 10, supportedNodes: new Set([1, 4]) });
    const s = sch.steps[0]!;
    expect(landAt(s)).toBeGreaterThan(s.start);
    expect(landAt(s)).toBeLessThan(s.end);
    expect(landedBetween(sch, -1, 100)).toBe(frame.length);
    expect(landedBetween(sch, -1, landAt(s))).toBe(1);
    expect(landedBetween(sch, landAt(s), landAt(s))).toBe(0);
  });
});

describe('grow matrix', () => {
  const apply = (m: number[], p: P3): P3 => ({
    x: m[0]! * p.x + m[4]! * p.y + m[8]! * p.z + m[12]!,
    y: m[1]! * p.x + m[5]! * p.y + m[9]! * p.z + m[13]!,
    z: m[2]! * p.x + m[6]! * p.y + m[10]! * p.z + m[14]!,
  });

  it('a member half grown reaches half way and keeps its section', () => {
    const [s] = buildSchedule([member('c', P(2, 1, 0), P(2, 1, 4), [1, 2])], 'list', { duration: 2 }).steps;
    const m = growMatrix(s!, 0.5);
    expect(apply(m, P(2, 1, 4))).toEqual(expect.objectContaining({ x: 2, y: 1, z: 2 }));
    expect(apply(m, P(2, 1, 0)).z).toBeCloseTo(0);
    // A point on the flange, 0.1 m off the axis at the top, stays 0.1 m off.
    expect(apply(m, P(2.1, 1, 4)).x).toBeCloseTo(2.1);
  });

  it('a slab grows in its plane and keeps its thickness', () => {
    const slab: BuildPiece = { key: 's', kind: 'shell', pts: [P(0, 0, 3), P(4, 0, 3), P(4, 4, 3), P(0, 4, 3)], nodes: [1, 2, 3, 4] };
    const [s] = buildSchedule([slab], 'list', { duration: 2 }).steps;
    const m = growMatrix(s!, 0.5);
    const corner = apply(m, P(4, 4, 3.1));
    expect(corner.x).toBeCloseTo(3);
    expect(corner.y).toBeCloseTo(3);
    expect(corner.z).toBeCloseTo(3.1);
  });
});

describe('drop, front and count', () => {
  it('a dropped piece starts above its place and lands on it', () => {
    expect(dropMatrix(5, 0)[14]).toBe(5);
    expect(dropMatrix(5, 1)[14]).toBe(0);
    expect(dropMatrix(5, 0.5)[0]).toBe(1);
  });

  it('the front rises with the pieces that have started, and so does the count', () => {
    const sch = buildSchedule(frame, 'construction', { duration: 10, supportedNodes: new Set([1, 4]) });
    expect(buildFront(sch, -1)).toBe(0);
    expect(piecesStarted(sch, -1)).toBe(0);
    expect(buildFront(sch, sch.steps[2]!.start + 0.01)).toBe(3);
    expect(buildFront(sch, 100)).toBe(6);
    expect(piecesStarted(sch, 100)).toBe(frame.length);
    expect([sch.zMin, sch.zMax]).toEqual([0, 6]);
  });
});
