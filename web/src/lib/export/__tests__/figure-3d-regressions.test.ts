import { beforeEach, expect, it } from 'vitest';
import { modelStore, resultsStore, uiStore } from '../../store';
import { createFigure3DRenderer } from '../figure-3d';
import { createSvgContext } from '../svg-context';
import { DEFAULT_FIGURE_OPTIONS } from '../view-figure';
import type { AnalysisResults3D } from '../../engine/types-3d';
import { displayedLoads3D } from '../../store/displayed-loads-3d';

beforeEach(() => {
  modelStore.clear(); resultsStore.clear();
  uiStore.analysisMode = '3d'; uiStore.includeSelfWeight = false;
  uiStore.unitSystem = 'SI';
  uiStore.cameraPosition3D = { x: 2, y: -10, z: 0 };
  uiStore.cameraTarget3D = { x: 2, y: 0, z: 0 };
});

function beam(type: 'frame' | 'truss' = 'frame') {
  const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(4, 0, 0);
  const e = modelStore.addElement(a, b, type);
  modelStore.addSupport(a, 'fixed3d');
  for (const c of [...modelStore.combinations]) modelStore.removeCombination(c.id);
  return { a, b, e };
}
function render(content: 'model' | 'reactions' | 'deformed') {
  const svg = createSvgContext(800, 600);
  createFigure3DRenderer().render(svg.ctx, 800, 600,
    { content, options: { ...DEFAULT_FIGURE_OPTIONS, titles: false } },
    { zoom: 100, panX: 100, panY: 300 });
  return svg.body();
}

it('exports only the applied loads belonging to the selected result case', () => {
  const { a, b } = beam();
  const [dead, live] = modelStore.loadCases;
  modelStore.addNodalLoad3D(b, 0, 0, -10, 0, 0, 0, dead.id);
  modelStore.addNodalLoad3D(b, 0, 0, -30, 0, 0, 0, live.id);
  modelStore.addCombination('D + L', [{ caseId: dead.id, factor: 1 }, { caseId: live.id, factor: 1 }]);
  const solved = modelStore.solveCombinations3D(false, false);
  if (!solved || typeof solved === 'string') throw new Error(String(solved));
  resultsStore.setCombinationResults3D(solved.perCase, solved.perCombo, solved.envelope);
  resultsStore.activeView = 'single'; resultsStore.activeCaseId = dead.id;
  expect(resultsStore.results3D!.reactions.find(r => r.nodeId === a)!.fz).toBeCloseTo(10);
  const svg = render('reactions');
  expect(svg).toContain('10.0 kN');
  expect(svg).not.toContain('30.0 kN');
});

it('exports applied loads on a 2D model displayed in the 3D workspace', () => {
  uiStore.analysisMode = '2d';
  const a = modelStore.addNode(0, 0), b = modelStore.addNode(4, 0);
  modelStore.addElement(a, b, 'frame');
  modelStore.addSupport(a, 'fixed');
  modelStore.addNodalLoad(b, 0, -12, 0);
  uiStore.analysisMode = '3d';
  expect(uiStore.viewportPresentation3D).toBe('upright2dIn3d');
  expect(render('model')).toContain('12.0 kN');
});

it.each(['truss', 'cable', 'tensionOnly', 'compressionOnly', 'frame'] as const)(
  'exports the appropriate deformed shape for a %s member', kind => {
  const { a, b, e } = beam(kind === 'truss' ? 'truss' : 'frame');
  if (kind !== 'truss' && kind !== 'frame') modelStore.updateElement(e, { behaviour: kind });
  resultsStore.setResults3D({
    displacements: [
      { nodeId: a, ux: 0, uy: 0, uz: 0, rx: 0, ry: 0.02, rz: 0.01 },
      { nodeId: b, ux: 0.001, uy: 0.002, uz: -0.01, rx: 0, ry: -0.03, rz: 0.02 },
    ],
    reactions: [], elementForces: [{ elementId: e, length: 4, nStart: -10, nEnd: -10,
      vyStart: 0, vyEnd: 0, vzStart: 0, vzEnd: 0, mxStart: 0, mxEnd: 0,
      myStart: 0, myEnd: 0, mzStart: 0, mzEnd: 0, hingeStart: false, hingeEnd: false,
      pointLoadsY: [], pointLoadsZ: [], distributedLoadsY: [], distributedLoadsZ: [],
    }],
  } as unknown as AnalysisResults3D);
  const svg = render('deformed');
  const path = [...svg.matchAll(/<path d="([^"]+)"[^>]*stroke="rgb\(229,72,42\)"/g)][0]?.[1];
  expect(path).toBeDefined();
  const coords = path!.match(/-?\d+(?:\.\d+)?/g)!.map(Number);
  const x0 = coords[0], y0 = coords[1], dx = coords.at(-2)! - x0, dy = coords.at(-1)! - y0;
  let maxDistance = 0;
  for (let i = 0; i < coords.length; i += 2) {
    maxDistance = Math.max(maxDistance, Math.abs((coords[i] - x0) * dy - (coords[i + 1] - y0) * dx) / Math.hypot(dx, dy));
  }
  if (kind === 'frame') expect(maxDistance).toBeGreaterThan(1);
  else expect(maxDistance).toBeLessThan(0.02);
});

