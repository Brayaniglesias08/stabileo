/**
 * Editing a self-weight rule's case or direction in the load tables can make it the same as
 * another rule (case, direction and reach): the two become one, with the edited row's factor, as
 * the Add load card replaces one. Two such rows counted the weight twice.
 */
import { describe, it, expect, beforeAll, beforeEach } from 'vitest';
import { modelStore } from '../model.svelte';
import { uiStore } from '../ui.svelte';
import '../index';
import { initSolver } from '../../engine/wasm-solver';
import { editSelfWeightRule, sameSelfWeightRule, type SelfWeightLoad } from '../../engine/analysis-settings';

beforeAll(async () => { await initSolver(); });
beforeEach(() => { uiStore.analysisMode = 'pro'; uiStore.includeSelfWeight = true; modelStore.clear(); });

const Z = (caseId: number, factor = -1, more: Partial<SelfWeightLoad> = {}): SelfWeightLoad => ({ caseId, direction: 'Z', factor, ...more });

describe('editSelfWeightRule', () => {
  it('a case that makes a row the same as another: one row, in the other\'s place, with the edited factor', () => {
    const rules = [Z(1), Z(2, -1.5), Z(3)];
    const r = editSelfWeightRule(rules, 1, { caseId: 1 });
    expect(r.merged).toBe(true);
    expect(r.rules).toEqual([Z(1, -1.5), Z(3)]);
  });

  it('a direction too; a different reach is a different rule, and a factor alone never merges', () => {
    const rules = [Z(1), { caseId: 1, direction: 'X' as const, factor: 0.1 }, Z(1, -1, { groupId: 4 })];
    expect(editSelfWeightRule(rules, 1, { direction: 'Z' })).toEqual({ merged: true, rules: [{ caseId: 1, direction: 'Z', factor: 0.1 }, Z(1, -1, { groupId: 4 })] });
    expect(editSelfWeightRule(rules, 2, { factor: -2 })).toEqual({ merged: false, rules: [rules[0], rules[1], Z(1, -2, { groupId: 4 })] });
    expect(editSelfWeightRule([Z(1), Z(2, -1, { groupId: 4 })], 1, { caseId: 1 }).merged).toBe(false);
  });

  it('the same members in another order are the same reach', () => {
    expect(sameSelfWeightRule(Z(1, -1, { elements: [3, 1, 2] }), Z(1, -2, { elements: [1, 2, 3] }))).toBe(true);
    expect(sameSelfWeightRule(Z(1, -1, { elements: [1, 2] }), Z(1, -1))).toBe(false);
  });
});

describe('the weight is counted once', () => {
  it('two rows edited into one case weigh the column once', () => {
    const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(0, 0, 3);
    modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'fixed3d');
    const sumRz = () => {
      const r = modelStore.solve3D(true, false, true);
      if (!r || typeof r === 'string') throw new Error(String(r));
      return r.reactions.reduce((s, x) => s + x.fz, 0);
    };
    modelStore.setAnalysis({ selfWeight: [Z(1)] });
    const once = sumRz();
    expect(once).toBeGreaterThan(1e-6);
    modelStore.setAnalysis({ selfWeight: [Z(1), Z(2)] });
    modelStore.setAnalysis({ selfWeight: editSelfWeightRule(modelStore.analysis!.selfWeight!, 1, { caseId: 1 }).rules });
    expect(modelStore.analysis!.selfWeight).toHaveLength(1);
    expect(sumRz()).toBeCloseTo(once, 9);
  });
});
