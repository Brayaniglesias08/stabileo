/**
 * The phases of the build timelapse's phased orders, read off the model.
 *
 * Both are PRO's: construction stages are an advanced analysis there, and groups are made in its
 * Groups panel. Basic never offers them, and a remembered choice of one falls back to
 * `construction` there or when the model has none.
 */
import { modelStore, uiStore } from '../store';
import { stagedConstruction } from '../store/staged-construction.svelte';
import type { BuildOrder, BuildPhase } from './build-timelapse';

/** Group kinds that are not a part of the structure: load regions, and derived member chains. */
const NOT_PARTS = new Set(['loadZone', 'floorLoad', 'physicalMember']);

const keysOf = (els: readonly number[] = [], plates: readonly number[] = [], quads: readonly number[] = []) => [
  ...els.map((id) => `e${id}`), ...plates.map((id) => `p${id}`), ...quads.map((id) => `q${id}`),
];

function stagePhases(): BuildPhase[] {
  return stagedConstruction.stages
    .map((s) => ({
      name: s.name,
      add: keysOf(s.elementsAdded, s.platesAdded, s.quadsAdded),
      remove: keysOf(s.elementsRemoved, s.platesRemoved, s.quadsRemoved),
    }))
    .filter((p) => p.add.length > 0 || p.remove.length > 0);
}

function groupPhases(): BuildPhase[] {
  const out: BuildPhase[] = [];
  for (const g of modelStore.model.groups?.values() ?? []) {
    if (NOT_PARTS.has(g.kind)) continue;
    const add = keysOf(g.members.elements, g.members.plates, g.members.quads);
    if (add.length > 0) out.push({ name: g.name, add });
  }
  if (out.length === 0) return out;
  // What no group holds goes up last, so the structure still ends whole.
  const named = new Set(out.flatMap((p) => p.add));
  const rest = keysOf([...modelStore.elements.keys()], [...modelStore.plates.keys()], [...modelStore.quads.keys()]).filter((k) => !named.has(k));
  if (rest.length > 0) out.push({ name: '', add: rest });
  return out;
}

/** Whether an order can be offered here. */
export function orderAvailable(order: BuildOrder): boolean {
  if (order !== 'stages' && order !== 'groups') return true;
  if (uiStore.appMode !== 'pro') return false;
  return (order === 'stages' ? stagePhases() : groupPhases()).length > 0;
}

/** The order the timelapse actually builds in: the chosen one, if it can be offered. */
export function effectiveOrder(order: BuildOrder): BuildOrder {
  return orderAvailable(order) ? order : 'construction';
}

/** The phases for a phased order; undefined for the others. */
export function phasesFor(order: BuildOrder): BuildPhase[] | undefined {
  if (order === 'stages') return stagePhases();
  if (order === 'groups') return groupPhases();
  return undefined;
}
