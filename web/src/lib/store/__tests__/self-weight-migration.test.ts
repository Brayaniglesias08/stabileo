/**
 * An older PRO project is given its self-weight rule once, from what it computed before: the
 * weight in its first dead-load case (the older rule counted it in every one), a case made for it
 * when there is none, nothing when self-weight was off. It is not the user's edit, so it takes no
 * undo step; and Basic, which has its own toggle, does not read the rule.
 */
import { describe, it, expect, beforeAll, beforeEach } from 'vitest';
import { modelStore } from '../model.svelte';
import { uiStore } from '../ui.svelte';
import { historyStore } from '../history.svelte';
import '../index';
import { leaveSelfWeightRule, migrateSelfWeightIfNeeded, planSelfWeight, selfWeightRuleEffect } from '../self-weight-migration';
import { initSolver } from '../../engine/wasm-solver';
import { t } from '../../i18n';

beforeAll(async () => { await initSolver(); });
beforeEach(() => { uiStore.analysisMode = 'pro'; uiStore.includeSelfWeight = true; uiStore.selfWeightCaseId = null; modelStore.clear(); historyStore.clear(); });

/** A column with a load, and the cases asked for (the default model's cases removed). */
function project(types: string[]) {
  const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(0, 0, 3);
  modelStore.addElement(a, b, 'frame');
  modelStore.addSupport(a, 'fixed3d');
  for (const c of [...modelStore.combinations]) modelStore.removeCombination(c.id);
  for (const c of [...modelStore.model.loadCases]) modelStore.removeLoadCase(c.id);
  const ids = types.map((ty, i) => modelStore.addLoadCase(`${ty}${i}`, ty as never));
  modelStore.addNodalLoad3D(b, 0, 0, -10, 0, 0, 0, ids[0] ?? 1);
  // The rule the store may have been given while building: an older file has none.
  modelStore.withoutUndo(() => modelStore.setAnalysis({ selfWeight: undefined }));
  historyStore.clear();
  return { ids, head: b };
}
const lastToast = () => uiStore.toasts[uiStore.toasts.length - 1]?.message ?? '';

describe('the plan', () => {
  it('the first dead-load case; none when off; the count of dead-load cases for the notice', () => {
    const cases = [{ id: 3, type: 'L' }, { id: 5, type: 'D' }, { id: 7, type: 'D' }];
    expect(planSelfWeight(cases, true)).toMatchObject({ caseId: 5, deadCases: 2, selfWeight: [{ caseId: 5, direction: 'Z', factor: -1 }] });
    expect(planSelfWeight(cases, false).selfWeight).toEqual([]);
    expect(planSelfWeight([{ id: 1, type: 'L' }], true)).toMatchObject({ caseId: null, selfWeight: [] });
  });
});

