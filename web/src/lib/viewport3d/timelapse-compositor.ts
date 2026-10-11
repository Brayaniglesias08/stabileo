/**
 * The build timelapse's picture: the 3D frame with its texts, on one 2D canvas.
 *
 * The WebGL canvas is copied in right after each render (its buffer is still valid then) and
 * the title, the count, the progress line and the brand are drawn over it. The same canvas is
 * what the viewport shows while the timelapse is on and what a recording captures, so the video
 * is exactly what was on screen.
 */
import type { TimelapseStyle } from '../store/build-timelapse.svelte';

export interface OverlayInfo {
  title: string;
  /** "232 nodes · 709 members · 28 supports". */
  stats: string;
  /** "Members 312 / 709". */
  counter: string;
  /** 0 to 1, how far the build has got. */
  progress: number;
  /** 0 to 1, how far the closing card has faded in (during the hold at the end). */
  closing: number;
  style: TimelapseStyle;
  texts: boolean;
}

const ACCENT = '#e5482a';
const FONT = "'Space Grotesk', 'IBM Plex Sans', system-ui, sans-serif";
const MONO = "'IBM Plex Mono', ui-monospace, monospace";

function palette(style: TimelapseStyle) {
  return style === 'studio'
    ? { fg: '#122030', fg2: 'rgba(18, 32, 48, 0.62)', track: 'rgba(18, 32, 48, 0.14)' }
    : { fg: '#f3f6f9', fg2: 'rgba(243, 246, 249, 0.66)', track: 'rgba(243, 246, 249, 0.16)' };
}

/** The brand: the outlined triangle and the name. */
function brand(ctx: CanvasRenderingContext2D, x: number, y: number, size: number, fg: string, align: 'left' | 'right' | 'center'): void {
  ctx.font = `600 ${size}px ${FONT}`;
  const word = 'Stabileo';
  const tw = ctx.measureText(word).width;
  const tri = size * 0.9, gap = size * 0.35;
  const width = tri + gap + tw;
  const x0 = align === 'left' ? x : align === 'right' ? x - width : x - width / 2;
  ctx.save();
  ctx.strokeStyle = ACCENT;
  ctx.lineWidth = Math.max(1.5, size * 0.09);
  ctx.lineJoin = 'round';
  ctx.beginPath();
  ctx.moveTo(x0 + tri / 2, y - tri * 0.86);
  ctx.lineTo(x0 + tri, y);
  ctx.lineTo(x0, y);
  ctx.closePath();
  ctx.stroke();
  ctx.fillStyle = fg;
  ctx.textAlign = 'left';
  ctx.textBaseline = 'alphabetic';
  ctx.fillText(word, x0 + tri + gap, y);
  ctx.restore();
}

export function createTimelapseCompositor() {
  const canvas = document.createElement('canvas');
  canvas.className = 'tl-frame';
  const ctx = canvas.getContext('2d', { alpha: false })!;

  function resize(w: number, h: number): void {
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
  }

  function draw(src: HTMLCanvasElement, o: OverlayInfo): void {
    const w = canvas.width, h = canvas.height;
    if (!w || !h) return;
    ctx.drawImage(src, 0, 0, w, h);
    if (!o.texts) return;

    const u = Math.min(w, h) / 1080;
    const pad = 56 * u;
    const tall = h > w * 1.2;
    const c = palette(o.style);
    ctx.save();
    ctx.textBaseline = 'alphabetic';

    // Title and what the model holds, top left (top centre on a tall frame). They step aside
    // for the closing card.
    const head = 1 - o.closing;
    if (head > 0.01) {
      ctx.globalAlpha = head;
      ctx.textAlign = tall ? 'center' : 'left';
      const hx = tall ? w / 2 : pad;
      ctx.fillStyle = c.fg;
      ctx.font = `600 ${Math.round(46 * u)}px ${FONT}`;
      ctx.fillText(o.title, hx, pad + 40 * u, w - 2 * pad);
      ctx.fillStyle = c.fg2;
      ctx.font = `400 ${Math.round(24 * u)}px ${FONT}`;
      ctx.fillText(o.stats, hx, pad + 80 * u, w - 2 * pad);
    }

    // The count and the progress line along the foot of the frame.
    ctx.globalAlpha = 1;
    const barY = h - pad;
    const barW = tall ? w - 2 * pad : w * 0.36;
    const barX = pad;
    ctx.fillStyle = c.track;
    ctx.fillRect(barX, barY, barW, 5 * u);
    ctx.fillStyle = ACCENT;
    ctx.fillRect(barX, barY, barW * Math.max(0, Math.min(1, o.progress)), 5 * u);
    ctx.fillStyle = c.fg2;
    ctx.textAlign = 'left';
    ctx.font = `500 ${Math.round(22 * u)}px ${MONO}`;
    ctx.fillText(o.counter, barX, barY - 16 * u);

    // The brand, bottom right; on a tall frame it sits above the bar, centred.
    if (tall) brand(ctx, w / 2, barY - 70 * u, 40 * u, c.fg, 'center');
    else brand(ctx, w - pad, barY + 5 * u, 36 * u, c.fg, 'right');

    // Closing card: the title large in the middle, the figures under it, the address.
    if (o.closing > 0.01) {
      ctx.globalAlpha = o.closing;
      const cy = tall ? h * 0.2 : h * 0.16;
      ctx.textAlign = 'center';
      ctx.fillStyle = c.fg;
      ctx.font = `700 ${Math.round(64 * u)}px ${FONT}`;
      ctx.fillText(o.title, w / 2, cy, w - 2 * pad);
      ctx.fillStyle = c.fg2;
      ctx.font = `400 ${Math.round(28 * u)}px ${FONT}`;
      ctx.fillText(o.stats, w / 2, cy + 50 * u, w - 2 * pad);
      ctx.fillStyle = ACCENT;
      ctx.font = `500 ${Math.round(26 * u)}px ${MONO}`;
      ctx.fillText('stabileo.com', w / 2, cy + 96 * u);
    }
    ctx.restore();
  }

  return { canvas, resize, draw };
}
