/**
 * The provenance of the results on screen, gathered from the live stores
 * (`provenance/result-provenance.ts` holds the record, its hashes and its printing).
 *
 * Computed when asked (a report, an export, the reproduce dialog), not at every publish: hashing a
 * large model's results costs tens of milliseconds and live calc publishes on every edit. That is
 * sound because results never outlive the model they came from: any model mutation clears them
 * (`store/index.ts`), so the model read here is the one that was solved. Reinforcement edits do
 * not clear analysis results, and are part of the model hash: they are inputs of the design
 * checks, which re-run on them.
 */
import { modelStore } from './model.svelte';
import { resultsStore } from './results.svelte';
import { uiStore } from './ui.svelte';
import { certificateStore } from './certificate.svelte';
import { recheckStore } from './recheck.svelte';
import { buildIdentity } from '../provenance/build-identity';
import { RESULTS_VERSION } from '../provenance/registry';
import { RESULTS_SCHEMA_VERSION } from '../provenance/solve-stamp';
import { exactHash } from '../provenance/exact-hash';
import {
  buildResultProvenance, type AnalysisSettings, type CertificateRecord, type ProvenanceLabels, type ResultProvenance,
} from '../provenance/result-provenance';
import { t } from '../i18n';
import { certifiableInputOf, type CertifiableInput } from '../engine/solver-service';
import { engineReproduceHashes } from '../engine/wasm-solver';

/**
 * The engine input of the base linear solve on screen (null: none, or not a plain linear solve).
 * Set by the static-publish hook in `store/index.ts`, from the raw result before it is drawn.
 */
let baseInput: CertifiableInput | null = null;
export function noteBaseSolve(results: object | null): void {
  baseInput = results ? certifiableInputOf(results) : null;
}

/** The solver input of the base solve, as JSON, for the CLI reproduce (`examples/reproduce.rs`). */
export function baseSolverInputJson(): { dim: '2d' | '3d'; json: string } | null {
  return baseInput ? { dim: baseInput.dim, json: JSON.stringify(baseInput.wire()) } : null;
}

/** The engine's own hashes of the base solve (re-solved once in WASM); null when unavailable. */
function engineHashes(): { input: string; results: string } | null {
  const b = baseSolverInputJson();
  if (!b) return null;
  const h = engineReproduceHashes(b.json, b.dim === '3d');
  return h ? { input: h.inputSha256, results: h.resultsSha256 } : null;
}

export function currentAnalysisSettings(): AnalysisSettings {
  const is3D = uiStore.is3DWorkspace;
  return {
    analysisMode: uiStore.analysisMode,
    includeSelfWeight: uiStore.includeSelfWeight,
    selfWeightCaseId: uiStore.selfWeightCaseId,
    axisConvention3D: uiStore.axisConvention3D,
    ...(is3D ? {} : { drawPlane2D: String(uiStore.drawPlane2D) }),
  };
}

/** Which result is on screen, by name. */
export function currentScope(): string {
  const view = resultsStore.activeView;
  if (view === 'envelope') return 'envelope';
  if (view === 'combo') {
    const id = resultsStore.activeComboId;
    const c = modelStore.model.combinations.find((x) => x.id === id);
    return `combination ${id}${c ? ` (${c.name})` : ''}`;
  }
  const caseId = resultsStore.activeCaseId;
  if (caseId !== null && caseId !== undefined) return `case ${caseId} (${modelStore.getLoadCaseName(caseId) ?? ''})`;
  return 'single';
}

function certificates(): CertificateRecord[] {
  const s = certificateStore.state;
  if (s.status !== 'done') return [];
  const { certificate } = s;
  return [{
    scope: s.scope ?? 'single',
    status: certificate.status,
    hash: exactHash({
      certificate,
      forceBounds: s.forceBounds, shellBounds: s.shellBounds, stationBounds: s.stationBounds,
      deflectionBounds: s.deflectionBounds, reactionBounds: s.reactionBounds,
    }),
    ...(certificate.status === 'certified' ? { absBound: certificate.absBound } : {}),
  }];
}

/** The provenance of what is on screen now (results null: none). */
export function currentResultProvenance(now?: Date): ResultProvenance {
  const is3D = uiStore.is3DWorkspace;
  const results = is3D ? resultsStore.results3D : resultsStore.results;
  const base = is3D ? resultsStore.singleResults3D : resultsStore.singleResults;
  return buildResultProvenance({
    build: buildIdentity(),
    resultsVersion: RESULTS_VERSION,
    resultsSchemaVersion: RESULTS_SCHEMA_VERSION,
    snapshot: modelStore.snapshot(),
    settings: currentAnalysisSettings(),
    scope: currentScope(),
    results,
    baseResults: base,
    certificates: certificates(),
    stale: recheckStore.stale,
    stamp: recheckStore.stamp,
    engine: results ? engineHashes() : null,
    now,
  });
}

/** The labels a report prints the record with, in the reader's language. */
export function provenanceLabels(): ProvenanceLabels {
  return {
    title: t('provenance.title'),
    app: t('provenance.app'),
    engine: t('provenance.engine'),
    wasm: t('provenance.wasm'),
    resultsVersion: t('provenance.resultsVersion'),
    model: t('provenance.model'),
    settings: t('provenance.settings'),
    results: t('provenance.results'),
    engineSolve: t('provenance.engineSolve'),
    scope: t('provenance.scope'),
    certificates: t('provenance.certificates'),
    none: t('provenance.none'),
    stale: t('provenance.stale'),
    reproduce: t('provenance.reproduceNote'),
  };
}
