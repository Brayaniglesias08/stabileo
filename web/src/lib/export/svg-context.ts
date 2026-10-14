/**
 * A drawing context that writes SVG.
 *
 * The 2D viewport draws with the Canvas 2D API. This object answers the part of that API the
 * drawing uses — paths, arcs, rectangles, text, dashes, transforms, save and restore — and keeps
 * each shape as an SVG element instead of pixels, so the same drawing code produces a vector
 * figure: lines stay sharp at any size, and text stays text.
 *
 * Transforms are applied to the points as they are added, as a canvas does, so the SVG needs no
 * nested groups. Arcs become short polylines. It also keeps the bounding box of everything drawn,
 * which is how an export frames its figure to what is actually on it.
 */
import { cssOf, parseColor } from './paper-ink';

type M = [number, number, number, number, number, number];

interface State {
  m: M;
  fillStyle: string;
  strokeStyle: string;
  lineWidth: number;
  dash: number[];
  lineCap: CanvasLineCap;
  lineJoin: CanvasLineJoin;
  font: string;
  textAlign: CanvasTextAlign;
  textBaseline: CanvasTextBaseline;
  globalAlpha: number;
}

export interface Bounds { x0: number; y0: number; x1: number; y1: number }

const fmt = (v: number) => (Math.round(v * 100) / 100).toString();
const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** Font parts out of a CSS font shorthand ("bold 11px sans-serif"). */
export function parseFont(font: string): { size: number; weight: string; style: string; family: string } {
  const m = /(\d+(?:\.\d+)?)px\s*(?:\/\s*[\d.]+\w*)?\s*(.*)$/.exec(font);
  const size = m ? parseFloat(m[1]!) : 10;
  let family = (m?.[2] ?? 'sans-serif').trim() || 'sans-serif';
  if (/^sans-serif$/i.test(family)) family = 'Helvetica, Arial, sans-serif';
  const head = m ? font.slice(0, m.index) : '';
  const weight = /\b(bold|[6-9]00)\b/.test(head) ? 'bold' : 'normal';
  const style = /\bitalic\b/.test(head) ? 'italic' : 'normal';
  return { size, weight, style, family };
}

let measurer: CanvasRenderingContext2D | null | undefined;
function measureWith(font: string, text: string): number {
  if (measurer === undefined) {
    try { measurer = typeof document !== 'undefined' ? document.createElement('canvas').getContext('2d') : null; } catch { measurer = null; }
  }
  if (measurer) { measurer.font = font; return measurer.measureText(text).width; }
  return parseFont(font).size * 0.56 * text.length;
}

