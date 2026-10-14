/**
 * Add support on many nodes (the Supports tab's card on a range, a group, a selection) is one pass:
 * the per-node `addSupport` scanned every support and copied the map on each node, so 8000 nodes
 * took seconds where the same number of loads took a tenth of one. The bulk path keeps what the
 * per-node one did: one support per node (a held node takes the new one in its place), the draft's
 * fields, and one undo step for the lot.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { modelStore } from '../model.svelte';
import { uiStore } from '../ui.svelte';
import { historyStore } from '../history.svelte';
import '../index';
import { drawState } from '../draw-state.svelte';

beforeEach(() => { uiStore.analysisMode = 'pro'; modelStore.clear(); historyStore.clear(); });

describe('Add support on many nodes', () => {
  it('8000 nodes in well under a second', () => {
    const ids: number[] = [];
    modelStore.bulkMutate(() => { for (let i = 0; i < 8000; i++) ids.push(modelStore.addNode(i, 0, 0)); });
    const t0 = performance.now();
    drawState.addSupportsAt(ids);
    const dt = performance.now() - t0;
    expect(modelStore.supports.size).toBe(8000);
    // The per-node path took 2.4 s here, and 3.7–6.7 s in the browser.
    expect(dt).toBeLessThan(1000);
  });

  it('a node that holds a support takes the new one in its place; one undo step takes them all back', () => {
    const [a, b, c] = [modelStore.addNode(0, 0, 0), modelStore.addNode(1, 0, 0), modelStore.addNode(2, 0, 0)];
    const old = modelStore.addSupport(a, 'pinned3d');
    const before = modelStore.supports.size;
    historyStore.clear();

    // The draft: elastic along Z.
    const d = drawState.support;
    d.elastic = true; d.springs.kz = 5000;
    try {
      const made = drawState.addSupportsAt([a, b, c, b]);
      expect(made).toHaveLength(4);
      // One per node: a's pinned support is gone, and b listed twice holds one.
      expect(modelStore.supports.has(old)).toBe(false);
      expect(modelStore.supports.size).toBe(3);
      const byNode = new Map([...modelStore.supports.values()].map((s) => [s.nodeId, s]));
      for (const n of [a, b, c]) {
        const s = byNode.get(n)!;
        expect(s.type).toBe('custom3d');
        expect(s.kz).toBe(5000);
        expect(s.dofRestraints).toMatchObject({ tx: true, ty: true, tz: false });
      }
      // Each its own restraints, not one object the three share.
      expect(byNode.get(a)!.dofRestraints).not.toBe(byNode.get(c)!.dofRestraints);

      historyStore.undo();
      expect(modelStore.supports.size).toBe(before);
      expect(modelStore.supports.get(old)?.type).toBe('pinned3d');
      expect(historyStore.canUndo).toBe(false);
    } finally {
      d.elastic = false; d.springs.kz = undefined;
    }
  });

  it('nothing to add is no undo step', () => {
    expect(drawState.addSupportsAt([])).toEqual([]);
    expect(historyStore.canUndo).toBe(false);
  });
});
