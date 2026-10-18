/**
 * The provenance of a result: enough to say which build produced it from which model and
 * settings, and to check a re-run bit for bit (roadmap: "Trust and verification backlog", item 9).
 *
 * Pure. `store/result-provenance.ts` gathers the inputs from the live stores; reports print the
 * record (`provenanceBlockHtml`, `provenanceTextLines`) and embed it as JSON, and the reproduce
 * path (`compareProvenance`) checks a fresh record against a printed one.
 *
 * ── What is hashed, and what is left out ───────────────────────────
 *
 * Every hash is SHA-256 of the exact canonical JSON (`exact-hash.ts`): every bit of every number.
 *
 * modelHash     the model as the solver and the design checks read it. Left out: what names,
 *               draws or remembers the project but cannot move a number — its name, title
 *               block, notes, saved camera views, grid, presentation, export record,
 *               hand-edit marks, solve stamp, revision counters and the detailing drawings
 *               (derived from the model and the results, and re-stamped on every load).
 * settingsHash  the session settings that change the solve: workspace, self-weight and its case,
 *               axis convention, drawing plane.
 * resultsHash   the results on screen, without wall-clock timings.
 */
import { exactHash } from './exact-hash';
import type { BuildIdentity } from './build-identity';
import type { SolveStamp } from './solve-stamp';

export const PROVENANCE_SCHEMA = 'stabileo.provenance.v1';

/** Snapshot keys that cannot change a result (see the header). */
export const MODEL_HASH_OMIT = [
  'name', 'projectInfo', 'notes', 'views', 'grid', 'presentation3D', 'provenance',
  'exports', 'manualEdits', 'solvedWith', 'revisions', 'detailing', 'analysisMode',
] as const;

/** Result keys that describe the run, not the answer. */
export const RESULTS_HASH_OMIT = ['timings', 'solveTimings', 'runMeta', 'meta'] as const;

export interface AnalysisSettings {
  analysisMode: string;
  includeSelfWeight: boolean;
  selfWeightCaseId: number | null;
  axisConvention3D: string;
  drawPlane2D?: string;
}

export interface CertificateRecord {
  scope: string;
  status: string;
  /** SHA-256 of the certificate as the app holds it (bounds included). */
  hash: string;
  absBound?: number;
}

export interface ResultProvenance {
  schema: typeof PROVENANCE_SCHEMA;
  appVersion: string;
  commit: string;
  engineVersion: string;
  engineCommit: string;
  wasmHash: string;
  resultsVersion: number;
  resultsSchemaVersion: number;
  modelHash: string;
  settings: AnalysisSettings;
  settingsHash: string;
  /** Which result: the base solve, a case, a combination or the envelope, by name. */
  scope: string;
  /** null: no results (a report written for a model that did not solve). */
  resultsHash: string | null;
  /** The base solve's hash when the scope is not the base solve. */
  baseResultsHash: string | null;
  certificates: CertificateRecord[];
  /** The project's results predate registered changes and have not been re-run. */
  stale: boolean;
  /** The stamp of the solve that produced the results. */
  solvedWith: Pick<SolveStamp, 'resultsVersion' | 'commit' | 'engineVersion' | 'solvedAt'> | null;
  generatedAt: string;
}

export function modelHash(snapshot: unknown): string {
  return exactHash(stripModel(snapshot));
}

function stripModel(snapshot: unknown): unknown {
  if (!snapshot || typeof snapshot !== 'object') return snapshot;
  const out: Record<string, unknown> = { ...(snapshot as Record<string, unknown>) };
  for (const k of MODEL_HASH_OMIT) delete out[k];
  return out;
}

export function resultsHash(results: unknown): string | null {
  return results ? exactHash(results, [...RESULTS_HASH_OMIT]) : null;
}

export interface ProvenanceInputs {
  build: BuildIdentity;
  resultsVersion: number;
  resultsSchemaVersion: number;
  snapshot: unknown;
  settings: AnalysisSettings;
  scope: string;
  results: unknown;
  baseResults?: unknown;
  certificates?: CertificateRecord[];
  stale: boolean;
  stamp: SolveStamp | null;
  now?: Date;
}

export function buildResultProvenance(i: ProvenanceInputs): ResultProvenance {
  const base = i.baseResults && i.baseResults !== i.results ? resultsHash(i.baseResults) : null;
  return {
    schema: PROVENANCE_SCHEMA,
    appVersion: i.build.appVersion,
    commit: i.build.commit,
    engineVersion: i.build.engineVersion,
    engineCommit: i.build.engineCommit,
    wasmHash: i.build.wasmHash,
    resultsVersion: i.resultsVersion,
    resultsSchemaVersion: i.resultsSchemaVersion,
    modelHash: modelHash(i.snapshot),
    settings: i.settings,
    settingsHash: exactHash(i.settings),
    scope: i.scope,
    resultsHash: resultsHash(i.results),
    baseResultsHash: base,
    certificates: i.certificates ?? [],
    stale: i.stale,
    solvedWith: i.stamp
      ? { resultsVersion: i.stamp.resultsVersion, commit: i.stamp.commit, engineVersion: i.stamp.engineVersion, solvedAt: i.stamp.solvedAt }
      : null,
    generatedAt: (i.now ?? new Date()).toISOString(),
  };
}

// ─── Reproduce ───────────────────────────────────────────────────

export type FieldVerdict = 'match' | 'differs' | 'absent';

