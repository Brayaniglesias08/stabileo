/**
 * The build timelapse: the structure put up piece by piece, for showing it.
 *
 * Pure: it takes the members and shells as points in scene space (Z up) and returns when each
 * one starts and finishes appearing, and the transform that draws it part-way. The viewport
 * applies that transform to the meshes it already has, so nothing is rebuilt per frame.
 *
 * Orders:
 * - `construction`: as it would be built. A piece goes up once something already standing
 *   holds it (a support, or a member built before), lowest level first; within a level the
 *   columns, then the braces, then the beams and the slabs, swept along the longest side.
 * - `bottomUp`: by the height of the piece's midpoint, swept along the longest side.
 * - `along`: along the longest side, frame after frame, each frame from the bottom up.
 * - `list`: in the order of the members table.
 */

export type BuildOrder = 'construction' | 'bottomUp' | 'along' | 'list';

export interface P3 { x: number; y: number; z: number }

export type BuildPiece =
  | { key: string; kind: 'member'; a: P3; b: P3; nodes: [number, number] }
  | { key: string; kind: 'shell'; pts: P3[]; nodes: number[] };

export interface BuildStep {
  key: string;
  /** Seconds from the start of the build. */
  start: number;
  end: number;
  /** Where the piece grows from. */
  origin: P3;
  /** Member: the unit direction it grows along. Shell: its unit normal (it grows in its plane). */
  dir: P3;
  kind: 'member' | 'shell';
}

export interface BuildSchedule {
  steps: BuildStep[];
  /** Seconds until the last piece is in place. */
  total: number;
}

export interface ScheduleOptions {
  /** Seconds the whole build takes. */
  duration: number;
  /** Nodes the structure stands on: what holds the first pieces. */
  supportedNodes?: ReadonlySet<number>;
}

const sub = (a: P3, b: P3): P3 => ({ x: a.x - b.x, y: a.y - b.y, z: a.z - b.z });
const len = (v: P3) => Math.hypot(v.x, v.y, v.z);
const unit = (v: P3): P3 => { const l = len(v) || 1; return { x: v.x / l, y: v.y / l, z: v.z / l }; };
const cross = (a: P3, b: P3): P3 => ({ x: a.y * b.z - a.z * b.y, y: a.z * b.x - a.x * b.z, z: a.x * b.y - a.y * b.x });

function centroid(pts: readonly P3[]): P3 {
  const c = { x: 0, y: 0, z: 0 };
  for (const p of pts) { c.x += p.x; c.y += p.y; c.z += p.z; }
  const n = pts.length || 1;
  return { x: c.x / n, y: c.y / n, z: c.z / n };
}

function piecePoints(p: BuildPiece): P3[] { return p.kind === 'member' ? [p.a, p.b] : p.pts; }

/** Column, brace or beam, by how steep the member is; a shell is a slab. */
function rankOf(p: BuildPiece): number {
  if (p.kind === 'shell') return 3;
  const d = sub(p.b, p.a);
  const l = len(d);
  if (l < 1e-9) return 2;
  const steep = Math.abs(d.z) / l;
  if (steep > 0.95) return 0; // column
  if (steep < 0.05) return 2; // beam
  return 1; // brace, rafter
}

interface Meta { piece: BuildPiece; index: number; minZ: number; maxZ: number; mid: P3 }

/**
 * The order the pieces go up in, as indexes into `pieces`.
 */
export function buildOrder(pieces: readonly BuildPiece[], order: BuildOrder, supported: ReadonlySet<number> = new Set()): number[] {
  if (pieces.length === 0) return [];
  if (order === 'list') return pieces.map((_, i) => i);

  const metas: Meta[] = pieces.map((piece, index) => {
    const pts = piecePoints(piece);
    let minZ = Infinity, maxZ = -Infinity;
    for (const q of pts) { if (q.z < minZ) minZ = q.z; if (q.z > maxZ) maxZ = q.z; }
    return { piece, index, minZ, maxZ, mid: centroid(pts) };
  });

  // The sweep runs along the longest horizontal side, and across it second.
  let x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity, z0 = Infinity, z1 = -Infinity;
  for (const m of metas) {
    x0 = Math.min(x0, m.mid.x); x1 = Math.max(x1, m.mid.x);
    y0 = Math.min(y0, m.mid.y); y1 = Math.max(y1, m.mid.y);
    z0 = Math.min(z0, m.minZ); z1 = Math.max(z1, m.maxZ);
  }
  const alongX = x1 - x0 >= y1 - y0;
  const along = (m: Meta) => (alongX ? m.mid.x : m.mid.y);
  const across = (m: Meta) => (alongX ? m.mid.y : m.mid.x);
  const span = Math.max(x1 - x0, y1 - y0, z1 - z0, 1e-6);
  // Heights and positions within this of each other count as the same.
  const tol = Math.max(0.05, span * 0.01);
  const band = (v: number) => Math.round(v / tol);
  const cmp = (a: number, b: number) => (a < b ? -1 : a > b ? 1 : 0);

  if (order === 'bottomUp') {
    return [...metas]
      .sort((a, b) => cmp(band(a.mid.z), band(b.mid.z)) || cmp(band(along(a)), band(along(b))) || cmp(across(a), across(b)) || a.index - b.index)
      .map((m) => m.index);
  }
  if (order === 'along') {
    return [...metas]
      .sort((a, b) => cmp(band(along(a)), band(along(b))) || cmp(band(a.mid.z), band(b.mid.z)) || cmp(across(a), across(b)) || a.index - b.index)
      .map((m) => m.index);
  }

  // construction: a level is the height a piece reaches; a piece waits for what holds it.
  const key = (m: Meta) => [band(m.maxZ), rankOf(m.piece), band(along(m)), across(m), m.index];
  const byKey = (a: Meta, b: Meta) => {
    const ka = key(a), kb = key(b);
    for (let i = 0; i < ka.length; i++) { const c = cmp(ka[i]!, kb[i]!); if (c) return c; }
    return 0;
  };
  const pending = [...metas].sort(byKey);
  // Without supports, the lowest nodes are the ground.
  const standing = new Set<number>(supported);
  if (standing.size === 0) {
    for (const m of metas) {
      const pts = piecePoints(m.piece);
      m.piece.nodes.forEach((id, i) => { if (band(pts[i]!.z) === band(z0)) standing.add(id); });
    }
  }
  const out: number[] = [];
  while (pending.length > 0) {
    let at = pending.findIndex((m) => m.piece.nodes.some((id) => standing.has(id)));
    // Nothing touches what stands (a part floating free): the next by level goes up anyway.
    if (at < 0) at = 0;
    const [m] = pending.splice(at, 1);
    out.push(m!.index);
    for (const id of m!.piece.nodes) standing.add(id);
  }
  return out;
}