describe('the migration', () => {
  it('one dead-load case: the rule goes there, with the plain notice, and no undo step', () => {
    const { ids } = project(['D', 'L']);
    expect(migrateSelfWeightIfNeeded()).toBe(true);
    expect(modelStore.analysis!.selfWeight).toEqual([{ caseId: ids[0], direction: 'Z', factor: -1 }]);
    expect(lastToast()).toContain(t('selfWeight.migrated').split('{')[0]!.trim().slice(0, 12));
    expect(historyStore.canUndo).toBe(false);
  });

  it('two dead-load cases: the first one, and the notice says the weight used to be in both', () => {
    project(['D', 'D']);
    migrateSelfWeightIfNeeded();
    expect(modelStore.analysis!.selfWeight).toHaveLength(1);
    expect(lastToast()).toContain('2');
  });

  it('no dead-load case: one is made for it, still without an undo step', () => {
    project(['L']);
    const before = modelStore.model.loadCases.length;
    migrateSelfWeightIfNeeded();
    expect(modelStore.model.loadCases.length).toBe(before + 1);
    const made = modelStore.model.loadCases[modelStore.model.loadCases.length - 1]!;
    expect(made.type).toBe('D');
    expect(modelStore.analysis!.selfWeight![0]!.caseId).toBe(made.id);
    expect(historyStore.canUndo).toBe(false);
  });

  it('from Basic, the case its reader chose for self-weight, not the first dead-load one', () => {
    const { ids } = project(['D', 'D', 'L']);
    uiStore.selfWeightCaseId = ids[1]!;
    migrateSelfWeightIfNeeded();
    expect(modelStore.analysis!.selfWeight).toEqual([{ caseId: ids[1], direction: 'Z', factor: -1 }]);
    // Chosen, but a case the model no longer has: the plan's first dead-load case.
    modelStore.withoutUndo(() => modelStore.setAnalysis({ selfWeight: undefined }));
    uiStore.selfWeightCaseId = 999;
    migrateSelfWeightIfNeeded();
    expect(modelStore.analysis!.selfWeight![0]!.caseId).toBe(ids[0]);
  });

  it('self-weight off: a stated empty rule, and it runs once', () => {
    uiStore.includeSelfWeight = false;
    project(['D']);
    expect(migrateSelfWeightIfNeeded()).toBe(true);
    expect(modelStore.analysis!.selfWeight).toEqual([]);
    expect(migrateSelfWeightIfNeeded()).toBe(false);
  });
});

describe('a new PRO project', () => {
  it('starts without self-weight, and drawing does not add it; a project with members is migrated', () => {
    modelStore.withoutUndo(() => modelStore.setAnalysis({ selfWeight: undefined }));
    historyStore.clear();
    selfWeightRuleEffect();
    expect(modelStore.analysis?.selfWeight).toBeUndefined();
    expect(historyStore.canUndo).toBe(false);
    const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(4, 0, 0);
    modelStore.addElement(a, b, 'frame');
    selfWeightRuleEffect();
    expect(modelStore.analysis!.selfWeight).toEqual([]);

    // An older project, with members and no rule: the weight it computed before, as a rule.
    const { ids } = project(['D']);
    selfWeightRuleEffect();
    expect(modelStore.analysis!.selfWeight).toEqual([{ caseId: ids[0], direction: 'Z', factor: -1 }]);
  });
});

/** ΣRz of a 3D solve, PRO (with shells) or Basic. */
function sumRz(isPro: boolean): number {
  const r = modelStore.solve3D(uiStore.includeSelfWeight, false, isPro);
  if (!r || typeof r === 'string') throw new Error(String(r));
  return r.reactions.reduce((s, x) => s + x.fz, 0);
}

describe('an older project made only of shells', () => {
  it('keeps its self-weight: the slab is structure, and the rule goes in its first D case', () => {
    // A 4×4 m slab of four quads on its corners, with 1 kN at the middle.
    const n: number[][] = [];
    for (let i = 0; i <= 2; i++) { n.push([]); for (let j = 0; j <= 2; j++) n[i]!.push(modelStore.addNode(2 * i, 2 * j, 0)); }
    for (let i = 0; i < 2; i++) for (let j = 0; j < 2; j++) modelStore.addQuad([n[i]![j]!, n[i + 1]![j]!, n[i + 1]![j + 1]!, n[i]![j + 1]!], 1, 0.2);
    for (const c of [n[0]![0]!, n[2]![0]!, n[0]![2]!, n[2]![2]!]) modelStore.addSupport(c, 'pinned3d');
    for (const c of [...modelStore.combinations]) modelStore.removeCombination(c.id);
    for (const c of [...modelStore.model.loadCases]) modelStore.removeLoadCase(c.id);
    const d = modelStore.addLoadCase('D', 'D');
    modelStore.addNodalLoad3D(n[1]![1]!, 0, 0, -1, 0, 0, 0, d);
    modelStore.withoutUndo(() => modelStore.setAnalysis({ selfWeight: undefined }));
    expect(modelStore.elements.size).toBe(0);
    selfWeightRuleEffect();
    expect(modelStore.analysis!.selfWeight).toEqual([{ caseId: d, direction: 'Z', factor: -1 }]);
    // 1 kN and the slab's weight (16 m² · 0.2 m of concrete), not the 1 kN alone.
    expect(sumRz(true)).toBeGreaterThan(100);
  });
});

