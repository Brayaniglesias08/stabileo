/** Applied loads of the displayed static result, shared by figures and the free-body view. */
import { modelStore } from './model.svelte';
import { resultsStore } from './results.svelte';
import { uiStore } from './ui.svelte';
import { singleSolveModel, withCaseEffects } from '../engine/case-effects';
import { activeModel } from '../engine/member-behaviour';
import type { ModelData } from '../engine/solver-service';
import { scaledLoad } from '../model/loads/load-magnitudes';
import type { Load } from './model.svelte';

let memo: { key: unknown[]; loads: Load[] } | undefined;

/** Envelopes and magnitude/advanced results have no single set of static applied loads. */
export function displayedLoads3D(): Load[] {
  const r = resultsStore.results3D;
  if (!r) return modelStore.loads;
  if (r.magnitude || resultsStore.activeView === 'envelope'
    || r === resultsStore.spectralResult3D?.results || r === resultsStore.pdeltaResult3D?.results) return [];

  // Match the actual result too: a missing selection may fall back to the single solve.
  const caseId = resultsStore.activeView === 'single' ? resultsStore.activeCaseId : null;
  const comboId = resultsStore.activeView === 'combo' ? resultsStore.activeComboId : null;
  const key = [r, caseId, comboId, modelStore.modelVersion, modelStore.loads,
    modelStore.loadCases, modelStore.combinations, uiStore.includeSelfWeight, uiStore.axisConvention3D];
  if (memo && key.every((v, i) => v === memo!.key[i])) return memo.loads;

  const base = activeModel(modelStore.model as unknown as ModelData);
  let loads: Load[] = [];
  const combo = comboId !== null && resultsStore.perCombo3D.get(comboId) === r
    ? modelStore.combinations.find(c => c.id === comboId) : undefined;
  const isCase = caseId !== null && resultsStore.perCase3D.get(caseId) === r;
  if (isCase || combo) {
    const cases = withCaseEffects(base, modelStore.loadCases, {
      includeSelfWeight: uiStore.includeSelfWeight, leftHand: uiStore.axisConvention3D === 'leftHand',
    });
    const factors = new Map<number, number>();
    for (const f of combo?.factors ?? [{ caseId: caseId!, factor: 1 }]) {
      factors.set(f.caseId, (factors.get(f.caseId) ?? 0) + f.factor);
    }
    loads = cases.loads.flatMap(l => {
      const k = factors.get(l.data.caseId ?? 1) ?? 0;
      return k === 0 ? [] : [k === 1 ? l : scaledLoad(l, k)];
    });
  } else if (r === resultsStore.singleResults3D) {
    loads = singleSolveModel(base, modelStore.loadCases).loads;
  }
  // Case expansion can generate notional loads; do it once per model/result, not per camera frame.
  memo = { key, loads };
  return loads;
}
