/**
 * The viewport's colours, put on paper.
 *
 * The 2D viewport draws light ink on a dark ground: near-white members and text, dark boxes
 * behind labels, pale tints of the brand colours for values. An exported figure goes on white
 * paper, so every colour the drawing sets is mapped as it is set — light neutrals become ink,
 * dark grounds become paper, light accents are darkened until they read on white. In black and
 * white every colour keeps only how dark it is.
 *
 * Mapping colours at the context, rather than giving the drawing a second palette, keeps one
 * drawing: what the export shows is what the viewport draws, including whatever the despiece or a
 * diagram learns to draw later.
 */

export type InkStyle = 'color' | 'bw' | 'screen';

export interface Rgba { r: number; g: number; b: number; a: number }

const NAMED: Record<string, string> = {
  white: '#ffffff', black: '#000000', red: '#ff0000', green: '#008000', blue: '#0000ff',
  orange: '#ffa500', yellow: '#ffff00', gray: '#808080', grey: '#808080', transparent: 'rgba(0,0,0,0)',
};

/** A CSS colour as RGBA (0–255, alpha 0–1); null for what is not a plain colour. */
export function parseColor(css: string): Rgba | null {
  const s = (NAMED[css.trim().toLowerCase()] ?? css).trim();
  let m = /^#([0-9a-f]{3,8})$/i.exec(s);
  if (m) {
    let h = m[1]!;
    if (h.length === 3 || h.length === 4) h = [...h].map((c) => c + c).join('');
    if (h.length !== 6 && h.length !== 8) return null;
    const n = (i: number) => parseInt(h.slice(i, i + 2), 16);
    return { r: n(0), g: n(2), b: n(4), a: h.length === 8 ? n(6) / 255 : 1 };
  }
  m = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:[\s,/]+([\d.]+%?))?\s*\)$/i.exec(s);
  if (m) {
    const a = m[4] === undefined ? 1 : m[4].endsWith('%') ? parseFloat(m[4]) / 100 : parseFloat(m[4]);
    return { r: +m[1]!, g: +m[2]!, b: +m[3]!, a };
  }
  return null;
}

function toHsl({ r, g, b }: Rgba): { h: number; s: number; l: number } {
  const R = r / 255, G = g / 255, B = b / 255;
  const max = Math.max(R, G, B), min = Math.min(R, G, B);
  const l = (max + min) / 2;
  if (max === min) return { h: 0, s: 0, l };
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === R ? ((G - B) / d + (G < B ? 6 : 0)) / 6 : max === G ? ((B - R) / d + 2) / 6 : ((R - G) / d + 4) / 6;
  return { h, s, l };
}

function fromHsl(h: number, s: number, l: number, a: number): Rgba {
  if (s === 0) { const v = Math.round(l * 255); return { r: v, g: v, b: v, a }; }
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s, p = 2 * l - q;
  const f = (t: number) => {
    if (t < 0) t += 1; if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  return { r: Math.round(f(h + 1 / 3) * 255), g: Math.round(f(h) * 255), b: Math.round(f(h - 1 / 3) * 255), a };
}

/** The colour as a CSS string the SVG and canvas both read. */
export function cssOf(c: Rgba): string {
  const a = Math.round(c.a * 1000) / 1000;
  return a >= 1 ? `rgb(${c.r},${c.g},${c.b})` : `rgba(${c.r},${c.g},${c.b},${a})`;
}

/** The ink the figure uses: near black, a touch of the app's blue. */
const INK = { r: 22, g: 30, b: 40 };

/**
 * A viewport colour as it is drawn on paper.
 *
 * - A light neutral (member, text, node) is ink.
 * - A dark ground (the canvas, the box behind a label) is paper.
 * - A middle grey stays grey, as dark as it was light.
 * - A colour keeps its hue; drawn solid it is darkened until it reads on white, while a pale
 *   translucent tint (a diagram's fill) is left as it is.
 */
export function inkColor(css: string, style: InkStyle): string {
  if (style === 'screen') return css;
  const c = parseColor(css);
  if (!c) return css;
  const { h, s, l } = toHsl(c);
  // A dark ground (the canvas, a label's box or halo), whatever its tint: paper.
  if (l < 0.18) return cssOf({ r: 255, g: 255, b: 255, a: c.a });
  // Near white, whatever its tint (HSL saturation runs high this close to white): ink.
  if (l > 0.88 && c.a >= 0.5) return style === 'bw' ? cssOf({ r: 20, g: 20, b: 20, a: c.a }) : cssOf({ ...INK, a: c.a });
  if (style === 'bw') {
    const lum = (0.299 * c.r + 0.587 * c.g + 0.114 * c.b) / 255;
    // A tint stays a light grey; anything drawn solid becomes ink, greys stay grey.
    if (c.a < 0.5) return cssOf({ r: 0, g: 0, b: 0, a: Math.min(0.35, c.a * 0.75) });
    const v = s < 0.25 && l > 0.25 && l < 0.75 ? Math.round(255 * Math.max(0.3, Math.min(0.55, 1 - lum))) : 20;
    return cssOf({ r: v, g: v, b: v, a: c.a });
  }
  if (s < 0.25) {
    if (l > 0.75) return cssOf({ ...INK, a: c.a });
    const v = Math.round(255 * Math.max(0.28, Math.min(0.55, 1 - l)));
    return cssOf({ r: v, g: v, b: v, a: c.a });
  }
  if (c.a < 0.5) return cssOf(c);
  return cssOf(fromHsl(h, Math.min(1, s), Math.min(l, 0.38), c.a));
}
