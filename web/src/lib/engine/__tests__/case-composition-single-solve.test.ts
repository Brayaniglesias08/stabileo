/**
 * The solve of every load without combinations, and everything that reads it, by one rule
 * (`case-effects.ts` `singleSolveModel`): each listed case's own loads once, reduced where a
 * reduction applies; a reference case, or one not solved alone, out unless a composite case takes
 * it in, and then through that composite times its factor; a composite's listed parts already
 * there, once; notional loads not worked out. The statics check of that solve, the load at a
 * slab–column joint for the active result set, and PRO's "All loads" after a solve with
 * combinations read the loads by the same rule. And with every active combination an SRSS or ABS
 * one, footings, joints and floors never read a magnitude as the result set.
 */
import { describe, it, expect, beforeAll, beforeEach, afterEach, vi } from 'vitest';
import { modelStore } from '../../store/model.svelte';
import { historyStore } from '../../store/history.svelte';
import { uiStore } from '../../store/ui.svelte';
import { resultsStore } from '../../store/results.svelte';
import '../../store';
import { initSolver } from '../wasm-solver';
import { publishCombinations3D, activeResultSet3D, noLinearCombinationActive } from '../../store/active-results';
import { designRunStore } from '../../store/design-run.svelte';
import { staticsRows, BALANCED } from '../../store/statics-rows';
import { nodalLoadAtJoint, collectFootingReactions } from '../../store/detailing-footing-inputs';
import { collectStresses } from '../../store/detailing-floor-inputs';
import { jointDesignStore } from '../../store/joint-design.svelte';
import { runGlobalSolve } from '../live-calc';
import { reductionApplies } from '../case-effects';
import type { AnalysisResults3D } from '../types-3d';

beforeAll(async () => { await initSolver(); });
beforeEach(() => {
  modelStore.clear(); historyStore.clear(); modelStore.setResultScopes(null); resultsStore.clear();
  for (const c of [...modelStore.combinations]) modelStore.removeCombination(c.id);
  uiStore.analysisMode = 'pro';
  uiStore.includeSelfWeight = false;
});
afterEach(() => { uiStore.analysisMode = '3d'; uiStore.includeSelfWeight = false; vi.restoreAllMocks(); });

const REDUCED = { ratio: 0.6, tributaryAreaM2: 80, elementKind: 'interiorBeam', floorsSupported: 1 } as const;
const sumFz = (r: AnalysisResults3D) => r.reactions.reduce((s, x) => s + x.fz, 0);
const sumFx = (r: AnalysisResults3D) => r.reactions.reduce((s, x) => s + x.fx, 0);

/** The single solve, published as the app publishes it. */
function single(): AnalysisResults3D {
  const r = modelStore.solve3D(uiStore.includeSelfWeight, false, true);
  if (!r || typeof r === 'string') throw new Error(String(r));
  resultsStore.setResults3D(r);
  return r;
}

/** A 6 m × 4 m portal in XZ, fixed feet; 20 kN/m on the beam in D, 15 kN sideways in W. */
function portal() {
  const n1 = modelStore.addNode(0, 0, 0), n2 = modelStore.addNode(6, 0, 0);
  const n3 = modelStore.addNode(0, 0, 4), n4 = modelStore.addNode(6, 0, 4);
  const c1 = modelStore.addElement(n1, n3), beam = modelStore.addElement(n3, n4), c2 = modelStore.addElement(n2, n4);
  for (const n of [n1, n2]) modelStore.addSupport(n, 'fixed3d');
  const D = modelStore.model.loadCases[0]!.id;
  const W = modelStore.addLoadCase('W', 'W');
  modelStore.addDistributedLoad3D(beam, 0, 0, -20, -20, undefined, undefined, D);
  modelStore.addNodalLoad3D(n3, 15, 0, 0, 0, 0, 0, W);
  return { n1, n2, n3, n4, c1, beam, c2, D, W };
}

