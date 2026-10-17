import { describe, it, expect } from 'vitest';
import { drawFreeBody, freeBodyFocus, screenPieces, valence, FB_GAP_PX, FB_FOCUS_ABOVE, type FBEnd } from '../free-body';
import { createSvgContext } from '../../export/svg-context';

// A portal on the screen: two columns and a beam (y down). The bases have one member each.
const nodes: Record<number, { x: number; y: number }> = { 1: { x: 100, y: 500 }, 2: { x: 100, y: 200 }, 3: { x: 500, y: 200 }, 4: { x: 500, y: 500 } };
const members = [
  { id: 1, nodeI: 1, nodeJ: 2, i: nodes[1]!, j: nodes[2]! },
  { id: 2, nodeI: 2, nodeJ: 3, i: nodes[2]!, j: nodes[3]! },
  { id: 3, nodeI: 3, nodeJ: 4, i: nodes[3]!, j: nodes[4]! },
];
const id = (p: { x: number; y: number }) => p;
const force = (name: string, dx: number, dy: number, m: number) => ({ name, kind: 'force' as const, dir: { x: dx, y: dy }, magnitude: m, color: '#f00' });
const ends: FBEnd[] = [
  { memberId: 2, end: 'I', nodeId: 2, side: 'member', actions: [force('N', -1, 0, 12), force('V', 0, 1, 30), { name: 'M', kind: 'moment', dir: { x: 0, y: 0 }, ccw: true, magnitude: 20, color: '#fc0' }] },
  { memberId: 2, end: 'J', nodeId: 3, side: 'member', actions: [force('N', 1, 0, 12), force('V', 0, 1, 30)] },
  { memberId: 1, end: 'J', nodeId: 2, side: 'member', actions: [force('N', 0, -1, 30)] },
];

describe('the free-body drawing', () => {
  it('cuts members back only at shared joints, by the same distance on the screen', () => {
    expect(valence(members).get(2)).toBe(2);
    const p = screenPieces(members, id, FB_GAP_PX);
    expect(p.get(1)!.a).toEqual(nodes[1]);                  // the base stays whole
    expect(p.get(1)!.b.y).toBeCloseTo(200 + FB_GAP_PX);      // the knee is cut back
    expect(p.get(2)!.a.x).toBeCloseTo(100 + FB_GAP_PX);
  });

  it('writes each cut\'s values as one block, magnitudes only, none over another', () => {
    const svg = createSvgContext(600, 600);
    drawFreeBody({ ctx: svg.ctx, toScreen: id, members, nodes: (n) => nodes[n], ends, supports: [], mode: 'members', sep: 1, planar: true, fmt: (v) => `${v.toFixed(1)} kN` });
    const body = svg.body();
    expect(body).toContain('N = 12.0 kN');
    expect(body).toContain('V = 30.0 kN');
    expect(body).not.toMatch(/= -/);
    // The text boxes of the blocks do not overlap.
    const rows = [...body.matchAll(/<text x="([\d.]+)" y="([\d.]+)"[^>]*>([^<]+)<\/text>/g)].map((m) => ({ x: +m[1]!, y: +m[2]!, t: m[3]! }));
    expect(rows.length).toBe(6);
    const ys = new Set(rows.map((r) => `${Math.round(r.x)}|${Math.round(r.y)}`));
    expect(ys.size).toBe(rows.length);
  });

  it('a large model details only what is selected', () => {
    const many = Array.from({ length: FB_FOCUS_ABOVE + 1 }, (_, k) => ({ id: k + 1, nodeI: k, nodeJ: k + 1 }));
    expect(freeBodyFocus(members, new Set(), new Set())).toBeUndefined();
    expect([...freeBodyFocus(many, new Set([3]), new Set([10]))!].sort((a, b) => a - b)).toEqual([3, 10, 11]);
    const svg = createSvgContext(600, 600);
    drawFreeBody({ ctx: svg.ctx, toScreen: id, members, nodes: (n) => nodes[n], ends, supports: [], mode: 'members', sep: 1, planar: true, fmt: (v) => `${v}`, focus: new Set([1]), hint: 'select' });
    expect(svg.body()).not.toContain('V = 30');
    expect(svg.body()).toContain('N = 30');
  });

  it('with nothing selected in a large model, the reactions are left out with the rest', () => {
    // "The rest drawn as pieces only": every support's reaction was still written.
    const supports = [{ nodeId: 1, actions: [force('Rz', 0, -1, 77)] }, { nodeId: 4, actions: [force('Rz', 0, -1, 88)] }];
    const draw = (focus: Set<number>) => {
      const svg = createSvgContext(600, 600);
      drawFreeBody({ ctx: svg.ctx, toScreen: id, members, nodes: (n) => nodes[n], ends, supports, mode: 'members', sep: 1, planar: true, fmt: (v) => `${v}`, focus, hint: 'select' });
      return svg.body();
    };
    const none = draw(new Set());
    expect(none).toContain('select');
    expect(none).not.toContain('Rz = 77');
    expect(none).not.toContain('Rz = 88');
    const first = draw(new Set([1]));
    expect(first).toContain('Rz = 77');
    expect(first).not.toContain('Rz = 88');
  });

  it.each(['nodes', 'all'] as const)('a joint of a selected member is drawn whole, its other members\' actions too (%s)', (mode) => {
    // Joint 2 of member 1: member 2's action on it is part of its free body, though 2 is not selected.
    const atJoint: FBEnd[] = [
      { memberId: 1, end: 'J', nodeId: 2, side: 'member', actions: [force('N', 0, -1, 30)] },
      { memberId: 1, end: 'J', nodeId: 2, side: 'node', actions: [force('N', 0, 1, 30)] },
      { memberId: 2, end: 'I', nodeId: 2, side: 'node', actions: [force('V', 0, -1, 31)] },
      { memberId: 2, end: 'J', nodeId: 3, side: 'node', actions: [force('V', 0, -1, 32)] },
    ];
    const svg = createSvgContext(600, 600);
    drawFreeBody({ ctx: svg.ctx, toScreen: id, members, nodes: (n) => nodes[n], ends: atJoint, supports: [], mode, sep: 1, planar: true, fmt: (v) => `${v}`, focus: new Set([1]) });
    expect(svg.body()).toContain('N = 30');
    expect(svg.body()).toContain('V = 31');
    // A joint no selected member reaches stays a piece only.
    expect(svg.body()).not.toContain('V = 32');
  });
});