export function createSvgContext(width: number, height: number) {
  const out: string[] = [];
  const stack: State[] = [];
  let st: State = {
    m: [1, 0, 0, 1, 0, 0], fillStyle: '#000', strokeStyle: '#000', lineWidth: 1, dash: [],
    lineCap: 'butt', lineJoin: 'miter', font: '10px sans-serif', textAlign: 'start', textBaseline: 'alphabetic', globalAlpha: 1,
  };
  /** The current path, in output coordinates: subpaths of points, each maybe closed. */
  let path: Array<{ pts: Array<[number, number]>; closed: boolean }> = [];
  const box: Bounds = { x0: Infinity, y0: Infinity, x1: -Infinity, y1: -Infinity };

  const tx = (x: number, y: number): [number, number] => {
    const [a, b, c, d, e, f] = st.m;
    return [a * x + c * y + e, b * x + d * y + f];
  };
  const scaleOf = () => Math.sqrt(Math.abs(st.m[0] * st.m[3] - st.m[1] * st.m[2])) || 1;
  const grow = (x: number, y: number, pad = 0) => {
    if (x - pad < box.x0) box.x0 = x - pad;
    if (y - pad < box.y0) box.y0 = y - pad;
    if (x + pad > box.x1) box.x1 = x + pad;
    if (y + pad > box.y1) box.y1 = y + pad;
  };
  const current = () => path[path.length - 1];
  const lineTo = (x: number, y: number) => {
    const p = tx(x, y);
    if (!current()) path.push({ pts: [], closed: false });
    current()!.pts.push(p);
  };

  /** A colour as SVG paint attributes, with the alpha folded into an opacity. */
  function paint(kind: 'fill' | 'stroke', css: string): string {
    const c = parseColor(css);
    if (!c) return `${kind}="${esc(css)}"`;
    const a = c.a * st.globalAlpha;
    return `${kind}="${cssOf({ ...c, a: 1 })}"${a < 1 ? ` ${kind}-opacity="${fmt(a)}"` : ''}`;
  }
  const isNone = (css: string) => { const c = parseColor(css); return !!c && c.a * st.globalAlpha < 0.003; };

  function d(): string {
    return path.filter((sp) => sp.pts.length > 0).map((sp) =>
      'M' + sp.pts.map(([x, y]) => `${fmt(x)} ${fmt(y)}`).join('L') + (sp.closed ? 'Z' : '')).join('');
  }

  function strokeAttrs(): string {
    const k = scaleOf();
    const w = st.lineWidth * k;
    let a = `${paint('stroke', st.strokeStyle)} stroke-width="${fmt(w)}"`;
    if (st.dash.length) a += ` stroke-dasharray="${st.dash.map((v) => fmt(v * k)).join(',')}"`;
    if (st.lineCap !== 'butt') a += ` stroke-linecap="${st.lineCap}"`;
    if (st.lineJoin !== 'miter') a += ` stroke-linejoin="${st.lineJoin}"`;
    return a;
  }

  function arcPoints(cx: number, cy: number, r: number, a0: number, a1: number, ccw: boolean): void {
    let sweep = a1 - a0;
    if (!ccw && sweep < 0) sweep = (sweep % (2 * Math.PI)) + 2 * Math.PI;
    if (ccw && sweep > 0) sweep = (sweep % (2 * Math.PI)) - 2 * Math.PI;
    if (!ccw && a1 - a0 >= 2 * Math.PI) sweep = 2 * Math.PI;
    if (ccw && a0 - a1 >= 2 * Math.PI) sweep = -2 * Math.PI;
    const n = Math.max(6, Math.min(72, Math.ceil(Math.abs(sweep) * Math.max(4, r * scaleOf()) / 4)));
    for (let i = 0; i <= n; i++) {
      const a = a0 + (sweep * i) / n;
      lineTo(cx + r * Math.cos(a), cy + r * Math.sin(a));
    }
  }

  const ctx = {
    canvas: { width, height },
    get fillStyle() { return st.fillStyle; }, set fillStyle(v: string) { if (typeof v === 'string') st.fillStyle = v; },
    get strokeStyle() { return st.strokeStyle; }, set strokeStyle(v: string) { if (typeof v === 'string') st.strokeStyle = v; },
    get lineWidth() { return st.lineWidth; }, set lineWidth(v: number) { if (v > 0) st.lineWidth = v; },
    get lineCap() { return st.lineCap; }, set lineCap(v: CanvasLineCap) { st.lineCap = v; },
    get lineJoin() { return st.lineJoin; }, set lineJoin(v: CanvasLineJoin) { st.lineJoin = v; },
    get font() { return st.font; }, set font(v: string) { st.font = v; },
    get textAlign() { return st.textAlign; }, set textAlign(v: CanvasTextAlign) { st.textAlign = v; },
    get textBaseline() { return st.textBaseline; }, set textBaseline(v: CanvasTextBaseline) { st.textBaseline = v; },
    get globalAlpha() { return st.globalAlpha; }, set globalAlpha(v: number) { st.globalAlpha = Math.max(0, Math.min(1, v)); },
    // Shadows and composition are screen effects; a printed figure does without them.
    shadowColor: 'transparent', shadowBlur: 0, shadowOffsetX: 0, shadowOffsetY: 0,
    globalCompositeOperation: 'source-over', imageSmoothingEnabled: true, miterLimit: 10, lineDashOffset: 0,

    save() { stack.push({ ...st, m: [...st.m] as M, dash: [...st.dash] }); },
    restore() { const s = stack.pop(); if (s) st = s; },
    translate(x: number, y: number) { const [a, b, c, dd, e, f] = st.m; st.m = [a, b, c, dd, e + a * x + c * y, f + b * x + dd * y]; },
    scale(sx: number, sy: number) { const [a, b, c, dd, e, f] = st.m; st.m = [a * sx, b * sx, c * sy, dd * sy, e, f]; },
    rotate(t: number) {
      const cs = Math.cos(t), sn = Math.sin(t); const [a, b, c, dd, e, f] = st.m;
      st.m = [a * cs + c * sn, b * cs + dd * sn, -a * sn + c * cs, -b * sn + dd * cs, e, f];
    },
    transform(a2: number, b2: number, c2: number, d2: number, e2: number, f2: number) {
      const [a, b, c, dd, e, f] = st.m;
      st.m = [a * a2 + c * b2, b * a2 + dd * b2, a * c2 + c * d2, b * c2 + dd * d2, a * e2 + c * f2 + e, b * e2 + dd * f2 + f];
    },
    setTransform(a: number | DOMMatrix2DInit = 1, b = 0, c = 0, dd = 1, e = 0, f = 0) {
      if (typeof a === 'object') st.m = [a.a ?? 1, a.b ?? 0, a.c ?? 0, a.d ?? 1, a.e ?? 0, a.f ?? 0];
      else st.m = [a, b, c, dd, e, f];
    },
    resetTransform() { st.m = [1, 0, 0, 1, 0, 0]; },
    getTransform() { const [a, b, c, dd, e, f] = st.m; return { a, b, c, d: dd, e, f }; },
    setLineDash(seg: number[]) { st.dash = seg.filter((v) => Number.isFinite(v)); },
    getLineDash() { return [...st.dash]; },

    beginPath() { path = []; },
    moveTo(x: number, y: number) { path.push({ pts: [tx(x, y)], closed: false }); },
    lineTo,
    closePath() { const c = current(); if (c && c.pts.length) { c.closed = true; path.push({ pts: [c.pts[0]!], closed: false }); } },
    arc(cx: number, cy: number, r: number, a0: number, a1: number, ccw = false) { arcPoints(cx, cy, r, a0, a1, ccw); },
    ellipse(cx: number, cy: number, rx: number, ry: number, rot: number, a0: number, a1: number, ccw = false) {
      ctx.save(); ctx.translate(cx, cy); ctx.rotate(rot); ctx.scale(1, ry / (rx || 1)); arcPoints(0, 0, rx, a0, a1, ccw); ctx.restore();
    },
    quadraticCurveTo(cpx: number, cpy: number, x: number, y: number) {
      const last = current()?.pts.at(-1);
      if (!last) { lineTo(x, y); return; }
      // The last point back in user space, to sample the curve where it was described.
      const [a, b, c, dd, e, f] = st.m; const det = a * dd - b * c || 1;
      const ux = (dd * (last[0] - e) - c * (last[1] - f)) / det, uy = (-b * (last[0] - e) + a * (last[1] - f)) / det;
      for (let i = 1; i <= 12; i++) { const t = i / 12, u = 1 - t; lineTo(u * u * ux + 2 * u * t * cpx + t * t * x, u * u * uy + 2 * u * t * cpy + t * t * y); }
    },
    bezierCurveTo(c1x: number, c1y: number, c2x: number, c2y: number, x: number, y: number) {
      const last = current()?.pts.at(-1);
      if (!last) { lineTo(x, y); return; }
      const [a, b, c, dd, e, f] = st.m; const det = a * dd - b * c || 1;
      const ux = (dd * (last[0] - e) - c * (last[1] - f)) / det, uy = (-b * (last[0] - e) + a * (last[1] - f)) / det;
      for (let i = 1; i <= 16; i++) {
        const t = i / 16, u = 1 - t;
        lineTo(u ** 3 * ux + 3 * u * u * t * c1x + 3 * u * t * t * c2x + t ** 3 * x, u ** 3 * uy + 3 * u * u * t * c1y + 3 * u * t * t * c2y + t ** 3 * y);
      }
    },
    arcTo(_x1: number, _y1: number, x2: number, y2: number) { lineTo(x2, y2); },
    rect(x: number, y: number, w: number, h: number) {
      ctx.moveTo(x, y); lineTo(x + w, y); lineTo(x + w, y + h); lineTo(x, y + h); ctx.closePath();
    },
    roundRect(x: number, y: number, w: number, h: number, radii: number | number[] = 0) {
      const r0 = Math.max(0, Math.min(Array.isArray(radii) ? (radii[0] ?? 0) : radii, Math.abs(w) / 2, Math.abs(h) / 2));
      if (r0 === 0) { ctx.rect(x, y, w, h); return; }
      ctx.moveTo(x + r0, y);
      lineTo(x + w - r0, y); arcPoints(x + w - r0, y + r0, r0, -Math.PI / 2, 0, false);
      lineTo(x + w, y + h - r0); arcPoints(x + w - r0, y + h - r0, r0, 0, Math.PI / 2, false);
      lineTo(x + r0, y + h); arcPoints(x + r0, y + h - r0, r0, Math.PI / 2, Math.PI, false);
      lineTo(x, y + r0); arcPoints(x + r0, y + r0, r0, Math.PI, 1.5 * Math.PI, false);
      ctx.closePath();
    },
    clip() { /* the figure is framed to its content; nothing it draws is clipped */ },

    fill() {
      if (isNone(st.fillStyle)) return;
      const dd = d();
      if (!dd) return;
      for (const sp of path) for (const [x, y] of sp.pts) grow(x, y);
      out.push(`<path d="${dd}" ${paint('fill', st.fillStyle)} stroke="none"/>`);
    },
    stroke() {
      if (isNone(st.strokeStyle)) return;
      const dd = d();
      if (!dd) return;
      const pad = (st.lineWidth * scaleOf()) / 2;
      for (const sp of path) for (const [x, y] of sp.pts) grow(x, y, pad);
      out.push(`<path d="${dd}" fill="none" ${strokeAttrs()}/>`);
    },
    fillRect(x: number, y: number, w: number, h: number) {
      const keep = path; path = []; ctx.rect(x, y, w, h); ctx.fill(); path = keep;
    },
    strokeRect(x: number, y: number, w: number, h: number) {
      const keep = path; path = []; ctx.rect(x, y, w, h); ctx.stroke(); path = keep;
    },
    clearRect() { /* the figure is drawn on a fresh page */ },

    measureText(text: string) {
      const w = measureWith(st.font, text);
      const { size } = parseFont(st.font);
      return { width: w, actualBoundingBoxAscent: size * 0.75, actualBoundingBoxDescent: size * 0.25, fontBoundingBoxAscent: size * 0.8, fontBoundingBoxDescent: size * 0.2 } as TextMetrics;
    },
    fillText(text: string, x: number, y: number) { text_(String(text), x, y, 'fill'); },
    strokeText(text: string, x: number, y: number) { text_(String(text), x, y, 'stroke'); },
    drawImage() { /* no raster in a vector figure */ },
  };

  function text_(text: string, x: number, y: number, mode: 'fill' | 'stroke'): void {
    if (!text) return;
    const style = mode === 'fill' ? st.fillStyle : st.strokeStyle;
    if (isNone(style)) return;
    const f = parseFont(st.font);
    // Baselines as offsets, which every SVG reader honours (dominant-baseline is not).
    const dy = st.textBaseline === 'middle' ? f.size * 0.35
      : st.textBaseline === 'top' || st.textBaseline === 'hanging' ? f.size * 0.8
      : st.textBaseline === 'bottom' || st.textBaseline === 'ideographic' ? -f.size * 0.22 : 0;
    const anchor = st.textAlign === 'center' ? 'middle' : st.textAlign === 'right' || st.textAlign === 'end' ? 'end' : 'start';
    const [px, py] = tx(x, y);
    const k = scaleOf();
    const rot = Math.atan2(st.m[1], st.m[0]) * 180 / Math.PI;
    const size = f.size * k;
    const w = measureWith(st.font, text) * k;
    const x0 = anchor === 'middle' ? -w / 2 : anchor === 'end' ? -w : 0;
    // The text's box, turned with it, for the frame.
    const cr = Math.cos(rot * Math.PI / 180), sr = Math.sin(rot * Math.PI / 180);
    for (const [ux, uy] of [[x0, dy * k - size * 0.8], [x0 + w, dy * k - size * 0.8], [x0, dy * k + size * 0.25], [x0 + w, dy * k + size * 0.25]] as const) {
      grow(px + ux * cr - uy * sr, py + ux * sr + uy * cr);
    }
    const at = Math.abs(rot) > 0.01
      ? `transform="translate(${fmt(px)} ${fmt(py)}) rotate(${fmt(rot)})" x="0" y="${fmt(dy * k)}"`
      : `x="${fmt(px)}" y="${fmt(py + dy * k)}"`;
    const paintAttr = mode === 'fill'
      ? `${paint('fill', style)}`
      : `fill="none" ${paint('stroke', style)} stroke-width="${fmt(st.lineWidth * k)}" stroke-linejoin="round"`;
    out.push(`<text ${at} font-family="${esc(f.family)}" font-size="${fmt(size)}"${f.weight !== 'normal' ? ` font-weight="${f.weight}"` : ''}${f.style !== 'normal' ? ` font-style="${f.style}"` : ''} text-anchor="${anchor}" ${paintAttr}>${esc(text)}</text>`);
  }

  return {
    /** The context, to hand to code that draws on a canvas. */
    ctx: ctx as unknown as CanvasRenderingContext2D,
    /** Everything drawn so far, as SVG elements. */
    body: () => out.join('\n'),
    /** The box around everything drawn; empty (x0 > x1) when nothing was. */
    bounds: (): Bounds => ({ ...box }),
  };
}
