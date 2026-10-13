/**
 * Solving, as an action rather than a component method.
 *
 * `handleSolve` lived inside ToolbarResults, so the only way to solve was to
 * open the results panel and press the button in it. The ribbon's Solve command
 * therefore could not solve — it opened the panel and asked the user to press
 * Solve again, which is two clicks and a detour for the single most-used action
 * in the application.
 *
 * Moving it here gives one implementation with one set of 3D handling, NaN
 * checks and toasts, callable from anywhere. Duplicating it into the ribbon
 * would have been the other option, and two copies of a solver entry point is
 * how they drift apart.
 */

import { uiStore, resultsStore, modelStore } from '../store';
import { instability } from '../store/instability.svelte';
import { publishCombinations3D } from '../store/active-results';
import { t } from '../i18n';
import { hasInvalid2DDisplacements, hasInvalid3DDisplacements } from '../geometry/coordinate-system';
import { initSolver, isWasmReady } from '../engine/wasm-solver';
import { reportModelDiagnostics } from '../engine/solve-diagnostics';
import { requestAutosave } from '../store/autosave-service';
import { syncDefinedLoadsForAnalysis } from '../store/defined-loads';

/** How a solve ended: with results, or with the message the reader was shown. */
export type SolveOutcome = { ok: true } | { ok: false; error: string };

export function runSolve(): void {
  /*
   * This is the solve entry point the Basic UI actually uses — Enter, the
   * ribbon's Solve command and Calcular all land here, NOT in
   * `runGlobalSolve` (which serves the `stabileo-solve` event path). The
   * pre-solve hygiene report therefore has to fire here too, or the user in
   * issue #181 keeps getting numbers in silence on the paths they take.
   * One call covers both branches: the 3D arm below delegates to
   * `runSolve3D`, which has no other caller. The PRO exclusion is inside the
   * reporter itself.
   */
  reportModelDiagnostics();
  if (uiStore.is3DWorkspace) {
    void runSolve3D();
    return;
  }
  runSolve2D();
}

/** Whether the results on hand are the current model's: an edit clears them. */
export function hasCurrentResults(): boolean {
  return uiStore.is3DWorkspace ? !!resultsStore.results3D : !!resultsStore.results;
}

/**
 * Solve unless the results on hand are already the model's, and say how it went.
 *
 * For the exports: a workbook or a report asked for before Calcular solves first, rather than
 * coming out without results and without saying why. The failure is the message the reader was
 * also shown as a toast, so the export can carry it.
 */
export async function ensureResults(): Promise<SolveOutcome> {
  if (hasCurrentResults()) return { ok: true };
  reportModelDiagnostics();
  return uiStore.is3DWorkspace ? runSolve3D() : runSolve2D();
}

function runSolve2D(): SolveOutcome {
  const results = modelStore.solve(uiStore.includeSelfWeight, uiStore.drawPlane2D);
  if (typeof results === 'string') {
    uiStore.toast(results, 'error');
    return { ok: false, error: results };
  } else if (results) {
    // Validate results aren't degenerate
    const hasNaN = hasInvalid2DDisplacements(results.displacements);
    if (hasNaN) {
      uiStore.toast(t('results.numericError'), 'error');
      return { ok: false, error: t('results.numericError') };
    }
    resultsStore.setResults(results);
    // Show classification in success toast
    const kin = modelStore.kinematicResult;
    let classText = '';
    if (kin) {
      if (kin.classification === 'isostatic') classText = t('toast.isostatic');
      else if (kin.classification === 'hyperstatic') classText = t('toast.hyperstatic').replace('{degree}', String(kin.degree));
    }
    // Auto-solve combinations if they exist
    let comboText = '';
    if (modelStore.model.combinations.length > 0) {
      const comboResult = modelStore.solveCombinations(uiStore.includeSelfWeight, uiStore.drawPlane2D);
      if (comboResult && typeof comboResult !== 'string') {
        resultsStore.setCombinationResults(comboResult.perCase, comboResult.perCombo, comboResult.envelope);
        comboText = t('toast.plusCombinations').replace('{n}', String(comboResult.perCombo.size));
      }
    }
    // Show diagnostics warnings if present
    const diagWarnings = [
      ...(results.diagnostics ?? []).filter(d => d.metric === 'negative_jacobian').map(d => d.message),
      ...(results.solverDiagnostics ?? []).filter(d => d.severity === 'warning').map(d => d.message),
    ];
    if (diagWarnings.length > 0) {
      uiStore.toast(diagWarnings.join(' | '), 'info');
    }
    uiStore.toast(`${t('results.calcSuccess')}${classText} — ${results.elementForces.length} ${t('results.bars')}, ${results.reactions.length} ${t('results.reactions')}${comboText}`, 'success');
    // A solve is the expensive operation autosave exists for; only live-calc asked for one.
    void requestAutosave('solve');
    return { ok: true };
  }
  uiStore.toast(t('results.emptyModelError'), 'error');
  return { ok: false, error: t('results.emptyModelError') };
  /*
   * A solve no longer opens anything by itself on a phone.
   *
   * `leftDrawerOpen` closed the old Toolbar drawer, which nothing renders any
   * more — the write survived the drawer. `mobileResultsPanelOpen` raised the
   * floating results panel, which on Basic does not exist at all (the panel
   * requires `appMode === 'pro'`) and on PRO landed on top of the results
   * sheet that `ProPanel.solve()` had just opened. The panel is now opened by
   * the bar's own button, and only when it is asked for.
   */
}

