/**
 * Figures of the model and its results, for "Exportar vista" and the calculation report.
 *
 * In 2D a figure is the viewport's own drawing, done again off screen: the 2D viewport registers a
 * renderer (`registerFigure2D`) that runs its draw routine on another context, with what the
 * figure shows in place of what the screen shows, and without the interaction on top (selection,
 * tool previews, the grid). Drawn on `svg-context` it is a vector figure; on a canvas, a PNG.
 * Either way through `ink-context`, so it comes out on paper, in colour or in black and white.
 *
 * Each figure is framed twice: once to see where everything it draws lands (diagrams and loads
 * stand off the members by a fixed number of pixels, so the structure's box is not enough), and
 * once for real, with the page cut to that box and a margin. Nothing is clipped and no figure is
 * mostly empty.
 *
 * The 3D viewport has no off-screen path: there a figure is the 3D view, captured with what the
 * figure shows switched on (`capture3D`), and comes out as a PNG.
 */
import { createSvgContext, type Bounds } from './svg-context';
import { inkContext } from './ink-context';
import type { InkStyle } from './paper-ink';
import { modelStore, resultsStore, uiStore } from '../store';
import { t } from '../i18n';
import { unitQ } from '../store/display-units.svelte';
import { resultSetName, composeFigure } from './figure';
import { viewportCanvas } from '../utils/viewport-canvas';
import { freeBodyCapture } from '../canvas/free-body';
import type { DiagramType } from '../store/results.svelte';

/** What a figure shows. */
export type FigureContent = 'model' | 'reactions' | 'moment' | 'shear' | 'axial' | 'deformed' | 'despiece';

export const FIGURE_CONTENTS: readonly FigureContent[] = ['model', 'reactions', 'despiece', 'moment', 'shear', 'axial', 'deformed'];

/** Contents that need results. */
export const NEEDS_RESULTS: ReadonlySet<FigureContent> = new Set(['reactions', 'moment', 'shear', 'axial', 'deformed', 'despiece']);

export interface FigureOptions {
  /** The loads on the model and reactions figures. */
  loads: boolean;
  /** Node and member numbers. */
  numbering: boolean;
  /** Member lengths. */
  lengths: boolean;
  /** Values on diagrams, the deformed shape and reactions. */
  values: boolean;
  /** A heading on each figure saying what it shows. */
  titles: boolean;
  style: InkStyle;
  /** Which pieces the free-body figure shows: the members, the joints, or both. */
  dcl?: 'members' | 'nodes' | 'all';
}

export const DEFAULT_FIGURE_OPTIONS: FigureOptions = {
  loads: true, numbering: false, lengths: false, values: true, titles: true, style: 'color', dcl: 'members',
};

/** What the viewport draws for one figure. */
export interface ExportFrame {
  content: FigureContent;
  options: FigureOptions;
}

export interface FrameView { zoom: number; panX: number; panY: number }

export interface Figure2DRenderer {
  /** Draw `frame` on `ctx` (logical size w × h) with the given world-to-screen view. */
  render(ctx: CanvasRenderingContext2D, w: number, h: number, frame: ExportFrame, view: FrameView): void;
  /** The structure's nodes as the 2D view places them. */
  nodes(): Array<{ x: number; y: number }>;
}

let renderer2D: Figure2DRenderer | null = null;

/** Called by the 2D viewport while it is mounted; returns the unregister. */
export function registerFigure2D(r: Figure2DRenderer): () => void {
  renderer2D = r;
  return () => { if (renderer2D === r) renderer2D = null; };
}

/** Whether figures can be drawn as vectors here (the 2D viewport is up). */
export function canDrawVector(): boolean {
  return renderer2D !== null && !uiStore.is3DWorkspace;
}

/** The viewport's diagram for a content. */
export function diagramFor(content: FigureContent): DiagramType {
  return content === 'model' || content === 'reactions' ? 'none' : content;
}

