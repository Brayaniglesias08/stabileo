import { describe, it, expect } from 'vitest';
import { resolveNodePlacement2D, memberParamOnRay, ON_NODE_PX } from '../node-placement';

// The formwork wall of the report: a vertical member 2 → 3 (z 3.00 → 1.64), drawn at 160 px/m.
const nodes = [{ id: 2, x: 0, y: 3 }, { id: 3, x: 0, y: 1.64 }, { id: 1, x: 0, y: 0 }];
const members = [{ id: 6, nodeI: 2, nodeJ: 3 }, { id: 7, nodeI: 3, nodeJ: 1 }];
const nodeAt = (id: number) => nodes.find((n) => n.id === id);
const at = (x: number, y: number, split = true) => resolveNodePlacement2D({ cursor: { x, y }, snapped: { x, y }, pxPerM: 160, nodes, members, nodeAt, split });

describe('where a node-tool click puts its node', () => {
  it('right on a node: there is one already, nothing is made and nothing is grabbed', () => {
    expect(at(0, 1.64 + (ON_NODE_PX - 1) / 160)).toMatchObject({ kind: 'onNode', nodeId: 3 });
  });

  it('close to a node but on the member: a new node splitting it, where the click is', () => {
    // 10 px above node 3, on member 6: the case that used to select node 3 instead.
    const p = at(0.01, 1.64 + 10 / 160);
    expect(p.kind).toBe('onMember');
    if (p.kind !== 'onMember') return;
    expect(p.elementId).toBe(6);
    expect(p.y).toBeCloseTo(1.64 + 10 / 160, 6);
    expect(p.x).toBeCloseTo(0);
  });

  it('near the middle of a member, its midpoint', () => {
    const p = at(0.02, 2.33);
    expect(p).toMatchObject({ kind: 'onMember', elementId: 6, midpoint: true });
    if (p.kind === 'onMember') expect(p.t).toBe(0.5);
  });

  it('away from everything, a free node; with splitting off, a free node on the member too', () => {
    expect(at(1, 1)).toMatchObject({ kind: 'free', x: 1, y: 1 });
    expect(at(0.01, 2.5, false)).toMatchObject({ kind: 'free' });
  });
});

describe('node on a member in space', () => {
  it('the ray from the pointer meets the member where it passes closest', () => {
    // A member along x at height 2; a ray straight down through x = 1.5 hits it at t = 0.375.
    const t = memberParamOnRay({ x: 1.5, y: 0, z: 10 }, { x: 0, y: 0, z: -1 }, { x: 0, y: 0, z: 2 }, { x: 4, y: 0, z: 2 });
    expect(t).toBeCloseTo(0.375);
    // Slanted ray passing over the member: still the closest station.
    expect(memberParamOnRay({ x: 0, y: -5, z: 2 }, { x: 1, y: 2.5, z: 0 }, { x: 0, y: 0, z: 2 }, { x: 4, y: 0, z: 2 })).toBeCloseTo(0.5);
    expect(memberParamOnRay({ x: 0, y: 0, z: 0 }, { x: 1, y: 0, z: 0 }, { x: 0, y: 0, z: 0 }, { x: 4, y: 0, z: 0 })).toBeNull();
  });
});