export async function runSolve3D(): Promise<SolveOutcome> {
  if (!isWasmReady()) {
    try { await initSolver(); } catch (e: any) {
      const error = e?.message || t('toast.solverInitFailed');
      uiStore.toast(error, 'error');
      return { ok: false, error };
    }
  }
  const isPro = uiStore.analysisMode === 'pro';
  // Floor loads kept as definitions, rewritten if the model moved under them (`defined-loads.ts`).
  syncDefinedLoadsForAnalysis();
  const versionAtStart = modelStore.modelVersion;
  const results = await modelStore.solve3DAsync(uiStore.includeSelfWeight, uiStore.axisConvention3D === 'leftHand', isPro);
  // Stale: the model was edited mid-solve, and these results are not its own.
  if (modelStore.modelVersion !== versionAtStart) return { ok: false, error: t('results.editedDuringSolve') };
  if (typeof results === 'string') {
    uiStore.toast(results, 'error');
    // Name the mechanism, when that is what stopped it (`store/instability.svelte.ts`).
    if (isPro) instability.explain(uiStore.includeSelfWeight, uiStore.axisConvention3D === 'leftHand');
    return { ok: false, error: results };
  } else if (results) {
    instability.clear();
    // Validate results aren't degenerate
    const hasNaN = hasInvalid3DDisplacements(results.displacements as Array<{ ux: number; uy: number; uz: number }>);
    if (hasNaN) {
      uiStore.toast(t('results.numericError3d'), 'error');
      return { ok: false, error: t('results.numericError3d') };
    }
    resultsStore.setResults3D(results);
    // Auto-solve 3D combinations if they exist
    let comboText = '';
    if (modelStore.model.combinations.length > 0) {
      const comboResult = modelStore.solveCombinations3D(uiStore.includeSelfWeight, uiStore.axisConvention3D === 'leftHand', isPro);
      if (comboResult && typeof comboResult !== 'string') {
        publishCombinations3D(comboResult);
        comboText = t('toast.plusCombinations').replace('{n}', String(comboResult.perCombo.size));
      } else if (typeof comboResult === 'string') {
        // Refused (an empty active list, say): the solve stands, the combinations are not there.
        uiStore.toast(comboResult, 'info');
      }
    }
    // Show diagnostics warnings if present
    const diagWarnings3D = [
      ...(results.diagnostics ?? []).filter((d: { metric: string }) => d.metric === 'negative_jacobian').map((d: { message: string }) => d.message),
      ...(results.solverDiagnostics ?? []).filter((d: { severity: string }) => d.severity === 'warning').map((d: { message: string }) => d.message),
    ];
    if (diagWarnings3D.length > 0) {
      uiStore.toast(diagWarnings3D.join(' | '), 'info');
    }
    uiStore.toast(
      `${t('results.analysis3dSuccess')} — ${results.elementForces.length} ${t('results.bars')}, ${results.reactions.length} ${t('results.reactions')}${comboText}`,
      'success',
    );
    void requestAutosave('solve');
    return { ok: true };
  }
  uiStore.toast(t('results.emptyModelError'), 'error');
  // Same as above: a solve opens no panel of its own on a phone.
  return { ok: false, error: t('results.emptyModelError') };
}