it('uses reduced composite cases and signed combination factors without changing model loads', () => {
  const { b } = beam();
  const [dead, live] = modelStore.loadCases;
  modelStore.addNodalLoad3D(b, 0, 0, -10, 0, 0, 0, dead.id);
  modelStore.addNodalLoad3D(b, 0, 0, -30, 0, 0, 0, live.id);
  modelStore.updateLoadCaseFields(live.id, { reduction: { ratio: 0.5, tributaryAreaM2: 80, elementKind: 'interiorBeam', floorsSupported: 1 } });
  const composite = modelStore.addLoadCase('Composite', 'D');
  modelStore.updateLoadCaseFields(composite, { includes: [{ caseId: dead.id, factor: 1.2 }, { caseId: live.id, factor: 1.6 }] });
  const combo = modelStore.addCombination('Reversed composite', [{ caseId: composite, factor: -2 }]);
  const before = JSON.stringify(modelStore.loads);
  const solved = modelStore.solveCombinations3D(false, false);
  if (!solved || typeof solved === 'string') throw new Error(String(solved));
  resultsStore.setCombinationResults3D(solved.perCase, solved.perCombo, solved.envelope);
  resultsStore.activeView = 'single'; resultsStore.activeCaseId = composite;
  const fz = () => displayedLoads3D().reduce((sum, l) => sum + (l.type === 'nodal3d' ? l.data.fz : 0), 0);
  expect(fz()).toBeCloseTo(-36);
  expect(render('reactions')).toContain('24.0 kN');
  const cached = displayedLoads3D();
  expect(displayedLoads3D()).toBe(cached);
  resultsStore.activeView = 'combo'; resultsStore.activeComboId = combo;
  expect(fz()).toBeCloseTo(72);
  expect(render('reactions')).toContain('48.0 kN');
  resultsStore.activeView = 'envelope';
  expect(displayedLoads3D()).toEqual([]);
  expect(JSON.stringify(modelStore.loads)).toBe(before);
});

it('does not pair an SRSS result with signed static loads', () => {
  const { b } = beam();
  modelStore.addNodalLoad3D(b, 0, 0, -10, 0, 0, 0);
  resultsStore.setResults3D({ displacements: [], reactions: [], elementForces: [], magnitude: { method: 'srss' } } as AnalysisResults3D);
  expect(displayedLoads3D()).toEqual([]);
  // A model figure still shows the entered loads independently of the result selection.
  expect(render('model')).toContain('10.0 kN');
});

it('follows the single solve rules for reduced and reference cases', () => {
  const { b } = beam();
  const [dead, live] = modelStore.loadCases;
  modelStore.addNodalLoad3D(b, 0, 0, -10, 0, 0, 0, dead.id);
  modelStore.addNodalLoad3D(b, 0, 0, -30, 0, 0, 0, live.id);
  modelStore.updateLoadCaseFields(dead.id, { reference: true });
  modelStore.updateLoadCaseFields(live.id, { reduction: { ratio: 0.5, tributaryAreaM2: 80, elementKind: 'interiorBeam', floorsSupported: 1 } });
  const solved = modelStore.solve3D(false, false);
  if (!solved || typeof solved === 'string') throw new Error(String(solved));
  resultsStore.setResults3D(solved);
  expect(displayedLoads3D()).toMatchObject([{ type: 'nodal3d', data: { fz: -15, caseId: live.id } }]);
  expect(render('reactions')).toContain('15.0 kN');
  expect(render('reactions')).not.toContain('10.0 kN');
});
