/**
 * Result provenance and determinism (roadmap: "Trust and verification backlog", item 9).
 *
 * The same model solved twice gives bit-identical results; the model hash survives a save and a
 * reopen; a report prints the record and the reproduce comparison reads it back.
 */
import { describe, it, expect, beforeAll, beforeEach } from 'vitest';
import { modelStore } from '../model.svelte';
import { uiStore } from '../ui.svelte';
import { resultsStore } from '../results.svelte';
import '../index';
import { initSolver } from '../../engine/wasm-solver';
import { currentResultProvenance, provenanceLabels, baseSolverInputJson } from '../result-provenance';
import { serializeProject, deserializeProject } from '../file';
import { recheckStore } from '../recheck.svelte';
import {
  compareProvenance, parseProvenance, provenanceBlockHtml, modelHash,
} from '../../provenance/result-provenance';
import { buildIdentity } from '../../provenance/build-identity';
import { generateCalcReportHtml } from '../../engine/calc-report';
import { publishCombinations3D } from '../active-results';
import { compareWithResultsOnHand } from '../../actions/reproduce';

beforeAll(async () => { await initSolver(); });
beforeEach(() => {
  uiStore.analysisMode = 'pro';
  uiStore.includeSelfWeight = true;
  modelStore.clear();
});

/** A small braced frame with loads in two directions: enough shared nodes for summation order to matter. */
function buildFrame(): void {
  const n = [
    modelStore.addNode(0, 0, 0), modelStore.addNode(5, 0, 0), modelStore.addNode(5, 4, 0), modelStore.addNode(0, 4, 0),
    modelStore.addNode(0, 0, 3), modelStore.addNode(5, 0, 3), modelStore.addNode(5, 4, 3), modelStore.addNode(0, 4, 3),
  ];
  for (let i = 0; i < 4; i++) { modelStore.addElement(n[i], n[i + 4], 'frame'); modelStore.addSupport(n[i], 'fixed3d'); }
  for (let i = 0; i < 4; i++) modelStore.addElement(n[4 + i], n[4 + ((i + 1) % 4)], 'frame');
  modelStore.addElement(n[4], n[6], 'truss');
  modelStore.addElement(n[0], n[5], 'truss');
  modelStore.addNodalLoad3D(n[6], 12.5, -3.3, -40.1, 0, 0, 0);
}

function solveMain(): void {
  const r = modelStore.solve3D(uiStore.includeSelfWeight, false, true);
  if (!r || typeof r === 'string') throw new Error(String(r));
  resultsStore.setResults3D(r);
}

describe('result provenance', () => {
  it('records the build, the engine and every hash', () => {
    buildFrame();
    solveMain();
    const p = currentResultProvenance();
    const b = buildIdentity();
    expect(b.wasmHash).toMatch(/^[0-9a-f]{64}$/);
    expect(p.wasmHash).toBe(b.wasmHash);
    expect(p.engineVersion).not.toBe('not-loaded');
    expect(p.modelHash).toMatch(/^[0-9a-f]{64}$/);
    expect(p.resultsHash).toMatch(/^[0-9a-f]{64}$/);
    expect(p.settings).toMatchObject({ analysisMode: 'pro', includeSelfWeight: true });
    expect(p.stale).toBe(false);
    expect(p.solvedWith?.resultsVersion).toBe(p.resultsVersion);
    expect(p.engineInputSha256).toMatch(/^[0-9a-f]{64}$/);
    expect(p.engineResultsSha256).toMatch(/^[0-9a-f]{64}$/);
  });

  it('solving the same model twice is bit-identical (web results and engine hashes)', () => {
    buildFrame();
    solveMain();
    const a = currentResultProvenance();
    resultsStore.clear();
    solveMain();
    const b = currentResultProvenance();
    expect(b.resultsHash).toBe(a.resultsHash);
    expect(b.engineResultsSha256).toBe(a.engineResultsSha256);
    expect(b.engineInputSha256).toBe(a.engineInputSha256);
    const v = compareProvenance(a, b);
    expect(v.reproduced).toBe(true);
    expect(v.sameBuild).toBe(true);
  });

  it('the model hash survives a save and a reopen; editing the model changes it', () => {
    buildFrame();
    const before = modelHash(modelStore.snapshot());
    expect(deserializeProject(serializeProject())).toBe(true);
    expect(modelHash(modelStore.snapshot())).toBe(before);
    modelStore.addNode(9, 9, 9);
    expect(modelHash(modelStore.snapshot())).not.toBe(before);
  });

  it('a changed setting is caught by the settings hash, and reproduce says so', () => {
    buildFrame();
    solveMain();
    const a = currentResultProvenance();
    uiStore.includeSelfWeight = false;
    solveMain();
    const b = currentResultProvenance();
    const v = compareProvenance(a, b);
    expect(v.settings).toBe('differs');
    expect(v.results).toBe('differs');
    expect(v.reproduced).toBe(false);
  });

  it('a report prints the block and the reproduce path reads it back from the HTML', () => {
    buildFrame();
    solveMain();
    const p = currentResultProvenance();
    const block = provenanceBlockHtml(p, provenanceLabels());
    expect(block).toContain(p.modelHash);
    expect(block).toContain(p.wasmHash);
    const html = generateCalcReportHtml({
      config: { projectName: 'P', engineerName: '', companyName: '', date: '', notes: '' },
      is3D: true, analysisMode: 'PRO', provenance: { kind: 'single' }, hasDesignChecks: false,
      nodes: [], elements: [], materials: [], sections: [], supports: [], loads: [], loadCases: [], combinations: [],
      results3D: resultsStore.results3D!,
      resultProvenance: { record: p, labels: provenanceLabels() },
    });
    expect(html).toContain('data-testid="report-provenance"');
    const read = parseProvenance(html);
    expect(read).toEqual(p);
    expect(compareProvenance(read!, p).reproduced).toBe(true);
  });

  it('exports the base solve input for the CLI reproduce', () => {
    buildFrame();
    solveMain();
    const b = baseSolverInputJson();
    expect(b?.dim).toBe('3d');
    expect(() => JSON.parse(b!.json)).not.toThrow();
  });

  it('results from an older version are stale in the record until re-solved', () => {
    buildFrame();
    const file = JSON.parse(serializeProject());
    delete file.snapshot.solvedWith;
    expect(deserializeProject(JSON.stringify(file))).toBe(true);
    expect(recheckStore.stale).toBe(recheckStore.assessment!.changes.length > 0);
    expect(currentResultProvenance().stale).toBe(recheckStore.stale);
    solveMain();
    expect(currentResultProvenance().stale).toBe(false);
  });
});