describe('a new PRO project states its rule when its first structure is drawn, not before', () => {
  it('seen empty in PRO, drawn in Basic, back in PRO: Basic and PRO weigh the same column', () => {
    modelStore.withoutUndo(() => modelStore.setAnalysis({ selfWeight: undefined }));
    selfWeightRuleEffect();
    // Nothing is written into an empty model: it has nothing to weigh yet.
    expect(modelStore.analysis?.selfWeight).toBeUndefined();
    // The panel unmounts with PRO.
    leaveSelfWeightRule();
    uiStore.analysisMode = '3d';
    uiStore.includeSelfWeight = true;
    const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(0, 0, 3);
    modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'fixed3d');
    const basic = sumRz(false);
    expect(basic).toBeGreaterThan(1e-6);
    uiStore.analysisMode = 'pro';
    selfWeightRuleEffect();
    expect(modelStore.analysis!.selfWeight).toEqual([{ caseId: 1, direction: 'Z', factor: -1 }]);
    expect(sumRz(true)).toBeCloseTo(basic, 6);
  });

  it('a first member or shell drawn in PRO: an empty rule, without an undo step', () => {
    modelStore.withoutUndo(() => modelStore.setAnalysis({ selfWeight: undefined }));
    historyStore.clear();
    selfWeightRuleEffect();
    expect(modelStore.analysis?.selfWeight).toBeUndefined();
    const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(4, 0, 0), c = modelStore.addNode(0, 4, 0);
    selfWeightRuleEffect();
    expect(modelStore.analysis?.selfWeight).toBeUndefined();
    modelStore.addPlate([a, b, c], 1, 0.2);
    selfWeightRuleEffect();
    expect(modelStore.analysis!.selfWeight).toEqual([]);
    // Undone, the model is empty again with no rule; redone, the empty rule comes back with it.
    historyStore.undo();
    expect(modelStore.plates.size).toBe(0);
    selfWeightRuleEffect();
    expect(modelStore.analysis?.selfWeight).toBeUndefined();
    historyStore.redo();
    expect(modelStore.analysis!.selfWeight).toEqual([]);
  });

  it('an older project opened over an empty one PRO has seen is migrated, not emptied', () => {
    selfWeightRuleEffect();
    const { ids } = project(['D']);
    const snap = modelStore.snapshot();
    modelStore.clear();
    selfWeightRuleEffect();
    modelStore.restore(snap);
    selfWeightRuleEffect();
    expect(modelStore.analysis!.selfWeight).toEqual([{ caseId: ids[0], direction: 'Z', factor: -1 }]);
  });

  it('an example opened in PRO keeps its rule when the panel runs between clearing and loading', async () => {
    const loading = modelStore.loadExample('pro-edificio-7p');
    // `loadExample` cleared the model and is awaiting the fixture: the panel's effect may run here.
    expect(modelStore.elements.size).toBe(0);
    selfWeightRuleEffect();
    await loading;
    expect(modelStore.elements.size).toBeGreaterThan(0);
    expect(modelStore.analysis!.selfWeight).toEqual([{ caseId: 1, direction: 'Z', factor: -1 }]);
  });
});

describe('Basic reads its own toggle', () => {
  it('a model that visited PRO with self-weight off still takes it in Basic when the toggle is on', () => {
    uiStore.includeSelfWeight = false;
    project(['D']);
    migrateSelfWeightIfNeeded();
    uiStore.analysisMode = '3d';
    const r = modelStore.solve3D(true, false, false);
    if (!r || typeof r === 'string') throw new Error(String(r));
    // 10 kN of load and the column's own weight.
    expect(r.reactions.reduce((s, x) => s + x.fz, 0)).toBeGreaterThan(10 + 1e-6);
  });
});