/** Heading of a figure: what it shows, its unit, and the results it comes from. */
export function figureTitle(content: FigureContent): string {
  const what =
    content === 'model' ? t('figures.model')
    : content === 'reactions' ? `${t('figures.reactions')} [${unitQ('force')}, ${unitQ('moment')}]`
    : content === 'despiece' ? t('figures.despiece')
    : content === 'moment' ? `${t('figures.moment')} [${unitQ('moment')}]`
    : content === 'shear' ? `${t('figures.shear')} [${unitQ('force')}]`
    : content === 'axial' ? `${t('figures.axial')} [${unitQ('force')}]`
    : t('figures.deformed');
  const set = NEEDS_RESULTS.has(content) ? resultsLabel() : '';
  return set ? `${what} — ${set}` : what;
}

/**
 * Which results a figure shows, when there is a choice to name: a combination, the envelope, or
 * one of several load cases. A model with one case and no combinations has nothing to tell apart.
 */
function resultsLabel(): string {
  if (!(resultsStore.results || resultsStore.results3D)) return '';
  const v = resultsStore.activeView;
  if (v === 'combo' || v === 'envelope') return resultSetName();
  const id = resultsStore.activeCaseId;
  if (id !== null && modelStore.loadCases.length > 1) return modelStore.getLoadCaseName(id) || '';
  return '';
}

/** Largest structure, in figure pixels, before the figure is cut to what was drawn. */
const FIT_W = 760, FIT_H = 500;
const PAD = 24;
/** Text grows with the figure: the viewport's sizes are for a screen, a page is read closer. */
const TEXT_SCALE = 1.15;
const TITLE_PX = 15;

/** A figure ready to draw: its size and the routine that paints it at the origin. */
export interface FigurePlan {
  content: FigureContent;
  title: string;
  width: number;
  height: number;
  paint(ctx: CanvasRenderingContext2D): void;
}

/**
 * The free body needs room at every joint for its values, so its figure may run taller: a page
 * holds a tall figure, while a cramped one forces the values away from the cuts they belong to.
 */
const FIT_H_FREE_BODY = 1000;

function fitView(nodes: Array<{ x: number; y: number }>, fitH = FIT_H): FrameView {
  let x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity;
  for (const n of nodes) { x0 = Math.min(x0, n.x); x1 = Math.max(x1, n.x); y0 = Math.min(y0, n.y); y1 = Math.max(y1, n.y); }
  if (!Number.isFinite(x0)) return { zoom: 100, panX: 0, panY: 0 };
  const sx = x1 - x0, sy = y1 - y0;
  const zoom = sx < 1e-9 && sy < 1e-9 ? 100
    : sy < 1e-9 ? FIT_W / sx
    : sx < 1e-9 ? fitH / sy
    : Math.min(FIT_W / sx, fitH / sy);
  // World x0 at screen 0, world y1 (the top) at screen 0.
  return { zoom, panX: -x0 * zoom, panY: y1 * zoom };
}

function textScale(): number {
  return TEXT_SCALE * (uiStore.labelScale || 1);
}

