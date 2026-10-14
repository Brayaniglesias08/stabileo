/**
 * The 2D despiece's actions, for `drawFreeBody`: what `computeDespieceVectors` says acts at every
 * cut and support (tested for equilibrium there), grouped by the piece it acts on.
 */
import { computeDespieceVectors, type DespieceBasis, type DespieceElement, type DespieceElementForces, type DespieceNode, type DespieceReaction } from './draw-despiece';
import type { FBAction, FBEnd, FBSupport } from './free-body';

export function freeBodyActions2D(args: {
  elements: DespieceElement[];
  getNode: (id: number) => DespieceNode | undefined;
  getElementForces: (id: number) => DespieceElementForces | undefined;
  reactions: Map<number, DespieceReaction>;
  basis: DespieceBasis;
  resultant: boolean;
  showReactions: boolean;
  /** Components below this share of the largest force (or moment) are left out; 0 keeps all. */
  negligible?: number;
}): { ends: FBEnd[]; supports: FBSupport[] } {
  const vectors = computeDespieceVectors({
    ...args, sep: 1, vectorMode: 'all', fmt: () => '',
  });
  let maxF = 0, maxM = 0;
  for (const v of vectors) {
    if (v.glyph === 'moment') maxM = Math.max(maxM, Math.abs(v.value)); else maxF = Math.max(maxF, Math.abs(v.value));
  }
  const share = args.negligible ?? 0;
  const ends = new Map<string, FBEnd>();
  const supports = new Map<number, FBSupport>();
  for (const v of vectors) {
    if (Math.abs(v.value) < share * (v.glyph === 'moment' ? maxM : maxF)) continue;
    const a: FBAction = {
      name: v.component ?? '', kind: v.glyph, magnitude: Math.abs(v.value), color: v.color,
      dir: { x: v.dirx ?? 0, y: v.diry ?? 0 }, ccw: v.ccw,
    };
    if (v.side === 'reaction') {
      const s = supports.get(v.nodeId!) ?? { nodeId: v.nodeId!, actions: [] };
      s.actions.push(a);
      supports.set(v.nodeId!, s);
      continue;
    }
    const key = `${v.side}|${v.elementId}|${v.end}`;
    const e = ends.get(key) ?? { memberId: v.elementId!, end: v.end!, nodeId: v.nodeId!, side: v.side, actions: [] };
    e.actions.push(a);
    ends.set(key, e);
  }
  return { ends: [...ends.values()], supports: [...supports.values()] };
}
