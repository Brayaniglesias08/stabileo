/**
 * The unit system the 2D canvas labels are written in.
 *
 * Module-level, like `setDiagramUnitSystem`, so the pure drawing functions
 * (loads, supports, reactions, dimensions) can follow the reader's choice
 * without reading a Svelte store or growing a parameter at every call site.
 * The viewport sets it at the start of each frame; anything that draws
 * without setting it (the tests) gets SI, the units the model is stored in.
 */
import type { UnitSystem } from '../utils/units';

let system: UnitSystem = 'SI';

export function setCanvasUnitSystem(us: UnitSystem): void {
  system = us;
}

export function canvasUnitSystem(): UnitSystem {
  return system;
}

/**
 * The reader's factor on the size of the loads drawn (arrows, their heads, moment arcs, thermal
 * marks): `uiStore.loadArrowScale`, the one the 3D arrows take, set by the viewport each frame the
 * same way. The text keeps its own size (the label scale).
 */
let loadScale = 1;

export function setCanvasLoadScale(k: number): void {
  loadScale = Number.isFinite(k) && k > 0 ? k : 1;
}

export function canvasLoadScale(): number {
  return loadScale;
}
