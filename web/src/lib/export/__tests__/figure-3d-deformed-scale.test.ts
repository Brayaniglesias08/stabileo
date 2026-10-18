/**
 * The deformed shape of a 3D figure follows the 2D figures' rule: at the reader's scale when they
 * set one, drawn so the largest movement reads at about a twentieth of the structure only while
 * they kept true scale (1). The structure's size is its box, found in one pass over the nodes.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { modelStore, resultsStore, uiStore } from '../../store';
import { createFigure3DRenderer, deformedFigureScale } from '../figure-3d';
import { createSvgContext } from '../svg-context';
import { DEFAULT_FIGURE_OPTIONS } from '../view-figure';
import type { AnalysisResults3D } from '../../engine/types-3d';

beforeEach(() => {
  modelStore.clear(); resultsStore.clear();
  uiStore.analysisMode = '3d'; uiStore.includeSelfWeight = false;
  uiStore.cameraPosition3D = { x: 2, y: -10, z: 0 };
  uiStore.cameraTarget3D = { x: 2, y: 0, z: 0 };
});

/** A 4 m truss member along X, its free end 10 mm down. */
function bar() {
  const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(4, 0, 0);
  const e = modelStore.addElement(a, b, 'truss');
  resultsStore.setResults3D({
    displacements: [
      { nodeId: a, ux: 0, uy: 0, uz: 0, rx: 0, ry: 0, rz: 0 },
      { nodeId: b, ux: 0, uy: 0, uz: -0.01, rx: 0, ry: 0, rz: 0 },
    ],
    reactions: [], elementForces: [{ elementId: e, length: 4, nStart: 0, nEnd: 0,
      vyStart: 0, vyEnd: 0, vzStart: 0, vzEnd: 0, mxStart: 0, mxEnd: 0,
      myStart: 0, myEnd: 0, mzStart: 0, mzEnd: 0, hingeStart: false, hingeEnd: false,
      pointLoadsY: [], pointLoadsZ: [], distributedLoadsY: [], distributedLoadsZ: [],
    }],
  } as unknown as AnalysisResults3D);
}

/** Where the deformed member's free end lands, in figure pixels (100 px a metre, y at 300). */
function deformedEnd(): { x: number; y: number } {
  const svg = createSvgContext(800, 600);
  createFigure3DRenderer().render(svg.ctx, 800, 600,
    { content: 'deformed', options: { ...DEFAULT_FIGURE_OPTIONS, titles: false, values: false } },
    { zoom: 100, panX: 100, panY: 300 });
  const d = [...svg.body().matchAll(/<path d="([^"]+)"[^>]*stroke="rgb\(229,72,42\)"/g)][0]![1]!;
  const n = d.match(/-?\d+(?:\.\d+)?/g)!.map(Number);
  return { x: n.at(-2)!, y: n.at(-1)! };
}

describe('the deformed shape of a 3D figure', () => {
  it('at true scale is drawn at a twentieth of the structure', () => {
    bar();
    resultsStore.deformedScale = 1;
    // 5 % of 4 m = 0.2 m = 20 px down.
    expect(deformedEnd().y).toBeCloseTo(320, 0);
  });

  it('at a scale the reader set is drawn at that scale', () => {
    bar();
    resultsStore.deformedScale = 7;
    // 7 × 10 mm = 70 mm = 7 px down.
    expect(deformedEnd().y).toBeCloseTo(307, 0);
  });

  it('measures the structure in one pass: twenty thousand nodes take no time', () => {
    const pts = Array.from({ length: 20_000 }, (_, i) => ({ x: (i % 200) * 0.5, y: Math.floor(i / 200) * 0.5, z: (i % 7) * 0.1 }));
    const disp = [{ ux: 0, uy: 0, uz: -0.01 }];
    const t0 = performance.now();
    const s = deformedFigureScale(pts, disp, 1);
    expect(performance.now() - t0).toBeLessThan(200);
    // The box is 99.5 × 49.5 × 0.6 m: its diagonal, 111.13 m; 5 % of it over 10 mm.
    expect(s).toBeCloseTo((0.05 * Math.hypot(99.5, 49.5, 0.6)) / 0.01, 3);
    expect(deformedFigureScale(pts, disp, 7)).toBe(7);
  });
});