describe('the single solve', () => {
  it('a reference case taken in only by a composite enters through it, times its factor', () => {
    const { n3 } = portal();
    const R = modelStore.addLoadCase('R', 'D');
    modelStore.addNodalLoad3D(n3, 0, 0, -100, 0, 0, 0, R);
    modelStore.updateLoadCaseFields(R, { reference: true });
    const C = modelStore.addLoadCase('C', '');
    modelStore.updateLoadCaseFields(C, { includes: [{ caseId: R, factor: 1.2 }] });
    // D's 120 kN on the beam, and R's 100 kN through C at 1,2.
    expect(sumFz(single())).toBeCloseTo(120 + 120, 6);
  });

  it('a composite’s listed parts are there once; it adds its own loads', () => {
    const { n4, D, W } = portal();
    const C = modelStore.addLoadCase('C', '');
    modelStore.updateLoadCaseFields(C, { includes: [{ caseId: D, factor: 1.2 }, { caseId: W, factor: 1.6 }] });
    modelStore.addNodalLoad3D(n4, 0, 0, -10, 0, 0, 0, C);
    const r = single();
    expect(sumFz(r)).toBeCloseTo(120 + 10, 6);
    expect(sumFx(r)).toBeCloseTo(-15, 6);
  });

  it('a reduction on a case typed other than L is not applied (Lr keeps its loads whole)', () => {
    const { n4 } = portal();
    const L = modelStore.addLoadCase('L', 'L');
    modelStore.addNodalLoad3D(n4, 0, 0, -50, 0, 0, 0, L);
    modelStore.updateLoadCaseFields(L, { reduction: { ...REDUCED } });
    expect(sumFz(single())).toBeCloseTo(120 + 30, 6);
    modelStore.updateLoadCaseType(L, 'Lr');
    expect(modelStore.model.loadCases.find((c) => c.id === L)!.reduction?.ratio).toBe(0.6);
    expect(reductionApplies(modelStore.model.loadCases.find((c) => c.id === L)!)).toBe(false);
    expect(sumFz(single())).toBeCloseTo(120 + 50, 6);
    // And in the cases a combination solves.
    modelStore.addCombination('Lr', [{ caseId: L, factor: 1 }]);
    const b = modelStore.solveCombinations3D(false, false, true);
    if (!b || typeof b === 'string') throw new Error(String(b));
    expect(sumFz(b.perCase.get(L)!)).toBeCloseTo(50, 6);
  });
});

describe('the statics check of the single solve reads the loads the solve had', () => {
  it('a composite case taking D: applied −120 against 120', () => {
    const { D } = portal();
    const C = modelStore.addLoadCase('C', '');
    modelStore.updateLoadCaseFields(C, { includes: [{ caseId: D, factor: 1 }] });
    single();
    const rows = staticsRows()!;
    expect(rows.cases).toHaveLength(1);
    expect(rows.cases[0]!.applied.fz).toBeCloseTo(-120, 6);
    expect(rows.cases[0]!.worstRelative).toBeLessThan(BALANCED);
  });

  it('the older self-weight switch with three D cases and a reduced L: the weight once', () => {
    const { n4, beam } = portal();
    expect(modelStore.model.analysis?.selfWeight).toBeUndefined();
    uiStore.includeSelfWeight = true;
    for (const name of ['D2', 'D3']) {
      const k = modelStore.addLoadCase(name, 'D');
      modelStore.addDistributedLoad3D(beam, 0, 0, -1, -1, undefined, undefined, k);
    }
    const L = modelStore.addLoadCase('L', 'L');
    modelStore.addNodalLoad3D(n4, 0, 0, -10, 0, 0, 0, L);
    modelStore.updateLoadCaseFields(L, { reduction: { ...REDUCED } });
    single();
    const row = staticsRows()!.cases[0]!;
    expect(row.selfWeightIncluded).toBe(true);
    expect(row.worstRelative).toBeLessThan(BALANCED);
  });

  it('a reference case not taken in and a notional case: neither applied', () => {
    const { n4, D } = portal();
    const R = modelStore.addLoadCase('R', 'W');
    modelStore.addNodalLoad3D(n4, 7, 0, 0, 0, 0, 0, R);
    modelStore.updateLoadCaseFields(R, { reference: true });
    const N = modelStore.addLoadCase('N', 'D');
    modelStore.updateLoadCaseFields(N, { notional: { sourceCaseId: D, ratio: 0.01, dir: '+X' } });
    const r = single();
    expect(sumFx(r)).toBeCloseTo(-15, 6);
    const row = staticsRows()!.cases[0]!;
    expect(row.applied.fx).toBeCloseTo(15, 6);
    expect(row.worstRelative).toBeLessThan(BALANCED);
  });
});

