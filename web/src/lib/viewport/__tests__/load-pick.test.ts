import { describe, it, expect } from 'vitest';
import { pickLoadAt } from '../load-pick';

// Screen = world x, y for these cases.
const flat = (x: number, y: number) => ({ x, y });
const seg = (ax: number, ay: number, bx: number, by: number) => [ax, ay, 0, bx, by, 0];

describe('pickLoadAt', () => {
  it('takes the load whose drawn segments are nearest, within the tolerance', () => {
    const fp = new Map([[1, seg(0, 0, 0, 50)], [2, seg(30, 0, 30, 50)]]);
    expect(pickLoadAt(4, 20, fp, flat)).toBe(1);
    expect(pickLoadAt(26, 20, fp, flat)).toBe(2);
    expect(pickLoadAt(15, 20, fp, flat)).toBeNull();
  });

  it('measures every segment of a load, not only its first', () => {
    const fp = new Map([[1, [...seg(0, 0, 0, 10), ...seg(100, 0, 100, 10)]]]);
    expect(pickLoadAt(101, 5, fp, flat)).toBe(1);
  });

  it('at a joint, the load acting at the point wins a near tie', () => {
    // A distributed load's first arrow and a nodal load's arrow both end at (0, 0).
    const fp = new Map([[1, seg(0, -40, 0, 0)], [3, seg(-40, 0, 0, 0)]]);
    expect(pickLoadAt(0, 0, fp, flat, 10, new Set([3]))).toBe(3);
    // Clearly on the distributed one, it still wins.
    expect(pickLoadAt(0, -30, fp, flat, 10, new Set([3]))).toBe(1);
  });
});

import { pickLoadsWithDistance } from '../load-pick';

describe('a load on a plate is picked on its fill', () => {
  // Screen = world x, y. Load 1: a slab 0..100 × 0..100 with one arrow at its centre.
  // Load 2: a slab temperature, only a face. Load 3: an arrow on the slab's edge.
  const flat = (x: number, y: number) => ({ x, y });
  const footprints = new Map<number, number[]>([[1, [50, 50, 0, 50, 50, 10]], [3, [0, 0, 0, 100, 0, 0]]]);
  const slab = [0, 0, 0, 100, 0, 0, 100, 100, 0, 0, 100, 0];
  const areas = new Map<number, number[][]>([[1, [slab]], [2, [slab.map((v, i) => (i % 3 === 0 ? v + 200 : v))]]]);

  it('inside the face, away from its arrows', () => {
    expect(pickLoadsWithDistance(80, 80, footprints, flat, 10, new Set(), areas).map((h) => h.id)).toEqual([1]);
  });
  it('a face with no arrows at all (a slab temperature)', () => {
    expect(pickLoadsWithDistance(250, 50, footprints, flat, 10, new Set(), areas).map((h) => h.id)).toEqual([2]);
  });
  it('an arrow under the pointer still wins over the fill around it', () => {
    expect(pickLoadsWithDistance(20, 2, footprints, flat, 10, new Set(), areas)[0]!.id).toBe(3);
  });
  it('outside every face and arrow: nothing', () => {
    expect(pickLoadsWithDistance(150, 150, footprints, flat, 10, new Set(), areas)).toEqual([]);
  });
});

describe('a click on overlapping fills takes the nearest, then the smallest', () => {
  // Screen x, y from world x, y; depth from world z (smaller is nearer, as Vector3.project gives).
  const deep = (x: number, y: number, z: number) => ({ x, y, z });
  const square = (x0: number, y0: number, s: number, z: number) => [x0, y0, z, x0 + s, y0, z, x0 + s, y0 + s, z, x0, y0 + s, z];

  it('the slab nearer the eye wins, whatever the ids', () => {
    // Load 1 on the lower slab (far), load 2 on the upper one (near), one over the other in plan.
    const areas = new Map<number, number[][]>([[1, [square(0, 0, 100, 0.9)]], [2, [square(0, 0, 100, 0.1)]]]);
    expect(pickLoadsWithDistance(50, 50, new Map(), deep, 10, new Set(), areas).map((h) => h.id)).toEqual([2, 1]);
  });

  it('on one slab, a load on a rectangle of it wins over the load on the whole slab', () => {
    const areas = new Map<number, number[][]>([[1, [square(0, 0, 100, 0.5)]], [2, [square(40, 40, 20, 0.5)]]]);
    expect(pickLoadsWithDistance(50, 50, new Map(), deep, 10, new Set(), areas)[0]!.id).toBe(2);
    expect(pickLoadsWithDistance(10, 10, new Map(), deep, 10, new Set(), areas).map((h) => h.id)).toEqual([1]);
  });
});

import { shellLoadExtent } from '../../engine/shell-load-integration';

describe('the fill a regional load is clicked on is its region', () => {
  const slab = [{ x: 0, y: 0, z: 3 }, { x: 4, y: 0, z: 3 }, { x: 4, y: 4, z: 3 }, { x: 0, y: 4, z: 3 }];
  const areaXY = (poly: number[][]) => Math.abs(poly.reduce((s, p, i) => { const q = poly[(i + 1) % poly.length]!; return s + p[0]! * q[1]! - q[0]! * p[1]!; }, 0)) / 2;

  it('a rectangle of the slab, not the slab', () => {
    const pieces = shellLoadExtent('quad', slab, { region: { normal: [0, 0, 1], points: [[1, 1, 0], [2, 1, 0], [2, 3, 0], [1, 3, 0]] } });
    expect(pieces).toHaveLength(1);
    expect(areaXY(pieces[0]!)).toBeCloseTo(2, 9);
    for (const p of pieces[0]!) expect(p[2]).toBeCloseTo(3, 9);
  });

  it('a range along x, the whole slab without one, nothing for an empty range', () => {
    expect(areaXY(shellLoadExtent('quad', slab, { vary: { dir: [1, 0, 0], c1: 0, q1: 1, c2: 1, q2: 2 } })[0]!)).toBeCloseTo(4, 9);
    expect(areaXY(shellLoadExtent('quad', slab, {})[0]!)).toBeCloseTo(16, 9);
    expect(shellLoadExtent('quad', slab, { vary: { dir: [1, 0, 0], c1: 2, q1: 1, c2: 2, q2: 2 } })).toEqual([]);
  });
});
