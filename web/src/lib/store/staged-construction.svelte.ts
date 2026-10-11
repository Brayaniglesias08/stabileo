/**
 * PRO's construction stages, as the Advanced tab edits them.
 *
 * They lived inside the tab's component, so leaving the tab threw them away, and nothing else
 * could read them. The build timelapse puts a model up stage by stage from here.
 *
 * They belong to the model that was open when they were written: opening another project
 * (`modelStore.loadEpoch`) starts with none, as the tab did when it was mounted afresh.
 */
import type { PanelStage } from '../engine/advanced-analyses';
import { modelStore } from './model.svelte';

function createStagedConstruction() {
  let list = $state<PanelStage[]>([]);
  let epoch = -1;
  return {
    get stages(): PanelStage[] { return epoch === modelStore.loadEpoch ? list : []; },
    set stages(v: PanelStage[]) { list = v; epoch = modelStore.loadEpoch; },
  };
}

export const stagedConstruction = createStagedConstruction();