/** The frame with a live-load case and two combinations, solved with them, as Calcular does. */
function solveWithCombinations(): void {
  buildFrame();
  const top = [...modelStore.nodes.keys()].slice(-1)[0];
  modelStore.addNodalLoad3D(top, 0, 7.5, -12, 0, 0, 0, 2);
  modelStore.addCombination('1.2D+1.6L', [{ caseId: 1, factor: 1.2 }, { caseId: 2, factor: 1.6 }]);
  modelStore.addCombination('1.4D', [{ caseId: 1, factor: 1.4 }]);
  solveMain();
  const c = modelStore.solveCombinations3D(true, false, true);
  if (!c || typeof c === 'string') throw new Error(String(c));
  publishCombinations3D(c);
}

describe('reproduce recomputes the result the report claims, not the one on screen', () => {
  it('a combination report reproduces while the screen shows the base solve', () => {
    solveWithCombinations();
    const comboId = modelStore.model.combinations[0]!.id;
    resultsStore.activeView = 'combo';
    resultsStore.activeComboId = comboId;
    const claimed = currentResultProvenance();
    expect(claimed.scopeRef).toEqual({ kind: 'combo', id: comboId });
    resultsStore.activeView = 'single';
    const out = compareWithResultsOnHand(claimed);
    expect(out.ok && out.verdict.status).toBe('reproduced');
    expect(out.ok && out.verdict.shownResults).toBe('match');
  });

  it('a combination hash that does not match is not reproduced, whatever the screen shows', () => {
    solveWithCombinations();
    resultsStore.activeView = 'combo';
    resultsStore.activeComboId = modelStore.model.combinations[0]!.id;
    const claimed = { ...currentResultProvenance(), resultsHash: '0'.repeat(64) };
    resultsStore.activeView = 'single';
    const out = compareWithResultsOnHand(claimed);
    expect(out.ok && out.verdict.status).toBe('differs');
    expect(out.ok && out.verdict.reproduced).toBe(false);
  });

  it('a report of the other combination is compared with that combination, and a swapped scope differs', () => {
    solveWithCombinations();
    const [a, b] = modelStore.model.combinations.map((c) => c.id);
    resultsStore.activeView = 'combo';
    resultsStore.activeComboId = a!;
    const claimedA = currentResultProvenance();
    // The report claims combination B but carries A's numbers.
    const swapped = { ...claimedA, scopeRef: { kind: 'combo' as const, id: b! } };
    const out = compareWithResultsOnHand(swapped);
    expect(out.ok && out.verdict.status).toBe('differs');
  });

  it('a claimed scope that does not exist here is an incomplete comparison, never success', () => {
    solveWithCombinations();
    const base = currentResultProvenance();
    for (const scopeRef of [
      { kind: 'combo' as const, id: 999 },
      { kind: 'case' as const, id: 999 },
      { kind: 'envelope' as const, name: 'Not here' },
      { kind: 'other' as const, label: 'pdelta' },
    ]) {
      const out = compareWithResultsOnHand({ ...base, scopeRef, scope: 'x', resultsHash: base.resultsHash });
      expect(out.ok && out.verdict.status, JSON.stringify(scopeRef)).toBe('incomplete');
      expect(out.ok && out.verdict.reproduced).toBe(false);
    }
  });

  it('an envelope report without an envelope on hand is incomplete', () => {
    solveWithCombinations();
    resultsStore.activeView = 'envelope';
    const claimed = currentResultProvenance();
    expect(claimed.scopeRef).toEqual({ kind: 'envelope', name: null });
    // The same model solved without its combinations: no envelope.
    resultsStore.clear();
    solveMain();
    const out = compareWithResultsOnHand(claimed);
    expect(out.ok && out.verdict.status).toBe('incomplete');
  });

  it('a record written before scopes were recorded reproduces only a base-solve report', () => {
    solveWithCombinations();
    resultsStore.activeView = 'combo';
    resultsStore.activeComboId = modelStore.model.combinations[0]!.id;
    const { scopeRef: _s, ...legacy } = currentResultProvenance();
    const out = compareWithResultsOnHand(legacy as never);
    expect(out.ok && out.verdict.status).toBe('incomplete');
  });
});