describe('the load at a slab–column joint for the active result set', () => {
  it('is the single solve’s: the reference case out, the reduced case reduced', () => {
    const n = modelStore.addNode(0, 0, 3);
    const D = modelStore.model.loadCases[0]!.id;
    const R = modelStore.addLoadCase('R', 'D'), L = modelStore.addLoadCase('L', 'L');
    modelStore.addNodalLoad3D(n, 0, 0, -100, 0, 0, 0, D);
    modelStore.addNodalLoad3D(n, 0, 0, -40, 0, 0, 0, R);
    modelStore.addNodalLoad3D(n, 0, 0, -50, 0, 0, 0, L);
    modelStore.updateLoadCaseFields(R, { reference: true });
    modelStore.updateLoadCaseFields(L, { reduction: { ...REDUCED } });
    expect(nodalLoadAtJoint(n, 0)).toBeCloseTo(130, 9);
    // Taken in by a composite, the reference case is in, times the factor.
    const C = modelStore.addLoadCase('C', '');
    modelStore.updateLoadCaseFields(C, { includes: [{ caseId: R, factor: 0.5 }] });
    expect(nodalLoadAtJoint(n, 0)).toBeCloseTo(150, 9);
  });
});

describe('PRO’s "All loads" after a solve with combinations', () => {
  /** A solve as the app runs it, the workers' combination solve replaced by the same sync one. */
  async function solveAsApp() {
    vi.spyOn(modelStore, 'solveCombinations3DParallel').mockImplementation(async (sw, lh, pro) => modelStore.solveCombinations3D(sw, lh, pro));
    await runGlobalSolve();
    const all = resultsStore.singleResults3D;
    if (!all) throw new Error('no result');
    return all;
  }

  it('counts a composite’s parts once, and takes a reference case through it', async () => {
    const { n4, D, W } = portal();
    const R = modelStore.addLoadCase('R', 'D');
    modelStore.addNodalLoad3D(n4, 0, 0, -100, 0, 0, 0, R);
    modelStore.updateLoadCaseFields(R, { reference: true });
    const C = modelStore.addLoadCase('C', '');
    modelStore.updateLoadCaseFields(C, { includes: [{ caseId: D, factor: 1.2 }, { caseId: W, factor: 1.6 }, { caseId: R, factor: 0.5 }] });
    modelStore.addCombination('D + W', [{ caseId: D, factor: 1 }, { caseId: W, factor: 1 }]);
    const all = await solveAsApp();
    expect(resultsStore.perCombo3D.size).toBe(1);
    expect(sumFz(all)).toBeCloseTo(120 + 50, 6);
    expect(sumFx(all)).toBeCloseTo(-15, 6);
    const direct = modelStore.solve3D(false, false, true);
    if (!direct || typeof direct === 'string') throw new Error(String(direct));
    expect(sumFz(all)).toBeCloseTo(sumFz(direct), 6);
  });

  it('a notional case: its loads are not worked out, as in the single solve', async () => {
    const { D } = portal();
    const N = modelStore.addLoadCase('N', 'D');
    modelStore.updateLoadCaseFields(N, { notional: { sourceCaseId: D, ratio: 0.01, dir: '+X' } });
    modelStore.addCombination('D + N', [{ caseId: D, factor: 1 }, { caseId: N, factor: 1 }]);
    const all = await solveAsApp();
    expect(sumFx(all)).toBeCloseTo(-15, 6);
  });

  it('the older self-weight switch with two D cases: the weight once', async () => {
    const { beam, D } = portal();
    uiStore.includeSelfWeight = true;
    const D2 = modelStore.addLoadCase('D2', 'D');
    modelStore.addDistributedLoad3D(beam, 0, 0, -1, -1, undefined, undefined, D2);
    modelStore.addCombination('D + D2', [{ caseId: D, factor: 1 }, { caseId: D2, factor: 1 }]);
    const all = await solveAsApp();
    const direct = modelStore.solve3D(true, false, true);
    if (!direct || typeof direct === 'string') throw new Error(String(direct));
    expect(sumFz(all)).toBeCloseTo(sumFz(direct), 6);
  });
});

