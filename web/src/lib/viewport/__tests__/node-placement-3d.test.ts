/**
 * The space node tool, read on the screen: seen in plan (100 px a metre, the working plane z = 0).
 *
 * - A node drawn over the cursor from another level is not the click, and what is under it on the
 *   working plane is: a member there is split, not covered by a free node that looks joined and is
 *   analysed as a cut.
 * - A cut the grid puts on a member's end is that end node: the store does not cut a member at its
 *   very end (`splitElementAtPoint` refuses t ≤ 0.01), and the hint promised a split that never came.
 */
import { describe, expect, it } from 'vitest';
import { resolveNodePlacement3D } from '../node-placement';
import { modelStore } from '../../store';

type N = { id: number; x: number; y: number; z: number };
const toScreen = (p: { x: number; y: number }) => ({ x: 100 + 100 * p.x, y: 300 - 100 * p.y });
const fromScreen = (s: { x: number; y: number }) => ({ x: (s.x - 100) / 100, y: (300 - s.y) / 100, z: 0 });

function click(model: N[], members: Array<{ id: number; nodeI: number; nodeJ: number }>, cursor: { x: number; y: number }, grid = 0) {
  const byId = new Map(model.map((n) => [n.id, n]));
  const snap = (p: { x: number; y: number; z: number }) => (grid ? { x: Math.round(p.x / grid) * grid, y: Math.round(p.y / grid) * grid, z: p.z } : p);
  return resolveNodePlacement3D({
    cursor, nodes: model.map((n) => ({ id: n.id, ...toScreen(n) })), members,
    nodeAt: (id) => { const n = byId.get(id); return n ? toScreen(n) : undefined; },
    split: true, model: byId, plane: 'XY', planeLevel: 0, ground: fromScreen(cursor), snap,
    rayParam: () => null, project: toScreen,
  });
}

describe('the space node tool on the screen', () => {
  it('a node above a ground beam, clicked in plan, splits the beam under it', () => {
    const model = [{ id: 1, x: 0, y: 0, z: 0 }, { id: 2, x: 4, y: 0, z: 0 }, { id: 3, x: 2, y: 0, z: 3 }];
    const p = click(model, [{ id: 1, nodeI: 1, nodeJ: 2 }], toScreen({ x: 2, y: 0 }));
    expect(p).toMatchObject({ kind: 'onMember', elementId: 1, t: 0.5 });
  });

  it('a column foot under its top, clicked in plan, is the click', () => {
    const model = [{ id: 1, x: 2, y: 0, z: 0 }, { id: 2, x: 2, y: 0, z: 3 }];
    expect(click(model, [{ id: 1, nodeI: 1, nodeJ: 2 }], toScreen({ x: 2, y: 0 }))).toMatchObject({ kind: 'onNode', nodeId: 1 });
  });

  it('a node above, with nothing under it, places a free node on the working plane', () => {
    const model = [{ id: 3, x: 2, y: 0, z: 3 }];
    expect(click(model, [], toScreen({ x: 2, y: 0 }))).toMatchObject({ kind: 'free', p: { x: 2, y: 0, z: 0 } });
  });

  it('a cut the grid snaps onto a long member\'s end is that end node', () => {
    // A 10 m beam, 1000 px on the screen; a click 12 px from node 1, the 1 m grid snapping to it.
    const model = [{ id: 1, x: 0, y: 0, z: 0 }, { id: 2, x: 10, y: 0, z: 0 }];
    const p = click(model, [{ id: 1, nodeI: 1, nodeJ: 2 }], toScreen({ x: 0.12, y: 0 }), 1);
    expect(p).toMatchObject({ kind: 'onNode', nodeId: 1 });
    // The same click without the grid cuts the member where it is.
    expect(click(model, [{ id: 1, nodeI: 1, nodeJ: 2 }], toScreen({ x: 0.12, y: 0 }))).toMatchObject({ kind: 'onMember', elementId: 1 });
    // And a cut at the end is one the store refuses: the reason it must not be offered.
    modelStore.clear();
    const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(10, 0, 0);
    expect(modelStore.splitElementAtPoint(modelStore.addElement(a, b, 'frame'), 0.01)).toBeNull();
  });

  it('every cut it offers is one the store makes', () => {
    const model = [{ id: 1, x: 0, y: 0, z: 0 }, { id: 2, x: 10, y: 0, z: 0 }];
    for (let x = 0.05; x < 10; x += 0.05) {
      const p = click(model, [{ id: 1, nodeI: 1, nodeJ: 2 }], toScreen({ x, y: 0 }), 0.5);
      if (p?.kind === 'onMember') {
        expect(p.t).toBeGreaterThan(0.01);
        expect(p.t).toBeLessThan(0.99);
      }
    }
  });
});
