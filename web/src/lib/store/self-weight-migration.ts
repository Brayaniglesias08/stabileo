/**
 * A PRO project that states no self-weight rule is given one: self-weight as a load in its first
 * dead-load case, on the whole model, downward, when the project had self-weight on. That is what
 * the older rule computed for a project with one dead-load case; with two, the older rule counted
 * the weight in both, and the notice says so.
 *
 * Members and shells are both structure: a project made only of slabs had self-weight too.
 *
 * A project whose first member or shell is drawn in PRO starts with none: in PRO the self-weight
 * is there when it was added (Add load › General › Self-weight), in the case it was added to, and
 * nowhere else. An empty model is not given the empty rule while it is empty: it may still be
 * drawn in Basic (whose toggle weighs it) or replaced by a file or an example, and either way the
 * rule it reaches PRO with has to be the one it computed, not the one an empty panel wrote.
 *
 * Not an edit: nothing the user did changes, so it takes no undo step. It is done once the model
 * is loaded, from wherever it came (a file, an example, a link, a tab), in the PRO panel.
 */
import { untrack } from 'svelte';
import { modelStore } from './model.svelte';
import { uiStore } from './ui.svelte';
import { t } from '../i18n';
import { GRAVITY_SELF_WEIGHT, planSelfWeight } from '../engine/analysis-settings';

export { planSelfWeight, type SelfWeightPlan } from '../engine/analysis-settings';

/**
 * Give the loaded PRO model its self-weight rule, if it has none. Returns whether it did.
 * `quiet`: no notice, for an example the user just opened and has not seen compute before.
 */
export function migrateSelfWeightIfNeeded(opts: { quiet?: boolean } = {}): boolean {
  if (modelStore.analysis?.selfWeight !== undefined) return false;
  const hadMembers = hasStructure();
  const plan = planSelfWeight(modelStore.model.loadCases, uiStore.includeSelfWeight);
  let selfWeight = plan.selfWeight;
  // A model coming from Basic, where the reader chose the case self-weight goes to (Loads ›
  // Combinations, `uiStore.selfWeightCaseId`): that case, not the first dead-load one.
  const chosen = uiStore.selfWeightCaseId;
  if (uiStore.includeSelfWeight && chosen !== null && modelStore.model.loadCases.some((c) => c.id === chosen)) {
    selfWeight = [{ caseId: chosen, ...GRAVITY_SELF_WEIGHT }];
  }
  modelStore.withoutUndo(() => {
    if (uiStore.includeSelfWeight && selfWeight.length === 0 && hadMembers) {
      // Self-weight on and no dead-load case to hold it: one is made for it.
      const id = modelStore.addLoadCase(t('selfWeight.caseName'), 'D');
      selfWeight = [{ caseId: id, ...GRAVITY_SELF_WEIGHT }];
    }
    modelStore.adoptAnalysis({ selfWeight });
  });
  // Said only for a project that already had members and loads: someone drawing a first member
  // has nothing that changed under them.
  if (!opts.quiet && hadMembers && modelStore.loads.length > 0 && uiStore.includeSelfWeight && selfWeight.length) {
    const name = modelStore.model.loadCases.find((c) => c.id === selfWeight[0]!.caseId)?.name ?? '';
    uiStore.toast(t(plan.deadCases > 1 && selfWeight[0]!.caseId === plan.caseId ? 'selfWeight.migratedMany' : 'selfWeight.migrated').replaceAll('{case}', name).replaceAll('{n}', String(plan.deadCases)), 'info');
  }
  return true;
}

/** Members, or shells (plates and quads): what self-weight weighs. Nodes alone weigh nothing. */
export function hasStructure(): boolean {
  return modelStore.elements.size > 0 || modelStore.quads.size > 0 || modelStore.plates.size > 0;
}

/**
 * The model the PRO panel last saw empty and without a rule, by its `loadEpoch` (bumped when the
 * whole model is replaced: a file, a tab, an example, an undo, a new project); `null` when it has
 * not, or the panel has left since. Structure appearing in that same model while the panel stayed
 * is structure drawn in PRO.
 */
let seenEmptyAt: number | null = null;

/**
 * The PRO panel's effect: re-run on every model change. A model with structure and no rule is
 * migrated, unless its first structure was drawn here, which gets the empty rule. Only the
 * version and the epoch are tracked (a model replaced inside an undo batch, as an example's
 * `clear()`, bumps the epoch and not the version); the migration's own reads and writes are not.
 */
export function selfWeightRuleEffect(): void {
  void modelStore.modelVersion;
  void modelStore.loadEpoch;
  untrack(() => {
    if (modelStore.analysis?.selfWeight !== undefined) { seenEmptyAt = null; return; }
    if (!hasStructure()) { seenEmptyAt = modelStore.loadEpoch; return; }
    if (seenEmptyAt === modelStore.loadEpoch) modelStore.withoutUndo(() => modelStore.adoptAnalysis({ selfWeight: [] }));
    else migrateSelfWeightIfNeeded();
    seenEmptyAt = null;
  });
}

/**
 * The PRO panel is leaving (another mode): what is drawn until it comes back is not drawn in PRO,
 * so a model it saw empty is migrated when it returns, not emptied.
 */
export function leaveSelfWeightRule(): void {
  seenEmptyAt = null;
}
