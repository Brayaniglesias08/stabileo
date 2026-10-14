/**
 * The influence line with the unit load across, to +X: on a vertical member (a wall, a column) a
 * downward unit load only runs along it, so every shear and moment line was zero and nothing was
 * drawn. A horizontal unit load gives the member's own line, the same as a beam's under gravity.
 */
import { describe, it, expect, beforeAll } from 'vitest';
import { initSolver } from '../wasm-solver';
import { computeInfluenceLine } from '../influence-service';
import { loadOnMember, verticalLoadOnMember } from '../moving-loads';
import type { SolverInput } from '../types';

beforeAll(async () => { await initSolver(); });

/** A member from (0, 0) to (0, L) (vertical) or (L, 0), pinned at both ends, as the store holds it. */
function member(L: number, vertical: boolean) {
  const nodes = new Map<number, unknown>([[1, { id: 1, x: 0, y: 0 }], [2, { id: 2, x: vertical ? 0 : L, y: vertical ? L : 0 }]]);
  return {
    nodes,
    elements: new Map([[1, { id: 1, nodeI: 1, nodeJ: 2, materialId: 1, sectionId: 1, type: 'frame', releaseI: {}, releaseJ: {} }]]),
    materials: new Map([[1, { id: 1, e: 200_000, nu: 0.3, rho: 78.5 }]]),
    sections: new Map([[1, { id: 1, a: 0.0069, iz: 9.8e-5, iy: 9.8e-5 }]]),
    supports: new Map([[1, { id: 1, nodeId: 1, type: 'pinned' }], [2, { id: 2, nodeId: 2, type: 'pinned' }]]),
    loads: [], plates: new Map(), quads: new Map(),
  } as never;
}

const peak = (r: ReturnType<typeof computeInfluenceLine>) => {
  if (typeof r === 'string') throw new Error(r);
  return Math.max(...r.points.map((p) => Math.abs(p.value)));
};

describe('influence line with a horizontal unit load', () => {
  it('on a vertical member: zero with the load down, L/4 at mid-height with it across', () => {
    expect(peak(computeInfluenceLine(member(4, true), 'M', undefined, 1, 0.5))).toBeLessThan(1e-9);
    const across = computeInfluenceLine(member(4, true), 'M', undefined, 1, 0.5, 20, 'horizontal');
    expect(peak(across)).toBeCloseTo(1, 3);
    expect(typeof across !== 'string' && across.direction).toBe('horizontal');
  });

  it('a vertical member across is a beam under gravity; a beam across carries nothing', () => {
    const beam = peak(computeInfluenceLine(member(4, false), 'V', undefined, 1, 0.5));
    expect(peak(computeInfluenceLine(member(4, true), 'V', undefined, 1, 0.5, 20, 'horizontal'))).toBeCloseTo(beam, 6);
    expect(peak(computeInfluenceLine(member(4, false), 'M', undefined, 1, 0.5, 20, 'horizontal'))).toBeLessThan(1e-9);
  });

  it('the downward load is placed as it always was', () => {
    const input = {
      nodes: new Map([[1, { id: 1, x: 0, z: 0 }], [2, { id: 2, x: 3, z: 4 }]]),
      elements: new Map([[1, { id: 1, type: 'frame', nodeI: 1, nodeJ: 2 }]]),
    } as unknown as SolverInput;
    expect(verticalLoadOnMember(input, 1, 2, 10)).toEqual(loadOnMember(input, 1, 2, 10, { x: 0, z: -1 }));
    const [perp] = loadOnMember(input, 1, 2, 10, { x: 1, z: 0 });
    // Across a 3-4-5 member, a push to +X: −s·Fx = −0.8·10 on its local y.
    expect(perp).toMatchObject({ type: 'pointOnElement', data: { a: 2, p: -8 } });
  });
});
