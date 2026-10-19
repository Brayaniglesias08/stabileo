/**
 * "Reproduce from provenance": solve the open project again and compare every hash with a
 * report's provenance record (`provenance/result-provenance.ts`).
 *
 * The results on hand are dropped first, so the comparison is against a solve that really ran
 * now and not against numbers kept from before. The CLI counterpart, for the engine alone, is
 * `cargo run --release --example reproduce` (docs/verification/PROVENANCE.md).
 */
import { resultsStore } from '../store';
import { runGlobalSolve } from '../engine/live-calc';
import { currentResultProvenance, provenanceForScope } from '../store/result-provenance';
import { compareProvenance, type ResultProvenance, type ReproduceVerdict } from '../provenance/result-provenance';
import { errorText } from '../utils/error-text';

export type ReproduceOutcome =
  | { ok: true; verdict: ReproduceVerdict; fresh: ResultProvenance }
  | { ok: false; error: string };

export async function reproduceFrom(claimed: ResultProvenance): Promise<ReproduceOutcome> {
  resultsStore.clear();
  try {
    await runGlobalSolve();
  } catch (e) {
    return { ok: false, error: errorText(e, 'error') };
  }
  return compareWithResultsOnHand(claimed);
}

/**
 * Compare a report's record with the results on hand, recomputing EXACTLY the result the report
 * claims (its case, combination or envelope), not whatever the view shows now. A claimed result
 * that cannot be selected here (a named envelope not on screen, an advanced analysis, a record
 * written before scopes were recorded) leaves the comparison incomplete, never reproduced.
 */
export function compareWithResultsOnHand(claimed: ResultProvenance): ReproduceOutcome {
  const ref = claimed.scopeRef ?? (claimed.scope === 'single' ? { kind: 'single' as const } : null);
  const fresh = ref ? provenanceForScope(ref) : { ...currentResultProvenance(), scopeRef: undefined };
  if (!fresh.baseResultsHash && !fresh.resultsHash) return { ok: false, error: 'no results' };
  return { ok: true, verdict: compareProvenance(claimed, fresh), fresh };
}
