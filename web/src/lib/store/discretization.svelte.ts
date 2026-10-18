/**
 * The discretization (mesh) error of the 3D linear static result on screen
 * (docs/verification/DISCRETIZATION.md). An ESTIMATE, never a certificate: the certificate
 * (`certificate.svelte.ts`) bounds the algebraic error of the assembled system; this estimates how
 * far that system's solution is from the structure the mesh stands for.
 *
 *   · `estimate`: Zienkiewicz–Zhu / SPR energy-norm estimate per shell element and for the model,
 *     computed in the background after every published 3D base solve that has shells.
 *   · `study`: the convergence check (uniform refinement, Richardson extrapolation), run on demand
 *     because it re-solves the model on meshes 4× and 16× its size.
 */
import { certifiableInputOf, type CertifiableInput } from '../engine/solver-service';
import { runCertify, hasCertify } from '../engine/certify-jobs';
import { errorText } from '../utils/error-text';
import { resultsStore } from './results.svelte';

export interface ElementErrorIndicator {
  kind: 'plate' | 'quad' | 'quad9' | 'curvedShell' | 'solidShell';
  elementId: number;
  eta: number;
  energyNorm: number;
  /** Element error on the scale of the global percentage (see the engine docs). */
  localPercent: number;
}

export interface DiscretizationEstimate {
  certified: false;
  method: string;
  relativeError?: number | null;
  errorNorm: number;
  solutionNorm: number;
  threshold: number;
  exceedsThreshold: boolean;
  elements: ElementErrorIndicator[];
  frameMembers: number;
  notes: string[];
}

export interface QuantityConvergence {
  quantity: 'maxDisplacement' | 'maxMoment' | 'maxStress' | 'reaction' | 'maxFrameMoment';
  nodeId?: number;
  values: number[];
  relativeChange: number;
  /** Absent (undefined over the WASM boundary) when there is none. */
  observedOrder?: number | null;
  extrapolated?: number | null;
  estimatedError?: number | null;
  status: 'converging' | 'converged' | 'assumedOrder' | 'irregularOrder' | 'oscillating' | 'notConverging' | 'zero';
}

export interface ConvergenceStudy {
  certified: false;
  geometry: 'smooth' | 'faceted';
  levels: Array<{ nodes: number; shellElements: number; energyEstimate: number | null }>;
  quantities: QuantityConvergence[];
  maxEstimatedError?: number | null;
  threshold: number;
  exceedsThreshold: boolean;
  caveats: string[];
}

type Estimate =
  | { status: 'idle' }
  | { status: 'pending' }
  | { status: 'done'; estimate: DiscretizationEstimate }
  | { status: 'failed'; reason: string };

type Study =
  | { status: 'idle' }
  | { status: 'running' }
  | { status: 'done'; study: ConvergenceStudy }
  | { status: 'failed'; reason: string };

const ESTIMATE_EXPORT = 'estimate_discretization_3d';
const STUDY_EXPORT = 'convergence_study_3d';
const SETTLE_MS = 300;
/** The dimension of the solves this store estimates (shells exist in 3D only). */
const SPACE = '3d' as const;

/** Whether a 3D wire input has shell-type elements (the estimate is about them). */
function hasShells(wire: Record<string, any>): boolean {
  const n = (k: string) => Object.keys(wire[k] ?? {}).length;
  return n('plates') + n('quads') + n('quad9s') + n('curvedShells') + n('solidShells') > 0;
}

function createDiscretizationStore() {
  let estimate = $state<Estimate>({ status: 'idle' });
  let study = $state<Study>({ status: 'idle' });
  /** Paint shells by their element error indicator. */
  let showMap = $state(false);
  let input: CertifiableInput | null = null;
  /**
   * The results object the estimate belongs to, as published (after normalisation). Any other
   * result on screen — a load case, a combination, a P-Δ or spectral result that replaced the
   * base solve — is not what was estimated.
   */
  let estimated = $state.raw<object | null>(null);
  let generation = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function run(gen: number, wire: Record<string, any>): Promise<void> {
    let next: Estimate;
    try {
      next = { status: 'done', estimate: await runCertify(ESTIMATE_EXPORT, [wire]) };
    } catch (e) {
      next = { status: 'failed', reason: errorText(e, 'error') };
    }
    if (gen === generation) estimate = next;
  }

  return {
    get estimate() { return estimate; },
    get study() { return study; },
    get showMap() { return showMap; },
    set showMap(v: boolean) { showMap = v; },

    /** A base static solve was published (or cleared): estimate it in the background. */
    track(results: object | null, dim: '2d' | '3d'): void {
      generation++;
      if (timer !== null) { clearTimeout(timer); timer = null; }
      study = { status: 'idle' };
      input = null;
      estimated = null;
      const pub = generation;
      // `setResults3D` publishes the normalised results right after this hook returns.
      queueMicrotask(() => { if (pub === generation) estimated = resultsStore.results3D; });
      if (dim !== SPACE) { estimate = { status: 'idle' }; return; }
      const found = results ? certifiableInputOf(results) : null;
      if (!found || found.dim !== SPACE || !hasCertify(ESTIMATE_EXPORT)) { estimate = { status: 'idle' }; showMap = false; return; }
      const wire = found.wire();
      if (!hasShells(wire)) { estimate = { status: 'idle' }; showMap = false; return; }
      input = found;
      estimate = { status: 'pending' };
      const gen = generation;
      timer = setTimeout(() => { timer = null; void run(gen, wire); }, SETTLE_MS);
    },

    /** Whether the estimate applies to what is on screen (the base solve, not a case or combination). */
    applies(): boolean {
      const shown = resultsStore.results3D;
      return shown !== null && shown === estimated && resultsStore.activeView !== 'envelope'
        && resultsStore.activeView !== 'combo' && resultsStore.activeCaseId === null;
    },

    canRunStudy(): boolean {
      return input !== null && this.applies() && hasCertify(STUDY_EXPORT) && study.status !== 'running';
    },

    /** Run the convergence check on the base solve (re-solves on refined meshes). */
    async runStudy(geometry: 'smooth' | 'faceted' = 'smooth'): Promise<void> {
      if (!input || !this.applies()) return;
      const gen = generation;
      study = { status: 'running' };
      let next: Study;
      try {
        next = { status: 'done', study: await runCertify(STUDY_EXPORT, [input.wire(), { geometry }]) };
      } catch (e) {
        next = { status: 'failed', reason: errorText(e, 'error') };
      }
      if (gen === generation) study = next;
    },

    /** The scale of the error map (%): 0 to the larger of twice the threshold and the largest value. */
    mapRange(): { min: number; max: number } {
      let max = 0;
      for (const v of this.elementMap().values()) if (v > max) max = v;
      const thr = estimate.status === 'done' ? 100 * estimate.estimate.threshold : 5;
      return { min: 0, max: Math.max(2 * thr, max) };
    },

    /** Element indicators keyed as the 3D viewport keys shells (`p12`, `q7`). */
    elementMap(): Map<string, number> {
      const m = new Map<string, number>();
      if (estimate.status !== 'done') return m;
      for (const e of estimate.estimate.elements) {
        if (e.kind === 'plate') m.set(`p${e.elementId}`, e.localPercent);
        else if (e.kind === 'quad') m.set(`q${e.elementId}`, e.localPercent);
      }
      return m;
    },
  };
}

export const discretizationStore = createDiscretizationStore();
