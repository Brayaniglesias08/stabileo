/**
 * A distributed load that goes on into the next member (split at a node, or typed member by
 * member) is written once at the joint, and not at all where it also keeps its slope.
 */
import { describe, expect, it } from 'vitest';
import { continuousLoadJoints } from '../draw-loads';

const nodes = new Map([[1, { x: 0, y: 0 }], [2, { x: 0, y: 3 }], [3, { x: 0, y: 1.64 }], [10, { x: 0, y: 1.776 }], [4, { x: 0, y: 0.68 }]]);
const elements = new Map([[9, { nodeI: 2, nodeJ: 10 }], [10, { nodeI: 10, nodeJ: 3 }], [7, { nodeI: 3, nodeJ: 4 }], [5, { nodeI: 1, nodeJ: 4 }]]);
const run = (loads: Parameters<typeof continuousLoadJoints>[0]) =>
  continuousLoadJoints(loads, (id) => elements.get(id), (id) => nodes.get(id));

describe('continuous distributed loads', () => {
  it('a split triangle loses both values at the new node; a change of slope keeps one', () => {
    const j = run([
      { elementId: 9, qI: 0, qJ: -36.9, caseName: 'D' },
      { elementId: 10, qI: -36.9, qJ: -41, caseName: 'D' },
      { elementId: 7, qI: -41, qJ: -58, caseName: 'D' },
    ]);
    expect(j.hideEnd).toEqual(new Set(['0:j', '1:i', '2:i']));
  });

  it('works whichever way the next member is drawn', () => {
    // Member 5 runs 1→4, against member 7 (3→4): the joint is node 4 at both J ends.
    const j = run([
      { elementId: 7, qI: -41, qJ: -58, caseName: 'D' },
      { elementId: 5, qI: -75, qJ: -58, caseName: 'D' },
    ]);
    expect(j.hideEnd.has('1:j')).toBe(true);
    expect(j.hideEnd.has('0:j')).toBe(false);
  });

  it('a different value, case or a partial load keeps its labels; equal uniform loads write one', () => {
    expect(run([{ elementId: 9, qI: 0, qJ: -36.9, caseName: 'D' }, { elementId: 10, qI: -30, qJ: -41, caseName: 'D' }]).hideEnd.size).toBe(0);
    expect(run([{ elementId: 9, qI: 0, qJ: -36.9, caseName: 'D' }, { elementId: 10, qI: -36.9, qJ: -41, caseName: 'L' }]).hideEnd.size).toBe(0);
    expect(run([{ elementId: 9, qI: 0, qJ: -36.9, caseName: 'D', b: 1 }, { elementId: 10, qI: -36.9, qJ: -41, caseName: 'D' }]).hideEnd.size).toBe(0);
    const u = run([{ elementId: 9, qI: -5, qJ: -5 }, { elementId: 10, qI: -5, qJ: -5 }]);
    expect(u.hideMid).toEqual(new Set([1]));
  });

  it('two loads on one member add up, so each keeps its value', () => {
    // Two equal UDLs on member 9: 20 acts, and hiding one would leave the reader with 10.
    const u = run([{ elementId: 9, qI: -10, qJ: -10, caseName: 'D' }, { elementId: 9, qI: -10, qJ: -10, caseName: 'D' }]);
    expect(u.hideMid.size).toBe(0);
    expect(u.hideEnd.size).toBe(0);
    // Two equal trapezoids on it keep all four end values.
    const t = run([{ elementId: 9, qI: -5, qJ: -8, caseName: 'D' }, { elementId: 9, qI: -5, qJ: -8, caseName: 'D' }]);
    expect(t.hideEnd.size).toBe(0);
  });
});
