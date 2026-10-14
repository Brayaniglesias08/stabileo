import { describe, expect, it } from 'vitest';
import { freeBodyActions3D } from '../free-body-3d';
import { drawFreeBody } from '../../canvas/free-body';
import { createSvgContext } from '../../export/svg-context';
import type { Element, Node, Load } from '../../store/model.svelte';

function actions(loads: Load[], end = { x: 0, y: 0, z: 4 }, leftHand = false, loadMode: 'all' | 'resultant' = 'all') {
  return freeBodyActions3D({
    nodes: new Map<number, Node>([[1, { id: 1, x: 0, y: 0, z: 0 }], [2, { id: 2, ...end }]]),
    elements: new Map<number, Element>([[1, { id: 1, nodeI: 1, nodeJ: 2, type: 'frame', materialId: 1, sectionId: 1 } as Element]]),
    forces: [], reactions: [], project2D: false, leftHand, basis: 'local', resultant: false,
    showReactions: true, loads, loadMode, loadText: (v, q) => `${v.toFixed(1)} ${q}`,
  });
}
const dist = (patch: Partial<Extract<Load, { type: 'distributed3d' }>['data']> = {}): Load => ({
  type: 'distributed3d', data: { id: 1, elementId: 1, qYI: 0, qYJ: 0, qZI: -10, qZJ: -10, ...patch },
});

