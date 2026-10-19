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
  buildResultProvenance, type ScopeRef, type AnalysisSettings, type CertificateRecord, type ProvenanceLabels, type ResultProvenance,
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

/** Which engine input a wire is (the engine's `is_3d`): not a workspace check. */
const SPACE_INPUT = { '2d': false, '3d': true } as const;

/** The engine's own hashes of the base solve (re-solved once in WASM); null when unavailable. */
function engineHashes(): { input: string; results: string } | null {
  const b = baseSolverInputJson();
  if (!b) return null;
  const h = engineReproduceHashes(b.json, SPACE_INPUT[b.dim]);
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

/** Which result is on screen, as something a re-run can select again. */
export function currentScopeRef(): ScopeRef {
  const is3D = uiStore.is3DWorkspace;
  const view = resultsStore.activeView;
  let ref: ScopeRef;
  if (view === 'envelope') ref = { kind: 'envelope', name: is3D ? resultsStore.viewedEnvelopeName : null };
  else if (view === 'combo' && resultsStore.activeComboId !== null) ref = { kind: 'combo', id: resultsStore.activeComboId };
  else if (resultsStore.activeCaseId !== null && resultsStore.activeCaseId !== undefined) ref = { kind: 'case', id: resultsStore.activeCaseId };
  else ref = { kind: 'single' };
  // What is drawn is not the result that scope names (a P-Delta or a collapse on screen): a
  // re-solve cannot give it back, and the record says so rather than naming the base solve.
  const shown = is3D ? resultsStore.results3D : resultsStore.results;
  if (shown && resultForScope(ref) !== shown) return { kind: 'other', label: String(resultsStore.diagramType) };
  return ref;
}

/**
 * The result a scope names in the results on hand, or null when there is none. The same lookup
 * the views make (`results.svelte.ts` activeView / activeComboId / activeCaseId).
 */
export function resultForScope(ref: ScopeRef | undefined): object | null {
  if (!ref) return null;
  const is3D = uiStore.is3DWorkspace;
  switch (ref.kind) {
    case 'single': return (is3D ? resultsStore.singleResults3D : resultsStore.singleResults) ?? null;
    case 'case': return (is3D ? resultsStore.perCase3D : resultsStore.perCase).get(ref.id) ?? null;
    case 'combo': return (is3D ? resultsStore.perCombo3D : resultsStore.perCombo).get(ref.id) ?? null;
    case 'envelope': {
      if (!is3D) return ref.name === null ? resultsStore.envelope?.maxAbsResults ?? null : null;
      if (ref.name === null) return resultsStore.envelope3D?.maxAbsResults3D ?? null;
      // A named envelope is a view built over the solve; it is not there after a re-solve unless
      // the same one is being shown.
      return resultsStore.viewedEnvelopeName === ref.name ? resultsStore.fullEnvelope3D?.maxAbsResults3D ?? null : null;
    }
    case 'other': return null;
  }
}

/** A scope by name, for the printed record. */
export function scopeLabel(ref: ScopeRef): string {
  switch (ref.kind) {
    case 'single': return 'single';
    case 'case': return `case ${ref.id} (${modelStore.getLoadCaseName(ref.id) ?? ''})`;
    case 'combo': {
      const c = modelStore.model.combinations.find((x) => x.id === ref.id);
      return `combination ${ref.id}${c ? ` (${c.name})` : ''}`;
    }
    case 'envelope': return ref.name === null ? 'envelope' : `envelope "${ref.name}"`;
    case 'other': return `other (${ref.label})`;
  }
}

/** Which result is on screen, by name. */
export function currentScope(): string {
  return scopeLabel(currentScopeRef());
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
  const ref = currentScopeRef();
  return resultProvenanceFor(ref, (is3D ? resultsStore.results3D : resultsStore.results) ?? null, now);
}

/**
 * The provenance of the result a scope names in the results on hand (`results` null: that scope
 * is not there; the record then carries no results hash for it). What `reproduceFrom` uses to
 * recompute exactly the result a report claims.
 */
export function provenanceForScope(ref: ScopeRef, now?: Date): ResultProvenance {
  return resultProvenanceFor(ref, resultForScope(ref), now);
}

function resultProvenanceFor(ref: ScopeRef, results: object | null, now?: Date): ResultProvenance {
  const is3D = uiStore.is3DWorkspace;
  const base = is3D ? resultsStore.singleResults3D : resultsStore.singleResults;
  return buildResultProvenance({
    build: buildIdentity(),
    resultsVersion: RESULTS_VERSION,
    resultsSchemaVersion: RESULTS_SCHEMA_VERSION,
    snapshot: modelStore.snapshot(),
    settings: currentAnalysisSettings(),
    scope: scopeLabel(ref),
    scopeRef: ref,
    results,
    baseResults: base ?? null,
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
