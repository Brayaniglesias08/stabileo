/**
 * The page a 2D figure is cut to, and the canvas a PNG is drawn on.
 *
 * - The page holds the title: a single column is a drawing a few pixels wide, and a page cut to
 *   it cut the title off after its first letters.
 * - The canvas stays inside what every browser can encode (iOS Safari: 16.7 MP; any: 32 767 px a
 *   side). Past that `toBlob` gives nothing, and a sheet of seven figures at 3× is 24 MP.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { uiStore } from '../../store';
import { createSvgContext } from '../svg-context';
import { DEFAULT_FIGURE_OPTIONS, plan2D, registerFigure2D, toCanvas, type FigurePlan } from '../view-figure';

describe('the page of a figure', () => {
  let unregister: () => void = () => {};
  beforeEach(() => {
    uiStore.analysisMode = '2d';
    // A 3 m column, as the 2D view would draw it: one line, nothing beside it.
    unregister = registerFigure2D({
      nodes: () => [{ x: 0, y: 0 }, { x: 0, y: 3 }],
      render(ctx, _w, _h, _frame, view) {
        ctx.strokeStyle = '#c7d3dd'; ctx.lineWidth = 2.5;
        ctx.beginPath(); ctx.moveTo(view.panX, view.panY); ctx.lineTo(view.panX, view.panY - 3 * view.zoom); ctx.stroke();
      },
    });
  });
  afterEach(() => unregister());

  it('is wide enough for its title', () => {
    const p = plan2D('moment', { ...DEFAULT_FIGURE_OPTIONS, titles: true })!;
    expect(p.title.length).toBeGreaterThan(5);
    const svg = createSvgContext(p.width, p.height);
    p.paint(svg.ctx);
    // Everything painted, the title too, lies on the page.
    expect(svg.bounds().x1).toBeLessThanOrEqual(p.width + 0.5);
    expect(svg.bounds().x0).toBeGreaterThanOrEqual(-0.5);
  });

  it('without a title it is cut to the drawing', () => {
    const p = plan2D('moment', { ...DEFAULT_FIGURE_OPTIONS, titles: false })!;
    expect(p.width).toBeLessThan(80);
  });
});

describe('the canvas of a PNG', () => {
  afterEach(() => vi.unstubAllGlobals());

  function canvasFor(plan: FigurePlan) {
    const canvas = { width: 0, height: 0, getContext: () => ({ scale() {}, lineJoin: 'miter' }) };
    vi.stubGlobal('document', { createElement: () => canvas });
    return toCanvas(plan) as unknown as typeof canvas;
  }
  const plan = (width: number, height: number): FigurePlan => ({ content: 'model', title: '', width, height, paint() {} });

  it('a small figure is drawn at three pixels per figure pixel', () => {
    const c = canvasFor(plan(400, 300));
    expect(c.width).toBe(1200);
    expect(c.height).toBe(900);
  });

  it('a sheet of seven figures is drawn smaller, inside what iOS Safari encodes', () => {
    // 809 × 3342 at 3× is 2427 × 10026: 24.3 MP.
    const c = canvasFor(plan(809, 3342));
    expect(c.width * c.height).toBeLessThanOrEqual(16_777_216);
    expect(c.width * c.height).toBeGreaterThan(0.95 * 16_777_216);
    expect(c.width / c.height).toBeCloseTo(809 / 3342, 2);
  });

  it('a very tall sheet keeps every side under the 32 767 px browsers allow', () => {
    const c = canvasFor(plan(300, 12000));
    expect(c.height).toBeLessThanOrEqual(32_767);
  });
});
