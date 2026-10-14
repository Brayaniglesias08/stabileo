/**
 * Load cases and combinations in Basic: on or off for the model in hand.
 *
 * Off, Basic shows one load case: the load bar has no case picker, the loads table no case column,
 * the Combinations fold only self-weight, and results are the loads together (no case, combination
 * or envelope). On, all of it shows. Each model that comes in (an example, a file, a link, a tab)
 * opens with them on when its loads use more than one case (`store/load-cases-mode.ts`).
 */
import { modelStore, resultsStore, uiStore } from '../store';
import { hasCurrentResults, runSolve } from './solve';

export { adoptLoadCases } from '../store/load-cases-mode';

export function setLoadCasesEnabled(on: boolean): void {
  if (uiStore.loadCasesEnabled === on) return;
  uiStore.loadCasesEnabled = on;
  if (!on) {
    // Nothing on screen names a case any more: new loads go to the first one, and the drawing
    // shows the loads together.
    uiStore.activeLoadCaseId = modelStore.loadCases[0]?.id ?? 1;
    resultsStore.activeCaseId = null;
    resultsStore.activeView = 'single';
    resultsStore.setOverlay(null);
    resultsStore.setOverlay3D(null);
    return;
  }
  // A model solved while they were off has no combinations solved: solve it again for them.
  if (hasCurrentResults()) runSolve();
}
