/**
 * Basic's load cases switch, set for each model that comes in (an example, a file, a link, a tab):
 * on when its loads use more than one case. Here, beside the stores, so the funnels in the store
 * layer reach it without the actions layer (`actions/load-cases.ts` turns it on and off by hand).
 */
import { modelStore } from './model.svelte';
import { uiStore } from './ui.svelte';
import { loadCasesInUse } from '../model/loads/load-cases-in-use';

export function adoptLoadCases(): void {
  uiStore.loadCasesEnabled = loadCasesInUse(modelStore.model.loads);
}
