/**
 * The certificate of the linear static solve on screen (docs/verification/CERTIFY.md).
 *
 * Not the design-code "verification" (`verification.svelte.ts`): this is a rigorous bound on how
 * far the computed displacements and member forces can be from the exact solution of the
 * assembled system, checked in interval arithmetic by the engine and proved in Lean.
 *
 * Certifying costs a few times the solve, so it never delays the results: a published base solve
 * is certified after a short pause (live-calc re-solves on every edit; only the one that stays is
 * certified), in a worker when the pool is there. Only plain linear static results carry the input
 * it needs (`certifiableInputOf`); anything else leaves the store idle.
 */
import { certifiableInputOf, type CertifiableInput } from '../engine/solver-service';
import { certifyInWorker, PoolUnavailableError } from '../engine/solver-pool';
import { certifyLinearWire, hasCertifyExport, type LinearCertificate, type ForceBound2D, type ForceBound3D } from '../engine/wasm-solver';
import { errorText } from '../utils/error-text';
import { resultsStore } from './results.svelte';

export { formatBound } from '../utils/certificate-format';

export type CertificateState =
  | { status: 'idle' }
  | { status: 'pending'; dim: '2d' | '3d' }
  | { status: 'done'; dim: '2d' | '3d'; certificate: LinearCertificate; forceBounds: Map<number, ForceBound2D | ForceBound3D> };

const SETTLE_MS = 250;

function createCertificateStore() {
  let state = $state<CertificateState>({ status: 'idle' });
  let generation = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function run(gen: number, dim: '2d' | '3d', input: CertifiableInput): Promise<void> {
    let out: { certificate: LinearCertificate; forceBounds: Array<ForceBound2D | ForceBound3D> };
    try {
      try {
        out = await certifyInWorker(dim, input.wire());
      } catch (e) {
        if (!(e instanceof PoolUnavailableError)) throw e;
        out = certifyLinearWire(dim, input.wire());
      }
    } catch (e) {
      out = { certificate: { status: 'uncertified', reason: errorText(e, 'error') }, forceBounds: [] };
    }
    if (gen !== generation) return; // superseded by a newer solve or a clear
    state = { status: 'done', dim, certificate: out.certificate, forceBounds: new Map((out.forceBounds ?? []).map((b) => [b.elementId, b])) };
  }

  return {
    get state() { return state; },

    /** A base static solve was published (or, with null, the results were cleared). */
    track(results: object | null, dim: '2d' | '3d'): void {
      generation++;
      if (timer !== null) { clearTimeout(timer); timer = null; }
      const input = results ? certifiableInputOf(results) : null;
      if (!input || input.dim !== dim || !hasCertifyExport()) { state = { status: 'idle' }; return; }
      state = { status: 'pending', dim };
      const gen = generation;
      timer = setTimeout(() => { timer = null; void run(gen, dim, input); }, SETTLE_MS);
    },

    /**
     * The state, while the results on screen are the certified ones: the base solve of the
     * workspace's dimension, not a load case, a combination or the envelope.
     */
    shown(is3D: boolean): CertificateState {
      const dim = is3D ? '3d' : '2d';
      if (state.status === 'idle' || state.dim !== dim) return { status: 'idle' };
      const onScreen = is3D ? resultsStore.results3D : resultsStore.results;
      if (!onScreen || resultsStore.activeView !== 'single' || resultsStore.activeCaseId !== null) return { status: 'idle' };
      return state;
    },

    /** The force bounds of one member of the results on screen, when certified. */
    forceBound<B extends ForceBound2D | ForceBound3D>(is3D: boolean, elementId: number): B | null {
      const s = this.shown(is3D);
      return s.status === 'done' ? (s.forceBounds.get(elementId) as B | undefined) ?? null : null;
    },
  };
}

export const certificateStore = createCertificateStore();