describe('Basic puts self-weight in the case chosen for it', () => {
  const sumFz = (r: { reactions: Array<{ fz?: number; rz?: number }> }) => r.reactions.reduce((s, x) => s + (x.fz ?? x.rz ?? 0), 0);

  it('3D: in the chosen case only, and the first dead-load case when none is chosen', () => {
    uiStore.analysisMode = '3d';
    const { ids } = project(['D', 'L']);
    modelStore.addCombination('C', [{ caseId: ids[0]!, factor: 1 }, { caseId: ids[1]!, factor: 1 }]);
    uiStore.selfWeightCaseId = ids[1]!;
    let r = modelStore.solveCombinations3D(true, false, false);
    if (!r || typeof r === 'string') throw new Error(String(r));
    // The load is in D; the weight now in L.
    expect(sumFz(r.perCase.get(ids[0]!)!)).toBeCloseTo(10, 6);
    expect(sumFz(r.perCase.get(ids[1]!)!)).toBeGreaterThan(1e-6);
    uiStore.selfWeightCaseId = null;
    r = modelStore.solveCombinations3D(true, false, false);
    if (!r || typeof r === 'string') throw new Error(String(r));
    expect(sumFz(r.perCase.get(ids[0]!)!)).toBeGreaterThan(10 + 1e-6);
    expect(Math.abs(sumFz(r.perCase.get(ids[1]!)!))).toBeLessThan(1e-6);
  });

  it('2D: the same rule per case', () => {
    uiStore.analysisMode = '2d';
    modelStore.clear();
    const a = modelStore.addNode(0, 0), b = modelStore.addNode(4, 0);
    modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'fixed');
    for (const c of [...modelStore.combinations]) modelStore.removeCombination(c.id);
    for (const c of [...modelStore.model.loadCases]) modelStore.removeLoadCase(c.id);
    const d = modelStore.addLoadCase('D0', 'D'), l = modelStore.addLoadCase('L1', 'L');
    modelStore.addNodalLoad(b, 0, -10, 0, d);
    modelStore.addNodalLoad(b, 0, -5, 0, l);
    modelStore.addCombination('C', [{ caseId: d, factor: 1 }, { caseId: l, factor: 1 }]);
    const vertical = (res: { reactions: Array<{ rz?: number; ry?: number }> }) => res.reactions.reduce((s, x) => s + (x.rz ?? x.ry ?? 0), 0);
    uiStore.selfWeightCaseId = l;
    const r = modelStore.solveCombinations(true);
    if (!r || typeof r === 'string') throw new Error(String(r));
    expect(vertical(r.perCase.get(d)!)).toBeCloseTo(10, 6);
    expect(vertical(r.perCase.get(l)!)).toBeGreaterThan(5 + 1e-6);
    uiStore.selfWeightCaseId = null;
  });
});

describe('a new project starts over', () => {
  it('forgets the self-weight case and what was hidden, but undo keeps them', async () => {
    const { viewVisibility } = await import('../view-state.svelte');
    uiStore.analysisMode = '3d';
    const { ids } = project(['D', 'L']);
    uiStore.selfWeightCaseId = ids[1]!;
    viewVisibility.hide({ nodes: [], elements: [...modelStore.elements.keys()], shells: [] });
    modelStore.addNode(5, 5, 5);
    historyStore.undo();
    expect(uiStore.selfWeightCaseId).toBe(ids[1]!);
    expect(viewVisibility.active).toBe(true);
    modelStore.clear();
    expect(uiStore.selfWeightCaseId).toBeNull();
    expect(viewVisibility.active).toBe(false);
  });
});