describe('with every active combination an SRSS one, no magnitude is read as the result set', () => {
  function srssOnly() {
    const p = portal();
    const s = modelStore.addCombination('D, W srss', [{ caseId: p.D, factor: 1 }, { caseId: p.W, factor: 1 }]);
    modelStore.updateCombination(s, { method: 'srss' });
    return { ...p, s };
  }
  /** Solved as the app does: "All loads", then the combinations, with the SRSS one on screen. */
  function solveAndShow(s: number, withSingle: boolean) {
    const all = withSingle ? single() : null;
    const b = modelStore.solveCombinations3D(false, false, true);
    if (!b || typeof b === 'string') throw new Error(String(b));
    publishCombinations3D(b);
    resultsStore.activeView = 'combo';
    resultsStore.activeComboId = s;
    expect(resultsStore.results3D?.magnitude?.method).toBe('srss');
    return all;
  }

  it('footings read the signed single solve, or nothing', () => {
    const { n1, s } = srssOnly();
    modelStore.addFooting(n1, 'Z1');
    const all = solveAndShow(s, true)!;
    expect(activeResultSet3D()).not.toBeNull();
    expect(activeResultSet3D()!.magnitude).toBeUndefined();
    const f = collectFootingReactions().get(n1)!.factored;
    expect(f).toHaveLength(1);
    expect(f[0]!.fz).toBeCloseTo(all.reactions.find((r) => r.nodeId === n1)!.fz, 6);
  });

  it('with no single solve on hand, footings and joints read nothing', () => {
    const { n1, n3, c1, beam, s } = srssOnly();
    modelStore.addFooting(n1, 'Z1');
    solveAndShow(s, false);
    expect(activeResultSet3D()).toBeNull();
    expect(collectFootingReactions().size).toBe(0);
    expect(jointDesignStore.designFor(n3, [c1, beam]).demands.combinationsConsidered).toBe(0);
  });

  it('joints read the signed single solve', () => {
    const { n3, c1, beam, s } = srssOnly();
    const all = solveAndShow(s, true)!;
    const d = jointDesignStore.designFor(n3, [c1, beam]).demands;
    expect(d.combinationsConsidered).toBe(1);
    const n = all.elementForces.find((f) => f.elementId === c1)!;
    expect(d.axial!.value).toBeCloseTo(Math.max(Math.abs(n.nStart), Math.abs(n.nEnd)), 6);
  });

  it('design says no linear combination is active, rather than that no demands were found', () => {
    const { s } = srssOnly();
    solveAndShow(s, true);
    expect(noLinearCombinationActive()).toBe(true);
    expect(designRunStore.computeDemands()).toMatchObject({ ok: false, reasonKey: 'design.error.noLinearCombination' });
  });

  it('floors read the signed single solve', () => {
    const { s } = srssOnly();
    const a = modelStore.addNode(0, 2, 4), b = modelStore.addNode(6, 2, 4);
    const n3 = [...modelStore.nodes.values()].find((n) => n.x === 0 && n.z === 4)!.id;
    const n4 = [...modelStore.nodes.values()].find((n) => n.x === 6 && n.z === 4)!.id;
    modelStore.addSupport(a, 'fixed3d'); modelStore.addSupport(b, 'fixed3d');
    const material = [...modelStore.model.materials.keys()][0]!;
    const quad = modelStore.addQuad([n3, n4, b, a], material, 0.2);
    const all = solveAndShow(s, true)!;
    const q = collectStresses().find((x) => x.elementId === quad)!;
    const own = all.quadStresses!.find((x) => x.elementId === quad)!;
    expect(q.mx).toBeCloseTo(own.mx, 9);
  });
});
