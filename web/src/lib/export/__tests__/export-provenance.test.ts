/**
 * Every export of results carries the provenance record and the stale status, in its own format,
 * prepared like every export's (awaited engine hashes, bound to the state it started on).
 */
import { describe, it, expect, beforeAll, beforeEach, vi } from 'vitest';
const texts = vi.hoisted(() => [] as Array<{ content: string; name: string }>);
vi.mock('../../store/file', async (orig) => ({
  ...(await orig<typeof import('../../store/file')>()),
  downloadText: (content: string, name: string) => { texts.push({ content, name }); },
}));
import { modelStore } from '../../store/model.svelte';
import { uiStore } from '../../store/ui.svelte';
import { resultsStore } from '../../store/results.svelte';
import '../../store/index';
import { initSolver } from '../../engine/wasm-solver';
import { serializeProject, deserializeProject } from '../../store/file';
import { recheckStore } from '../../store/recheck.svelte';
import { currentResultProvenance } from '../../store/result-provenance';
import { buildExcelWorkbook, loadXlsxModule } from '../excel';
import {
  csvWithProvenance, htmlWithProvenance, dxfWithProvenance, svgWithProvenance, pngWithProvenance,
  jpegWithProvenance, provenanceFromPng, downloadResultText,
} from '../provenance-attach';

beforeAll(async () => { await initSolver(); });
beforeEach(() => { uiStore.analysisMode = 'pro'; uiStore.includeSelfWeight = true; modelStore.clear(); texts.length = 0; });

function frame(): void {
  const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(0, 0, 3), c = modelStore.addNode(4, 0, 3);
  modelStore.addElement(a, b, 'frame'); modelStore.addElement(b, c, 'frame');
  modelStore.addSupport(a, 'fixed3d');
  modelStore.addNodalLoad3D(c, 0, 0, -10, 0, 0, 0);
}
function solve(): void {
  const r = modelStore.solve3D(true, false, true);
  if (!r || typeof r === 'string') throw new Error(String(r));
  resultsStore.setResults3D(r);
}
/** A project opened from a file saved before solve stamps: stale. */
function openStale(): void {
  frame();
  const f = JSON.parse(serializeProject());
  delete f.snapshot.solvedWith;
  expect(deserializeProject(JSON.stringify(f))).toBe(true);
  expect(recheckStore.stale).toBe(true);
}
const STALE = /STALE/;

describe("Basic's Excel export", () => {
  it('opens its summary with the status and adds a Provenance sheet (STALE when stale)', async () => {
    openStale();
    const XLSX = (await loadXlsxModule())!;
    const wb = (await buildExcelWorkbook({ solveError: 'not solved' }))!;
    expect(wb.SheetNames).toContain('Provenance');
    const summary = XLSX.utils.sheet_to_json<string[]>(wb.Sheets[wb.SheetNames[0]!]!, { header: 1 });
    expect(summary[1]?.join(' ')).toMatch(STALE);
    const prov = XLSX.utils.sheet_to_json<string[]>(wb.Sheets['Provenance']!, { header: 1 });
    expect(prov[0]?.join(' ')).toMatch(STALE);
  });

  it('a solved project: the Provenance sheet carries the engine hashes and the status is current', async () => {
    frame();
    solve();
    const XLSX = (await loadXlsxModule())!;
    const wb = (await buildExcelWorkbook())!;
    const rows = XLSX.utils.sheet_to_json<string[]>(wb.Sheets['Provenance']!, { header: 1 }).map((r) => r.join(' '));
    expect(rows[0]).not.toMatch(STALE);
    expect(rows.some((r) => /input [0-9a-f]{64} · results [0-9a-f]{64}/.test(r))).toBe(true);
  });

  it('a table workbook (onlyExtras) carries it too; the project workbook opts out, its cover has it', async () => {
    frame();
    solve();
    const extras = (await buildExcelWorkbook({ onlyExtras: true, extraSheets: [{ name: 'T', rows: [['a']] }] }))!;
    expect(extras.SheetNames).toEqual(['Provenance', 'T']);
    const none = (await buildExcelWorkbook({ onlyExtras: true, extraSheets: [{ name: 'T', rows: [['a']] }], provenance: false }))!;
    expect(none.SheetNames).toEqual(['T']);
  });

  it('a model edited while it is prepared: not written', async () => {
    frame();
    solve();
    const pending = buildExcelWorkbook();
    modelStore.addNode(9, 9, 9);
    expect(await pending).toBeNull();
  });
});

describe('result files downloaded as text', () => {
  it('a result CSV carries the record and the status; a model edited meanwhile writes nothing', async () => {
    openStale();
    expect(await downloadResultText('a,b\n1,2', 'x.csv', 'text/csv')).toBe(true);
    expect(texts[0]!.content).toMatch(/^# .*STALE/);
    expect(texts[0]!.content.endsWith('a,b\n1,2')).toBe(true);
    texts.length = 0;
    const pending = downloadResultText('a,b', 'x.csv', 'text/csv');
    modelStore.addNode(7, 7, 7);
    expect(await pending).toBe(false);
    expect(texts).toHaveLength(0);
  });
});

describe('each format embeds the record and the status', () => {
  it('CSV, HTML, DXF, SVG, PNG and JPEG', () => {
    openStale();
    const p = currentResultProvenance();
    expect(csvWithProvenance('h\n1', p).split('\n')[0]).toMatch(STALE);
    const html = htmlWithProvenance('<html><body><p>r</p></body></html>', p);
    expect(html).toMatch(STALE);
    expect(html.indexOf('report-provenance')).toBeLessThan(html.indexOf('</body>'));
    const dxf = dxfWithProvenance('0\nSECTION\n', p);
    expect(dxf.split('\n')[0]).toBe('999');
    expect(dxf.split('\n')[1]).toMatch(STALE);
    const svg = svgWithProvenance('<svg xmlns="http://www.w3.org/2000/svg"><g/></svg>', p);
    expect(svg).toMatch(STALE);
    expect(svg).toContain('<metadata id="stabileo-provenance">');
    // A minimal PNG: signature + IHDR + IEND.
    const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
      0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0, 0x90, 0x77, 0x53, 0xde,
      0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82]);
    const read = provenanceFromPng(pngWithProvenance(png, p));
    expect(JSON.parse(read!).modelHash).toBe(p.modelHash);
    expect(JSON.parse(read!).stale).toBe(true);
    const jpg = jpegWithProvenance(new Uint8Array([0xff, 0xd8, 0xff, 0xd9]), p);
    expect(jpg[2]).toBe(0xff);
    expect(jpg[3]).toBe(0xfe);
    expect(new TextDecoder().decode(jpg)).toContain('"stale":true');
  });
});
