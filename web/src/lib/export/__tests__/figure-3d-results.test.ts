import { beforeEach, expect, it } from 'vitest';
import { modelStore, resultsStore, uiStore } from '../../store';
import { createFigure3DRenderer } from '../figure-3d';
import { createSvgContext } from '../svg-context';
import { computeEnvelope3D } from '../../engine/wasm-solver';
import { DEFAULT_FIGURE_OPTIONS, type FigureOptions } from '../view-figure';

beforeEach(() => {
  modelStore.clear(); resultsStore.clear();
  uiStore.analysisMode = '3d'; uiStore.includeSelfWeight = false;
  uiStore.axisConvention3D = 'rightHand'; uiStore.unitSystem = 'SI';
  uiStore.cameraPosition3D = { x: 3, y: -10, z: 0 };
  uiStore.cameraTarget3D = { x: 3, y: 0, z: 0 };
  resultsStore.deformedScale = 1;
});

function beam() {
  for (const c of [...modelStore.combinations]) modelStore.removeCombination(c.id);
  const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(6, 0, 0);
  const e = modelStore.addElement(a, b, 'frame');
  modelStore.addSupport(a, 'pinned3d');
  modelStore.addSupport(b, 'custom3d', undefined, { dofRestraints: { tx: false, ty: true, tz: true, rx: true, ry: false, rz: false } });
  const d = modelStore.addLoadCase('Dead', 'D');
  modelStore.addDistributedLoad3D(e, 0, 0, -10, -10, undefined, undefined, d);
  return { a, b, e, d };
}

function svg(content: 'moment' | 'deformed', options: Partial<FigureOptions> = {}) {
  const s = createSvgContext(800, 600);
  createFigure3DRenderer().render(s.ctx, 800, 600,
    { content, options: { ...DEFAULT_FIGURE_OPTIONS, titles: false, ...options } },
    { zoom: 100, panX: 100, panY: 300 });
  return s.body();
}

it('exports the actual envelope maximum', () => {
  const { d } = beam();
  modelStore.addCombination('D', [{ caseId: d, factor: 1 }]);
  modelStore.addCombination('1.4D', [{ caseId: d, factor: 1.4 }]);
  const r = modelStore.solveCombinations3D(false, false, false);
  if (!r || typeof r === 'string') throw new Error(String(r));
  resultsStore.setCombinationResults3D(r.perCase, r.perCombo, r.envelope);
  resultsStore.activeView = 'envelope';
  const actual = Math.max(...r.envelope.momentY.elements[0].posValues.map(Math.abs), ...r.envelope.momentY.elements[0].negValues.map(Math.abs));
  const text = [...svg('moment').matchAll(/<text[^>]*>(.*?)<\/text>/g)].map(m => m[1]);
  expect(actual).toBeCloseTo(63);
  expect(text.some(t => t.includes('63.0'))).toBe(true);
  expect(text.some(t => t.includes('88.2'))).toBe(false);
});

it.each(['simple', 'fixed'] as const)('auto-scales a %s beam whose end translations are zero', support => {
  beam();
  if (support === 'fixed') {
    for (const s of modelStore.supports.values()) modelStore.updateSupport(s.id, { type: 'fixed3d' });
  }
  const r = modelStore.solve3D(false, false, false);
  if (!r || typeof r === 'string') throw new Error(String(r));
  resultsStore.setResults3D(r);
  const path = [...svg('deformed').matchAll(/<path d="([^"]+)"[^>]*stroke="rgb\(229,72,42\)"/g)][0]?.[1];
  const coords = path!.match(/-?\d+(?:\.\d+)?/g)!.map(Number);
  const ys = coords.filter((_, i) => i % 2 === 1);
  expect(Math.max(...ys) - Math.min(...ys)).toBeCloseTo(30, 1);
});

it.each(['srss', 'abs'] as const)('keeps %s figure magnitudes unsigned', method => {
  const { d } = beam();
  const c = modelStore.addCombination(method.toUpperCase(), [{ caseId: d, factor: 1 }]);
  modelStore.updateCombination(c, { method });
  const r = modelStore.solveCombinations3D(false, false, true);
  if (!r || typeof r === 'string') throw new Error(String(r));
  resultsStore.setCombinationResults3D(r.perCase, r.perCombo, r.envelope);
  resultsStore.activeComboId = c; resultsStore.activeView = 'combo';
  expect(resultsStore.results3D!.magnitude?.method).toBe(method);
  const text = [...svg('moment').matchAll(/<text[^>]*>(.*?)<\/text>/g)].map(m => m[1]);
  expect(text).toContain('My 45.0 kN·m');
  expect(text.some(t => /My [-−]/.test(t))).toBe(false);
});

it('uses a named envelope on screen and the requested envelope off screen', () => {
  const { d } = beam();
  const c1 = modelStore.addCombination('D', [{ caseId: d, factor: 1 }]);
  modelStore.addCombination('1.4D', [{ caseId: d, factor: 1.4 }]);
  const r = modelStore.solveCombinations3D(false, false, false);
  if (!r || typeof r === 'string') throw new Error(String(r));
  resultsStore.setCombinationResults3D(r.perCase, r.perCombo, r.envelope);
  const named = computeEnvelope3D([r.perCombo.get(c1)!])!;
  resultsStore.viewEnvelope3D(named, 'Only D');
  resultsStore.activeView = 'envelope';
  expect(svg('moment')).toContain('My -45.0 kN·m');
  expect(svg('moment')).not.toContain('63.0');
  expect(svg('moment', { results: resultsStore.envelope3D!.maxAbsResults3D })).toContain('My -63.0 kN·m');
  resultsStore.activeCaseId = d;
  expect(svg('moment', { results: resultsStore.envelope3D!.maxAbsResults3D })).toContain('My -63.0 kN·m');
});