describe('applied loads in 3D figures and free bodies', () => {
  it('renders a pure nodal moment and its value, including when viewed end on', () => {
    const data = actions([{ type: 'nodal3d', data: { id: 1, nodeId: 2, fx: 0, fy: 0, fz: 0, mx: 0, my: 10, mz: 0 } }]);
    expect(data.loads).toMatchObject([{ kind: 'nodal', action: 'moment', dir: { x: 0, y: 1, z: 0 }, label: '10.0 moment' }]);
    for (const endOn of [false, true]) {
      const svg = createSvgContext(500, 500);
      drawFreeBody({ ...data, ctx: svg.ctx,
        toScreen: p => ({ x: 100 + p.x * 50, y: 300 - (p.z ?? 0) * 50 - (endOn ? 0 : p.y * 50) }),
        towardViewer: d => d.y, mode: 'members', sep: 1, planar: false, fmt: String });
      expect(svg.body()).toContain('10.0 moment');
      const heads = [...svg.body().matchAll(/<path[^>]*fill="rgb\(217,164,65\)"[^>]*stroke="none"/g)];
      expect(heads).toHaveLength(endOn ? 1 : 2); // dot, or double arrowhead
    }
  });

  it.each([false, true])('preserves global direction on a column (left hand: %s)', leftHand => {
    const loads = actions([dist({ frame: 'global' })], undefined, leftHand).loads;
    expect(loads).toMatchObject([{ kind: 'dist', qI: { x: 0, y: 0, z: -10 }, qJ: { x: 0, y: 0, z: -10 } }]);
    const resultant = actions([dist({ frame: 'global' })], undefined, leftHand, 'resultant').loads;
    expect(resultant).toMatchObject([{ kind: 'point', t: 0.5, dir: { x: 0, y: 0, z: -1 }, label: '40.0 force' }]);
  });

  it('preserves zero ends, sign changes and endpoint intensities instead of averaging', () => {
    for (const [qZI, qZJ] of [[0, -10], [-10, 10], [-3, -10]]) {
      expect(actions([dist({ frame: 'global', qZI, qZJ })]).loads).toMatchObject([
        { kind: 'dist', qI: { x: 0, y: 0, z: qZI }, qJ: { x: 0, y: 0, z: qZJ } },
      ]);
    }
  });

  it.each([0, -10])('draws the profile, with a zero end or a reversal (qI: %s)', qZI => {
    const data = actions([dist({ frame: 'global', qZI, qZJ: 10 })], { x: 4, y: 0, z: 0 });
    const svg = createSvgContext(500, 300);
    drawFreeBody({ ...data, ctx: svg.ctx,
      toScreen: p => ({ x: 100 + 50 * p.x, y: 100 - 50 * (p.z ?? 0) }),
      mode: 'members', sep: 0, planar: false, fmt: String });
    const profile = [...svg.body().matchAll(/<path d="([^"]*)"[^>]*stroke="rgb\(217,164,65\)"[^>]*stroke-width="1.4"/g)];
    expect(profile).toHaveLength(1);
    const coordinates = profile[0][1].match(/-?\d+(?:\.\d+)?/g)!.map(Number);
    expect(coordinates.slice(0, 2)).toEqual([100, qZI === 0 ? 100 : 74.4]);
    expect(coordinates.slice(-2)).toEqual([300, 125.6]);
    expect(svg.body()).toContain('10.0 distributedLoad');
  });

  it('keeps load values visible when member loads point along the viewing direction', () => {
    for (const loadMode of ['all', 'resultant'] as const) {
      const data = actions([dist({ frame: 'global', qYI: 10, qYJ: 10, qZI: 0, qZJ: 0 })], { x: 4, y: 0, z: 0 }, false, loadMode);
      const svg = createSvgContext(500, 300);
      drawFreeBody({ ...data, ctx: svg.ctx,
        toScreen: p => ({ x: 100 + 50 * p.x, y: 100 - 50 * (p.z ?? 0) }),
        towardViewer: d => d.y, mode: 'members', sep: 0, planar: false, fmt: String });
      expect(svg.body()).toContain(loadMode === 'all' ? '10.0 distributedLoad' : '40.0 force');
      expect(svg.body()).toContain('stroke="rgb(217,164,65)"');
    }
  });

  it('retains axial distributed components and applies projected-load factors over a partial span', () => {
    expect(actions([dist({ qXI: 7, qXJ: 7, qZI: 0, qZJ: 0 })]).loads).toMatchObject([
      { kind: 'dist', qI: { x: 0, y: 0, z: 7 }, qJ: { x: 0, y: 0, z: 7 } },
    ]);
    const end = { x: 4, y: 0, z: 3 };
    const loads = [dist({ frame: 'projected', a: 1, b: 3 })];
    expect(actions(loads, end).loads).toMatchObject([
      { kind: 'dist', t0: 0.2, t1: 0.6, qI: { x: 0, y: 0, z: -8 }, qJ: { x: 0, y: 0, z: -8 } },
    ]);
    expect(actions(loads, end, false, 'resultant').loads).toMatchObject([
      { kind: 'point', t: 0.4, dir: { x: 0, y: 0, z: -1 }, label: '16.0 force' },
    ]);
  });

  it('keeps both forces of a sign-reversing resultant, including their couple', () => {
    const loads = actions([dist({ frame: 'global', qZI: -10, qZJ: 10 })], { x: 4, y: 0, z: 0 }, false, 'resultant').loads;
    expect(loads).toHaveLength(2);
    const points = loads.filter(l => l.kind === 'point');
    expect(points[0].t).toBeCloseTo(1 / 3);
    expect(points[1].t).toBeCloseTo(2 / 3);
    expect(points.map(p => p.dir.z)).toEqual([-1, 1]);
    expect(points.map(p => p.label)).toEqual(['20.0 force', '20.0 force']);
  });

  it('preserves total force and moment for a partial load with different component centroids', () => {
    const loads = actions([dist({ frame: 'global', a: 1, b: 3,
      qXI: 1, qXJ: 3, qYI: -4, qYJ: -2, qZI: -10, qZJ: 20,
    })], { x: 4, y: 0, z: 0 }, false, 'resultant').loads;
    const total = { x: 0, y: 0, z: 0, my: 0, mz: 0 };
    for (const load of loads) {
      expect(load.kind).toBe('point');
      if (load.kind !== 'point') continue;
      const magnitude = parseFloat(load.label), x = 4 * load.t;
      const fy = magnitude * load.dir.y, fz = magnitude * (load.dir.z ?? 0);
      total.x += magnitude * load.dir.x; total.y += fy; total.z += fz;
      total.my -= x * fz; total.mz += x * fy;
    }
    // Analytical integrals over x ∈ [1, 3]; labels round forces to 0.1.
    expect(total.x).toBeCloseTo(4, 1);
    expect(total.y).toBeCloseTo(-6, 1);
    expect(total.z).toBeCloseTo(10, 1);
    expect(total.my).toBeCloseTo(-30, 1);
    expect(total.mz).toBeCloseTo(-34 / 3, 1);
  });
});
