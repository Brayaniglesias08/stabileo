/**
 * "Re-run analysis and design", offered by the re-check notice (`store/recheck.svelte.ts`).
 *
 * The analysis is the manual solve every other entry point uses (`runGlobalSolve`), so the re-run
 * is exactly what pressing Calcular does; its publish stamps the project with this build and ends
 * the stale state. The design re-run is the RC code check over the provided reinforcement (PRO),
 * NOT the auto-design: re-designing would overwrite reinforcement the engineer chose, and the
 * question here is whether what they chose still passes under the changed checks. The steel
 * checks read the new results by themselves.
 */
import { uiStore, resultsStore } from '../store';
import { designRunStore } from '../store/design-run.svelte';
import { recheckStore } from '../store/recheck.svelte';
import { runGlobalSolve } from '../engine/live-calc';
import { RESULTS_VERSION } from '../provenance/registry';
import { t, tp } from '../i18n';
import { errorText } from '../utils/error-text';

export async function rerunAnalysisAndDesign(): Promise<{ ok: boolean; error?: string }> {
  uiStore.toast(t('recheck.rerunning'), 'info');
  try {
    await runGlobalSolve();
  } catch (e) {
    const error = errorText(e, 'error');
    uiStore.toast(tp('recheck.rerunFailed', { error }), 'error');
    return { ok: false, error };
  }
  const solved = uiStore.is3DWorkspace ? !!resultsStore.results3D : !!resultsStore.results;
  if (!solved || recheckStore.stale) {
    const error = t('results.emptyModelError');
    uiStore.toast(tp('recheck.rerunFailed', { error }), 'error');
    return { ok: false, error };
  }
  if (uiStore.analysisMode === 'pro' && resultsStore.results3D) {
    // A refusal here (no combinations, no concrete members) is not a failed re-run: there was
    // no RC check to repeat. The design panel says why when it is opened.
    designRunStore.runCodeCheck();
  }
  uiStore.toast(tp('recheck.rerunDone', { v: RESULTS_VERSION }), 'success');
  return { ok: true };
}