/**
 * When each piece appears. Pieces start one after the other at an even pace, and each takes a
 * moment to grow, so several are growing at once on a large model.
 */
export function buildSchedule(pieces: readonly BuildPiece[], order: BuildOrder, opts: ScheduleOptions): BuildSchedule {
  const seq = buildOrder(pieces, order, opts.supportedNodes);
  const n = seq.length;
  if (n === 0) return { steps: [], total: 0 };
  const duration = Math.max(1, opts.duration);
  const grow = Math.min(0.9, Math.max(0.18, (duration / n) * 6), duration * 0.5);
  const spacing = n > 1 ? (duration - grow) / (n - 1) : 0;
  const steps: BuildStep[] = seq.map((i, k) => {
    const p = pieces[i]!;
    const start = k * spacing;
    if (p.kind === 'member') {
      // From the lower end, so a column rises and a beam runs from where it starts.
      const fromA = p.a.z <= p.b.z + 1e-9;
      const o = fromA ? p.a : p.b, e = fromA ? p.b : p.a;
      return { key: p.key, start, end: start + grow, origin: o, dir: unit(sub(e, o)), kind: 'member' };
    }
    const c = centroid(p.pts);
    const nrm = p.pts.length >= 3 ? unit(cross(sub(p.pts[1]!, p.pts[0]!), sub(p.pts[2]!, p.pts[0]!))) : { x: 0, y: 0, z: 1 };
    return { key: p.key, start, end: start + grow, origin: c, dir: nrm, kind: 'shell' };
  });
  return { steps, total: duration };
}

/** How far a piece has grown at time `t`, 0 to 1, eased so it settles into place. */
export function growthAt(step: BuildStep, t: number): number {
  if (t <= step.start) return 0;
  if (t >= step.end) return 1;
  const u = (t - step.start) / (step.end - step.start);
  return 1 - Math.pow(1 - u, 3);
}

/**
 * The transform (column-major 4×4, as THREE.Matrix4.elements) that draws a piece grown to `s`.
 *
 * A member is stretched along its own axis from its origin: L = I + (s − 1)·d·dᵀ, so its section
 * keeps its size and only its length grows. A shell grows in its own plane from its centroid:
 * L = n·nᵀ + s·(I − n·nᵀ), keeping its thickness.
 */
export function growMatrix(step: BuildStep, s: number, out: number[] = new Array(16)): number[] {
  const { x: dx, y: dy, z: dz } = step.dir;
  const d = [dx, dy, dz];
  const L: number[][] = [[0, 0, 0], [0, 0, 0], [0, 0, 0]];
  for (let r = 0; r < 3; r++) {
    for (let c = 0; c < 3; c++) {
      const ddt = d[r]! * d[c]!;
      const id = r === c ? 1 : 0;
      L[r]![c] = step.kind === 'member' ? id + (s - 1) * ddt : ddt + s * (id - ddt);
    }
  }
  const o = [step.origin.x, step.origin.y, step.origin.z];
  // T(o)·L·T(−o): the translation is o − L·o.
  const t = [0, 1, 2].map((r) => o[r]! - (L[r]![0]! * o[0]! + L[r]![1]! * o[1]! + L[r]![2]! * o[2]!));
  for (let c = 0; c < 3; c++) for (let r = 0; r < 3; r++) out[c * 4 + r] = L[r]![c]!;
  out[3] = 0; out[7] = 0; out[11] = 0;
  out[12] = t[0]!; out[13] = t[1]!; out[14] = t[2]!; out[15] = 1;
  return out;
}
