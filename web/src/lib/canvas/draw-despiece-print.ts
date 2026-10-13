/**
 * The despiece laid out for print: the free-body diagram as a textbook draws it.
 *
 * The on-screen despiece pulls every member a fraction of its length off its joints and draws,
 * at every cut, the action on the member and the equal and opposite one on the joint, each with
 * its signed value. On a page that is two sets of arrows and numbers in the same few
 * millimetres, and the values carry a sign on top of an arrow that already shows the sense.
 *
 * Here:
 * - A member is cut back by a FIXED distance on the page (not a fraction of its length), so
 *   every joint has the same room and short members do not crowd it.
 * - A joint where only one member arrives is not cut: the member is the free body there, and
 *   the reactions and the loads at that joint act on it directly, as a book draws a cantilever.
 * - Each arrow points the way the action acts, and its value is a magnitude: the sense is the
 *   arrow's, never a minus sign.
 * - Every value is queued on a label layout with the members and the arrows as obstacles, so
 *   no value is written over another or over the drawing.
 * - The pieces shown are the members, the joints, or both (`mode`).
 *
 * The forces themselves come from `computeDespieceVectors`, the same routine the screen uses:
 * only where they are drawn changes.
 */
import {
  computeDespieceVectors,
  type DespieceBasis, type DespieceElement, type DespieceElementForces, type DespieceElementSpan,
  type DespieceNode, type DespieceReaction, type DespieceVectorMode,
} from './draw-despiece';
import { drawMomentSymbol } from './draw-loads';
import type { LabelCollector } from './label-layout';

/** How far a member is cut back from a joint, on the page. */
export const PRINT_GAP_PX = 58;
const ARROW_PX = 34;
const ARC_PX = 12;
const HEAD_PX = 7;

const COL = { axial: '#ff7070', shear: '#7fd4cc', moment: '#ffd166', reaction: '#00e676', member: '#c7d3dd', joint: '#8fa3b3', link: '#5a6478' };

export interface DespiecePrintCtx {
  ctx: CanvasRenderingContext2D;
  worldToScreen: (wx: number, wy: number) => { x: number; y: number };
  /** Pixels per metre, to turn the page gap into a world distance. */
  zoom: number;
  elements: DespieceElement[];
  getNode: (id: number) => DespieceNode | undefined;
  getElementForces: (id: number) => DespieceElementForces | undefined;
  reactions: Map<number, DespieceReaction>;
  /** A magnitude with its unit. */
  fmt: (v: number, q?: 'force' | 'moment') => string;
  basis?: DespieceBasis;
  mode: DespieceVectorMode;
  labels: LabelCollector;
  labelPx?: number;
}

/** How many members meet at each node. */
export function valence(elements: Iterable<DespieceElement>): Map<number, number> {
  const v = new Map<number, number>();
  for (const e of elements) {
    v.set(e.nodeI, (v.get(e.nodeI) ?? 0) + 1);
    v.set(e.nodeJ, (v.get(e.nodeJ) ?? 0) + 1);
  }
  return v;
}

/**
 * Where a member's free body runs, in world coordinates: cut back `gapPx` on the page at a
 * joint shared with another member, left whole at a joint it has to itself. A member too short
 * for two gaps keeps a third of its length.
 */
export function printSpan(ni: DespieceNode, nj: DespieceNode, cutI: boolean, cutJ: boolean, zoom: number, gapPx = PRINT_GAP_PX): DespieceElementSpan {
  const len = Math.hypot(nj.x - ni.x, nj.y - ni.y);
  const ux = len > 0 ? (nj.x - ni.x) / len : 1, uy = len > 0 ? (nj.y - ni.y) / len : 0;
  const g = Math.min(gapPx / Math.max(zoom, 1e-9), len / 3);
  const gi = cutI ? g : 0, gj = cutJ ? g : 0;
  const aI = { x: ni.x + ux * gi, y: ni.y + uy * gi };
  const aJ = { x: nj.x - ux * gj, y: nj.y - uy * gj };
  return { aI, aJ, lenOrig: len, lenShrunk: Math.hypot(aJ.x - aI.x, aJ.y - aI.y) };
}

/** The spans of every member for the print layout. */
export function printSpans(d: Pick<DespiecePrintCtx, 'elements' | 'getNode' | 'zoom'>): Map<number, DespieceElementSpan> {
  const val = valence(d.elements);
  const out = new Map<number, DespieceElementSpan>();
  for (const e of d.elements) {
    const ni = d.getNode(e.nodeI), nj = d.getNode(e.nodeJ);
    if (!ni || !nj) continue;
    out.set(e.id, printSpan(ni, nj, (val.get(e.nodeI) ?? 0) > 1, (val.get(e.nodeJ) ?? 0) > 1, d.zoom));
  }
  return out;
}

