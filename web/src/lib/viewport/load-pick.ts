/**
 * Which load a click lands on.
 *
 * The 3D viewport draws every load into one batch, so there is no object per load to raycast.
 * The batch records the world segments each load drew (shafts, envelopes, outlines); this
 * measures the click against them on screen and takes the nearest, within a tolerance in pixels.
 * A click on the arrow is what a user aims at, so the arrow is what is measured, not the node or
 * the member the load sits on.
 */
export function pickLoadAt(
  px: number,
  py: number,
  footprints: ReadonlyMap<number, readonly number[]>,
  toScreen: (x: number, y: number, z: number) => { x: number; y: number },
  tolerance = 10,
  /**
   * Loads that act at a point (nodal, point on a member). At a joint the first arrow of a
   * distributed load lands on the same spot as a nodal load there; a click on that spot means
   * the load that acts at it, so within `TIE` pixels these win.
   */
  pointLoads: ReadonlySet<number> = new Set(),
): number | null {
  return pickLoadsAt(px, py, footprints, toScreen, tolerance, pointLoads)[0] ?? null;
}

/**
 * Every load within `tolerance` pixels, the one a click means first: nearest, with a point load
 * ahead of a distributed one within `TIE` pixels (see `pickLoadAt`). Two loads drawn on top of
 * each other (the same load added twice on a member) are both here, so a click again on the
 * same spot can reach the second (viewport/pick-cycle.ts).
 */
export function pickLoadsAt(
  px: number,
  py: number,
  footprints: ReadonlyMap<number, readonly number[]>,
  toScreen: (x: number, y: number, z: number) => { x: number; y: number },
  tolerance = 10,
  pointLoads: ReadonlySet<number> = new Set(),
): number[] {
  return pickLoadsWithDistance(px, py, footprints, toScreen, tolerance, pointLoads).map((h) => h.id);
}

/**
 * `pickLoadsAt` with each load's distance on screen, in pixels, for comparing with members.
 * `toScreen` may give a depth (`z`, smaller nearer, as `Vector3.project` does): two fills under
 * the pointer are then told apart by it.
 */
export function pickLoadsWithDistance(
  px: number,
  py: number,
  footprints: ReadonlyMap<number, readonly number[]>,
  toScreen: (x: number, y: number, z: number) => { x: number; y: number; z?: number },
  tolerance = 10,
  pointLoads: ReadonlySet<number> = new Set(),
  areas: ReadonlyMap<number, readonly (readonly number[])[]> = new Map(),
): { id: number; d: number }[] {
  const hits: { id: number; key: number; point: boolean; d: number; fill?: Fill }[] = [];
  /*
   * A load on a plate is also taken by a click on its fill: inside one of its faces on screen it
   * counts as at the tolerance, so an arrow or another load drawn under the pointer still wins.
   * Two fills under the pointer: the nearer the eye first (a slab over another in plan), then
   * the smaller (a load on a rectangle of a slab over the load on the whole slab); the lower id
   * only when both tie.
   */
  const inFace = (id: number): Fill | null => {
    let best: Fill | null = null;
    for (const f of areas.get(id) ?? []) {
      const pts: Array<{ x: number; y: number; z?: number }> = [];
      for (let i = 0; i + 2 < f.length; i += 3) pts.push(toScreen(f[i]!, f[i + 1]!, f[i + 2]!));
      const hit = fillAt(px, py, pts);
      if (hit && (!best || hit.depth < best.depth)) best = hit;
    }
    return best;
  };
  const seen = new Set<number>();
  for (const [id, f] of footprints) {
    let d = Infinity;
    for (let i = 0; i + 5 < f.length; i += 6) {
      const a = toScreen(f[i]!, f[i + 1]!, f[i + 2]!);
      const b = toScreen(f[i + 3]!, f[i + 4]!, f[i + 5]!);
      d = Math.min(d, distanceToSegment(px, py, a.x, a.y, b.x, b.y));
    }
    let fill: Fill | null = null;
    if (d > tolerance && (fill = inFace(id))) d = tolerance;
    if (d > tolerance) continue;
    const point = pointLoads.has(id);
    seen.add(id);
    hits.push({ id, key: d - (point ? TIE : 0), point, d, ...(fill ? { fill } : {}) });
  }
  // A load that drew a face and no segment near the pointer (a slab temperature, only a tag).
  for (const id of areas.keys()) {
    if (seen.has(id)) continue;
    const fill = inFace(id);
    if (fill) hits.push({ id, key: tolerance, point: false, d: tolerance, fill });
  }
  const byFill = (x?: Fill, y?: Fill) => (x && y ? (Math.abs(x.depth - y.depth) > DEPTH_TIE ? x.depth - y.depth : x.area - y.area) : 0);
  return hits
    .sort((x, y) => x.key - y.key || Number(y.point) - Number(x.point) || byFill(x.fill, y.fill) || x.id - y.id)
    .map((h) => ({ id: h.id, d: h.d }));
}

/** A fill under the pointer: its depth there and its area on screen. */
interface Fill { depth: number; area: number }
/** Depths closer than this are one surface (two loads on one slab). */
const DEPTH_TIE = 1e-6;

/**
 * The fill of a face at (px, py), or null outside it: the depth there interpolated over the
 * fan triangle under the pointer (0 when `toScreen` gives none), and the face's area on screen.
 */
function fillAt(px: number, py: number, pts: ReadonlyArray<{ x: number; y: number; z?: number }>): Fill | null {
  if (pts.length < 3 || !insidePolygon(px, py, pts)) return null;
  let area = 0;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) area += pts[j]!.x * pts[i]!.y - pts[i]!.x * pts[j]!.y;
  let depth = pts.reduce((s, p) => s + (p.z ?? 0), 0) / pts.length;
  const a = pts[0]!;
  for (let i = 1; i + 1 < pts.length; i++) {
    const b = pts[i]!, c = pts[i + 1]!;
    const det = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
    if (Math.abs(det) < 1e-12) continue;
    const u = ((px - a.x) * (c.y - a.y) - (c.x - a.x) * (py - a.y)) / det;
    const v = ((b.x - a.x) * (py - a.y) - (px - a.x) * (b.y - a.y)) / det;
    if (u >= -1e-9 && v >= -1e-9 && u + v <= 1 + 1e-9) { depth = (a.z ?? 0) + u * ((b.z ?? 0) - (a.z ?? 0)) + v * ((c.z ?? 0) - (a.z ?? 0)); break; }
  }
  return { depth, area: Math.abs(area) / 2 };
}

const TIE = 2;

/** Whether (px, py) is inside the polygon, by the even-odd rule. */
function insidePolygon(px: number, py: number, pts: ReadonlyArray<{ x: number; y: number }>): boolean {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const a = pts[i]!, b = pts[j]!;
    if ((a.y > py) !== (b.y > py) && px < ((b.x - a.x) * (py - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
}

function distanceToSegment(px: number, py: number, ax: number, ay: number, bx: number, by: number): number {
  const dx = bx - ax, dy = by - ay;
  const len2 = dx * dx + dy * dy;
  const t = len2 < 1e-12 ? 0 : Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / len2));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}