/** Plan one 2D figure; null when there is no 2D viewport to draw it. */
export function plan2D(content: FigureContent, options: FigureOptions): FigurePlan | null {
  const r = renderer2D;
  if (!r) return null;
  const frame: ExportFrame = { content, options };
  const view = fitView(r.nodes(), content === 'despiece' ? FIT_H_FREE_BODY : FIT_H);
  // First pass: where does the drawing land?
  const probe = createSvgContext(FIT_W, FIT_H);
  r.render(inkContext(probe.ctx, options.style, textScale()), FIT_W, FIT_H, frame, view);
  let b: Bounds = probe.bounds();
  if (!(b.x1 > b.x0)) b = { x0: 0, y0: 0, x1: FIT_W, y1: FIT_H };
  const title = options.titles ? figureTitle(content) : '';
  const head = title ? Math.round(TITLE_PX * 2.2) : 0;
  const width = Math.ceil(b.x1 - b.x0 + 2 * PAD);
  const height = Math.ceil(b.y1 - b.y0 + 2 * PAD + head);
  const shifted: FrameView = { zoom: view.zoom, panX: view.panX + PAD - b.x0, panY: view.panY + PAD + head - b.y0 };
  return {
    content, title, width, height,
    paint(ctx) {
      // The ground is set on the context itself: through the ink, white would be read as a light
      // neutral and come out as ink.
      ctx.save(); ctx.fillStyle = options.style === 'screen' ? '#0c1620' : '#ffffff'; ctx.fillRect(0, 0, width, height); ctx.restore();
      const ink = inkContext(ctx, options.style, textScale());
      ink.save();
      if (title) {
        ink.fillStyle = options.style === 'screen' ? '#f4f7fa' : '#c7d3dd';
        ink.font = `bold ${TITLE_PX}px sans-serif`;
        ink.textAlign = 'left';
        ink.textBaseline = 'alphabetic';
        ink.fillText(title, PAD, PAD + TITLE_PX);
      }
      ink.restore();
      r.render(ink, width, height, frame, shifted);
    },
  };
}

/** Gap between figures on a sheet. */
const SHEET_GAP = 18;

/** Several figures, one under the other, as one. */
export function sheetOf(plans: FigurePlan[], style: InkStyle): FigurePlan {
  const width = Math.max(...plans.map((p) => p.width));
  const height = plans.reduce((h, p) => h + p.height, 0) + SHEET_GAP * (plans.length - 1);
  return {
    content: plans[0]!.content, title: '', width, height,
    paint(ctx) {
      ctx.save();
      ctx.fillStyle = style === 'screen' ? '#0c1620' : '#ffffff';
      ctx.fillRect(0, 0, width, height);
      ctx.restore();
      let y = 0;
      for (const p of plans) {
        ctx.save();
        ctx.translate(Math.round((width - p.width) / 2), y);
        p.paint(ctx);
        ctx.restore();
        y += p.height + SHEET_GAP;
      }
    },
  };
}

/** The figure as an SVG document. */
export function toSvg(plan: FigurePlan): string {
  const svg = createSvgContext(plan.width, plan.height);
  plan.paint(svg.ctx);
  return `<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" width="${plan.width}" height="${plan.height}" viewBox="0 0 ${plan.width} ${plan.height}">\n${svg.body()}\n</svg>\n`;
}

/** Pixels per figure pixel in a PNG: about 300 dpi when the figure is printed 16 cm wide. */
export const PNG_SCALE = 3;

/** The figure drawn on a canvas, `scale` device pixels per figure pixel. */
export function toCanvas(plan: FigurePlan, scale = PNG_SCALE): HTMLCanvasElement {
  const c = document.createElement('canvas');
  c.width = Math.max(1, Math.round(plan.width * scale));
  c.height = Math.max(1, Math.round(plan.height * scale));
  const g = c.getContext('2d')!;
  g.scale(scale, scale);
  g.lineJoin = 'round';
  plan.paint(g);
  return c;
}

export function canvasToBlob(c: HTMLCanvasElement): Promise<Blob | null> {
  return new Promise((res) => c.toBlob((b) => res(b), 'image/png'));
}

/* ─── 3D: the view itself, captured with each content shown ─────────────── */

/** The 3D viewport's diagram for a content: Basic 3D draws its moments and shears about z. */
function diagram3D(content: FigureContent): DiagramType {
  if (content === 'moment') return 'momentZ';
  if (content === 'shear') return 'shearY';
  return diagramFor(content);
}

const nextFrame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));

/**
 * Each content as a PNG of the 3D view, captioned. The view's diagram and loads are switched for
 * each capture and put back after.
 */
