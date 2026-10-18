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