function arrow(ctx: CanvasRenderingContext2D, x0: number, y0: number, x1: number, y1: number, color: string): void {
  const dx = x1 - x0, dy = y1 - y0, d = Math.hypot(dx, dy);
  if (d < 0.5) return;
  const a = Math.atan2(dy, dx);
  ctx.strokeStyle = color; ctx.fillStyle = color; ctx.lineWidth = 1.8;
  ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x1 - Math.cos(a) * HEAD_PX * 0.6, y1 - Math.sin(a) * HEAD_PX * 0.6); ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x1 - HEAD_PX * Math.cos(a - 0.38), y1 - HEAD_PX * Math.sin(a - 0.38));
  ctx.lineTo(x1 - HEAD_PX * Math.cos(a + 0.38), y1 - HEAD_PX * Math.sin(a + 0.38));
  ctx.closePath(); ctx.fill();
}

export function drawDespiecePrint(d: DespiecePrintCtx): void {
  const { ctx, labels } = d;
  const font = `bold ${d.labelPx ?? 10}px sans-serif`;
  const val = valence(d.elements);
  const spans = printSpans(d);
  const S = (p: DespieceNode) => d.worldToScreen(p.x, p.y);

  // The pieces: each member between its cuts, a faint link back to each joint it was cut from,
  // and the joints themselves.
  ctx.save();
  ctx.setLineDash([3, 4]); ctx.lineWidth = 1; ctx.strokeStyle = COL.link;
  for (const e of d.elements) {
    const sp = spans.get(e.id); if (!sp) continue;
    for (const [nid, end] of [[e.nodeI, sp.aI], [e.nodeJ, sp.aJ]] as const) {
      const n = d.getNode(nid); if (!n) continue;
      const a = S(n), b = S(end);
      if (Math.hypot(b.x - a.x, b.y - a.y) < 2) continue;
      ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
    }
  }
  ctx.restore();
  ctx.setLineDash([]);
  for (const e of d.elements) {
    const sp = spans.get(e.id); if (!sp) continue;
    const a = S(sp.aI), b = S(sp.aJ);
    ctx.strokeStyle = COL.member; ctx.lineWidth = 3; ctx.lineCap = 'round';
    ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
    labels.block({ x1: a.x, y1: a.y, x2: b.x, y2: b.y, pad: 4 });
  }
  ctx.lineCap = 'butt';
  for (const [nid, k] of val) {
    if (k < 2) continue;
    const n = d.getNode(nid); if (!n) continue;
    const s = S(n);
    ctx.fillStyle = COL.joint;
    ctx.beginPath(); ctx.arc(s.x, s.y, 4, 0, Math.PI * 2); ctx.fill();
  }

  // The forces, from the routine the screen uses; only where they stand is decided here.
  const vectors = computeDespieceVectors({
    elements: d.elements, getNode: d.getNode, getElementForces: d.getElementForces, reactions: d.reactions,
    sep: 1, vectorMode: 'all', basis: d.basis ?? 'local', showReactions: true, fmt: d.fmt,
  });
  const wantMember = d.mode !== 'nodes', wantNode = d.mode !== 'members';
  // Values go in one small block per cut (or per support), not one label per arrow: N, V and M
  // of an end read together, beside the piece, each in its arrow's colour.
  const ORDER: Record<string, number> = { N: 0, Fx: 0, Rx: 0, V: 1, Fz: 1, Rz: 1, F: 0, M: 2, My: 2 };
  const blocks = new Map<string, { lines: Array<{ text: string; colour: string; rank: number; mag: number }>; x: number; y: number; dirX: number; dirY: number; align: 'left' | 'right' | 'center' }>();
  const centre = (() => {
    let x = 0, y = 0, k = 0;
    for (const e of d.elements) for (const id of [e.nodeI, e.nodeJ]) { const n = d.getNode(id); if (n) { const s2 = S(n); x += s2.x; y += s2.y; k++; } }
    return k ? { x: x / k, y: y / k } : { x: 0, y: 0 };
  })();

  /** Where an end's block sits: beside the piece, in from the cut, on the side the shear comes from. */
  function endBlock(key: string, end: { x: number; y: number }, inward: { x: number; y: number }, side: { x: number; y: number }) {
    if (blocks.has(key)) return blocks.get(key)!;
    const x = end.x + inward.x * 22 + side.x * 14;
    const y = end.y + inward.y * 22 + side.y * 14 + 4;
    const align = Math.abs(side.x) > 0.5 ? (side.x > 0 ? 'left' : 'right') : (inward.x > 0 ? 'left' : 'right');
    const b = { lines: [], x, y, dirX: side.x, dirY: side.y, align } as typeof blocks extends Map<string, infer V> ? V : never;
    blocks.set(key, b);
    return b;
  }

  for (const v of vectors) {
    const nid = v.nodeId!;
    const node = d.getNode(nid);
    if (!node) continue;
    const sn = S(node);
    const mag = Math.abs(v.value);
    const q = v.glyph === 'moment' ? 'moment' : 'force';
    const line = { text: `${v.component} = ${d.fmt(mag, q)}`, colour: v.color, rank: ORDER[v.component ?? ''] ?? 3, mag };

    if (v.side === 'reaction') {
      if (v.glyph === 'moment') drawMomentSymbol(ctx, sn.x, sn.y, v.ccw ? 1 : -1, v.color, ARC_PX + 8);
      else {
        const ex = v.dirx ?? 0, ey = -(v.diry ?? 0);
        // An external action arrives at the support from outside.
        arrow(ctx, sn.x - ex * ARROW_PX * 1.2, sn.y - ey * ARROW_PX * 1.2, sn.x, sn.y, v.color);
        labels.block({ x1: sn.x - ex * ARROW_PX * 1.2, y1: sn.y - ey * ARROW_PX * 1.2, x2: sn.x, y2: sn.y, pad: 3 });
      }
      const out = { x: sn.x >= centre.x ? 1 : -1, y: 0 };
      endBlock(`r${nid}`, { x: sn.x, y: sn.y + 16 }, { x: 0, y: 1 }, out).lines.push(line);
      continue;
    }

    // A joint with one member is not cut: the reaction and the joint's loads stand for it.
    if ((val.get(nid) ?? 0) < 2) continue;
    if (v.side === 'member' && !wantMember) continue;
    if (v.side === 'node' && !wantNode) continue;
    const el = d.elements.find((e) => e.id === v.elementId)!;
    const sp = spans.get(el.id)!;
    const se = S(v.end === 'I' ? sp.aI : sp.aJ);
    const gx = se.x - sn.x, gy = se.y - sn.y, gl = Math.hypot(gx, gy) || 1;
    const inward = { x: gx / gl, y: gy / gl };            // joint → piece
    const anchor = v.side === 'member' ? se : { x: sn.x + inward.x * 16, y: sn.y + inward.y * 16 };
    const outX = v.side === 'member' ? -inward.x : inward.x, outY = v.side === 'member' ? -inward.y : inward.y;

    let shearFrom: { x: number; y: number } | null = null;
    if (v.glyph === 'moment') {
      drawMomentSymbol(ctx, anchor.x, anchor.y, v.ccw ? 1 : -1, v.color, ARC_PX);
      labels.block({ x1: anchor.x - ARC_PX, y1: anchor.y - ARC_PX, x2: anchor.x + ARC_PX, y2: anchor.y + ARC_PX, pad: 2 });
    } else {
      const ex = v.dirx ?? 0, ey = -(v.diry ?? 0);
      const l = Math.hypot(ex, ey) || 1, ux = ex / l, uy = ey / l;
      const away = ux * outX + uy * outY;
      let x0: number, y0: number, x1: number, y1: number;
      if (Math.abs(away) > 0.5 && away > 0) { x0 = anchor.x; y0 = anchor.y; x1 = anchor.x + ux * ARROW_PX; y1 = anchor.y + uy * ARROW_PX; }
      else { x1 = anchor.x; y1 = anchor.y; x0 = anchor.x - ux * ARROW_PX; y0 = anchor.y - uy * ARROW_PX; }
      arrow(ctx, x0, y0, x1, y1, v.color);
      labels.block({ x1: x0, y1: y0, x2: x1, y2: y1, pad: 3 });
      // Across the piece: the side the shear arrow stands on.
      if (Math.abs(away) <= 0.5) shearFrom = { x: -ux, y: -uy };
    }
    // The joint's own actions are the member's, opposite: labelled only when they are all there is.
    if (v.side === 'node' && wantMember) continue;
    const key = `${v.side}${v.elementId}${v.end}`;
    const perp = { x: -inward.y, y: inward.x };
    // The block goes on the side the shear arrow comes from, or failing that away from the middle.
    const towardOut = (perp.x * (anchor.x - centre.x) + perp.y * (anchor.y - centre.y)) >= 0 ? 1 : -1;
    const fallback = { x: perp.x * towardOut, y: perp.y * towardOut };
    const b = endBlock(key, anchor, inward, shearFrom ?? fallback);
    if (shearFrom) { b.dirX = shearFrom.x; b.dirY = shearFrom.y; b.x = anchor.x + inward.x * 22 + shearFrom.x * 14; b.y = anchor.y + inward.y * 22 + shearFrom.y * 14 + 4;
      b.align = Math.abs(shearFrom.x) > 0.5 ? (shearFrom.x > 0 ? 'left' : 'right') : (inward.x > 0 ? 'left' : 'right'); }
    b.lines.push(line);
  }

  for (const [key, b] of blocks) {
    const priority = Math.max(...b.lines.map((l) => l.mag));
    // A block is lines stacked down the page (the layout stacks along the escape direction), and
    // it may still slide sideways to find room. Beside a column it moves along the column.
    b.dirX = 0;
    b.dirY = b.dirY < -0.5 ? -1 : 1;
    for (const l of b.lines) {
      labels.add({ text: l.text, colour: l.colour, font, box: { x: b.x, y: b.y, width: 0, height: 0, dirX: b.dirX, dirY: b.dirY, priority, anchorX: b.align, sweep: 'any', group: key, rank: l.rank } });
    }
  }
}