export interface ReproduceVerdict {
  /** The re-run gave the same results from the same model and settings. */
  reproduced: boolean;
  model: FieldVerdict;
  settings: FieldVerdict;
  /** The base solve's results (every report records them, whatever result it shows). */
  results: FieldVerdict;
  /** The result the report showed (a combination, the envelope), when the re-run shows the same one. */
  shownResults: FieldVerdict;
  /** The builds differ (a differing result may then be a registered change, not a defect). */
  sameBuild: boolean;
  buildDifferences: string[];
}

const verdict = (a: string | null | undefined, b: string | null | undefined): FieldVerdict =>
  !a || !b ? 'absent' : a === b ? 'match' : 'differs';

/** The base solve's hash of a record, whatever scope it showed. */
export function baseHashOf(p: ResultProvenance): string | null {
  return p.scope === 'single' ? p.resultsHash : p.baseResultsHash ?? null;
}

/** Compare a printed provenance (`claimed`) with the one a fresh re-run produced (`fresh`). */
export function compareProvenance(claimed: ResultProvenance, fresh: ResultProvenance): ReproduceVerdict {
  const model = verdict(claimed.modelHash, fresh.modelHash);
  const settings = verdict(claimed.settingsHash, fresh.settingsHash);
  const results = verdict(baseHashOf(claimed), baseHashOf(fresh));
  const shownResults = claimed.scope === fresh.scope ? verdict(claimed.resultsHash, fresh.resultsHash) : 'absent';
  const buildDifferences = (['appVersion', 'commit', 'engineVersion', 'engineCommit', 'wasmHash', 'resultsVersion'] as const)
    .filter((k) => String(claimed[k]) !== String(fresh[k]));
  return {
    reproduced: model === 'match' && settings === 'match' && results === 'match' && shownResults !== 'differs',
    model, settings, results, shownResults,
    sameBuild: buildDifferences.length === 0,
    buildDifferences,
  };
}

/** Read a provenance record from JSON text or from a report's HTML (its embedded record). */
export function parseProvenance(text: string): ResultProvenance | null {
  let json = text.trim();
  const m = /<script[^>]*id=["']stabileo-provenance["'][^>]*>([\s\S]*?)<\/script>/i.exec(text);
  if (m) json = m[1].trim();
  try {
    const v = JSON.parse(json) as Partial<ResultProvenance>;
    if (v?.schema !== PROVENANCE_SCHEMA || typeof v.modelHash !== 'string' || typeof v.settingsHash !== 'string') return null;
    return v as ResultProvenance;
  } catch {
    return null;
  }
}

// ─── Printing ────────────────────────────────────────────────────

export interface ProvenanceLabels {
  title: string;
  app: string;
  engine: string;
  wasm: string;
  resultsVersion: string;
  model: string;
  settings: string;
  results: string;
  scope: string;
  certificates: string;
  none: string;
  stale: string;
  reproduce: string;
}

/** Self-contained, so any report can print the block whatever its own stylesheet. */
export const PROVENANCE_CSS = `.provenance{margin-top:16px;break-inside:avoid;font-size:9pt}
.provenance h2{font-size:11pt;margin:0 0 6px}
.provenance-table{border-collapse:collapse;width:100%}
.provenance-table th{text-align:left;font-weight:600;padding:2px 8px 2px 0;vertical-align:top;white-space:nowrap}
.provenance-table td{padding:2px 0;word-break:break-all}
.provenance-table code{font-size:8pt}
.provenance-stale{border:2px solid #c0392b;color:#c0392b;padding:6px 8px;margin:6px 0}
.provenance-note{color:#666;font-size:8pt;margin-top:6px}`;

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** One line per field, for plain-text exports (CSV comments, workbook rows). */
export function provenanceTextLines(p: ResultProvenance, l: ProvenanceLabels): Array<[string, string]> {
  const s = p.settings;
  return [
    [l.app, `${p.appVersion} · ${p.commit}`],
    [l.engine, `${p.engineVersion} · ${p.engineCommit}`],
    [l.wasm, p.wasmHash],
    [l.resultsVersion, `${p.resultsVersion} (schema ${p.resultsSchemaVersion})`],
    [l.model, p.modelHash],
    [l.settings, `${p.settingsHash} — ${s.analysisMode}, self-weight ${s.includeSelfWeight ? 'on' : 'off'}${s.selfWeightCaseId !== null ? ` (case ${s.selfWeightCaseId})` : ''}, ${s.axisConvention3D}${s.drawPlane2D ? `, plane ${s.drawPlane2D}` : ''}`],
    [l.scope, p.scope],
    [l.results, p.resultsHash ?? l.none],
    [l.certificates, p.certificates.length ? p.certificates.map((c) => `${c.scope} ${c.status} ${c.hash}`).join('; ') : l.none],
  ];
}

/**
 * The provenance block a report prints, with the record embedded as JSON so the reproduce path
 * can read it back from the saved HTML.
 */
export function provenanceBlockHtml(p: ResultProvenance, l: ProvenanceLabels): string {
  const rows = provenanceTextLines(p, l)
    .map(([k, v]) => `<tr><th>${esc(k)}</th><td><code>${esc(v)}</code></td></tr>`)
    .join('');
  const stale = p.stale ? `<p class="provenance-stale"><strong>${esc(l.stale)}</strong></p>` : '';
  // `<` escaped inside the JSON so no value can close the script element.
  const json = JSON.stringify(p).replace(/</g, '\\u003c');
  return `<section class="provenance" data-testid="report-provenance">
<style>${PROVENANCE_CSS}</style>
<h2>${esc(l.title)}</h2>${stale}
<table class="provenance-table">${rows}</table>
<p class="provenance-note">${esc(l.reproduce)}</p>
<script type="application/json" id="stabileo-provenance">${json}</script>
</section>`;
}