export async function capture3D(contents: FigureContent[], options: FigureOptions): Promise<Array<{ content: FigureContent; title: string; dataUrl: string }>> {
  const keep = {
    diagram: resultsStore.diagramType, reactions: resultsStore.showReactions, values: resultsStore.showDiagramValues,
    loads: uiStore.showLoads3D, despieceLoads: uiStore.despieceLoadMode, despieceMode: uiStore.despieceVectorMode,
  };
  uiStore.despieceVectorMode = options.dcl ?? 'members';
  freeBodyCapture.active = true;
  // A free body is in equilibrium only with its loads on it.
  if (uiStore.despieceLoadMode === 'off') uiStore.despieceLoadMode = 'all';
  // Where the reader is standing (the viewport publishes it as the camera moves), to come back to.
  const camera = { position: { ...uiStore.cameraPosition3D }, target: { ...uiStore.cameraTarget3D }, orthoZoom: uiStore.cameraOrthoZoom3D };
  const out: Array<{ content: FigureContent; title: string; dataUrl: string }> = [];
  // The whole model in each figure, whatever the view was zoomed to.
  window.dispatchEvent(new CustomEvent('stabileo-zoom-to-fit'));
  try {
    for (const content of contents) {
      resultsStore.diagramType = diagram3D(content);
      resultsStore.showReactions = content === 'reactions' || content === 'despiece';
      resultsStore.showDiagramValues = options.values;
      uiStore.showLoads3D = options.loads && (content === 'model' || content === 'reactions');
      // Two frames: one for the scene to take the change, one for it to be drawn.
      await nextFrame(); await nextFrame(); await nextFrame();
      const c = withOverlay(viewportCanvas());
      if (!c) continue;
      const title = options.titles ? figureTitle(content) : '';
      const dataUrl = composeFigure(c, title || (modelStore.model.name ?? ''), null);
      if (dataUrl) out.push({ content, title, dataUrl });
    }
  } finally {
    resultsStore.diagramType = keep.diagram;
    resultsStore.showReactions = keep.reactions;
    resultsStore.showDiagramValues = keep.values;
    uiStore.showLoads3D = keep.loads;
    uiStore.despieceLoadMode = keep.despieceLoads;
    uiStore.despieceVectorMode = keep.despieceMode;
    freeBodyCapture.active = false;
    window.dispatchEvent(new CustomEvent('stabileo-camera-set', { detail: camera }));
  }
  return out;
}

/**
 * The 3D view with what is drawn over it: the despiece is a 2D canvas laid over the WebGL one,
 * so a capture of the WebGL canvas alone would leave it out.
 */
function withOverlay(gl: HTMLCanvasElement | null): HTMLCanvasElement | null {
  if (!gl) return null;
  const over = document.querySelector('.viewport-container canvas.fb-overlay') as HTMLCanvasElement | null;
  if (!over || resultsStore.diagramType !== 'despiece' || !over.width) return gl;
  const c = document.createElement('canvas');
  c.width = gl.width; c.height = gl.height;
  const g = c.getContext('2d');
  if (!g) return gl;
  try { g.drawImage(gl, 0, 0); g.drawImage(over, 0, 0, c.width, c.height); } catch { return gl; }
  return c;
}

/** Captured 3D figures, one under the other, as one PNG. */
export async function stackPngs(urls: string[]): Promise<Blob | null> {
  const imgs = await Promise.all(urls.map((u) => new Promise<HTMLImageElement>((res, rej) => {
    const i = new Image(); i.onload = () => res(i); i.onerror = rej; i.src = u;
  })));
  const gap = 24;
  const c = document.createElement('canvas');
  c.width = Math.max(...imgs.map((i) => i.width));
  c.height = imgs.reduce((h, i) => h + i.height, 0) + gap * (imgs.length - 1);
  const g = c.getContext('2d')!;
  g.fillStyle = '#ffffff';
  g.fillRect(0, 0, c.width, c.height);
  let y = 0;
  for (const i of imgs) { g.drawImage(i, Math.round((c.width - i.width) / 2), y); y += i.height + gap; }
  return canvasToBlob(c);
}
