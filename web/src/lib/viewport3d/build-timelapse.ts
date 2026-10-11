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
 * - `byType`: every column, then every brace, every beam and every slab, each from the bottom up.
 * - `radial`: out from the middle of the footprint at ground level, like a dome swelling.
 * - `list`: in the order of the members table.
 * - `stages`, `groups`: phase by phase, as the caller names them (PRO's construction stages, the
 *   model's groups); within a phase as it would be built. A stage can also take pieces away.
 */

export type BuildOrder = 'construction' | 'bottomUp' | 'along' | 'byType' | 'radial' | 'list' | 'stages' | 'groups';

/** A named step of a phased order: the pieces it puts up and those it takes down, by key. */
export interface BuildPhase {
  name: string;
  add: readonly string[];
  remove?: readonly string[];
}

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
  /** Height of the piece's highest point. */
  top: number;
  /** When a stage takes the piece away again: it shrinks out over these seconds. */
  outStart?: number;
  outEnd?: number;
}

export interface BuildSchedule {
  steps: BuildStep[];
  /** Seconds until the last piece is in place. */
  total: number;
  /** Lowest and highest points of the whole structure. */
  zMin: number;
  zMax: number;
  /** A phased order: when each phase begins, for its caption. Empty otherwise. */
  phases: Array<{ name: string; start: number }>;
  /** Some pieces are in no phase, so they never go up and stay hidden. */
  partial: boolean;
}

export interface ScheduleOptions {
  /** Seconds the whole build takes. */
  duration: number;
  /** Nodes the structure stands on: what holds the first pieces. */
  supportedNodes?: ReadonlySet<number>;
  /** The phases of `stages` and `groups`. Without them those orders build as `construction`. */
  phases?: readonly BuildPhase[];
}

/** How far into its growth a piece counts as landed, for its sound. */
const LAND_U = 0.55;

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

const cmp = (a: number, b: number) => (a < b ? -1 : a > b ? 1 : 0);
function byKeys<T>(key: (m: T) => number[]) {
  return (a: T, b: T) => {
    const ka = key(a), kb = key(b);
    for (let i = 0; i < ka.length; i++) { const c = cmp(ka[i]!, kb[i]!); if (c) return c; }
    return 0;
  };
}

/** What the orders measure the pieces by: their extent, the sweep direction and the tolerance. */
function survey(pieces: readonly BuildPiece[]) {
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
  const span = Math.max(x1 - x0, y1 - y0, z1 - z0, 1e-6);
  // Heights and positions within this of each other count as the same.
  const tol = Math.max(0.05, span * 0.01);
  return {
    metas, z0,
    centre: { x: (x0 + x1) / 2, y: (y0 + y1) / 2, z: z0 },
    along: (m: Meta) => (alongX ? m.mid.x : m.mid.y),
    across: (m: Meta) => (alongX ? m.mid.y : m.mid.x),
    band: (v: number) => Math.round(v / tol),
  };
}
type Survey = ReturnType<typeof survey>;

/** Nodes that stand from the start: the supports, or without any the lowest nodes. */
function groundOf(sv: Survey, supported: ReadonlySet<number>): Set<number> {
  const standing = new Set<number>(supported);
  if (standing.size === 0) {
    for (const m of sv.metas) {
      const pts = piecePoints(m.piece);
      m.piece.nodes.forEach((id, i) => { if (sv.band(pts[i]!.z) === sv.band(sv.z0)) standing.add(id); });
    }
  }
  return standing;
}

/**
 * As it would be built: a level is the height a piece reaches, and a piece waits for something
 * standing to hold it. `standing` grows with what goes up, so phases can carry it on.
 */
function constructionSeq(sv: Survey, metas: readonly Meta[], standing: Set<number>): number[] {
  const { band, along, across } = sv;
  const pending = [...metas].sort(byKeys((m: Meta) => [band(m.maxZ), rankOf(m.piece), band(along(m)), across(m), m.index]));
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
 * The order the pieces go up in, as indexes into `pieces`. The phased orders are not known here
 * (they need their phases) and build as `construction`.
 */
export function buildOrder(pieces: readonly BuildPiece[], order: BuildOrder, supported: ReadonlySet<number> = new Set()): number[] {
  if (pieces.length === 0) return [];
  if (order === 'list') return pieces.map((_, i) => i);
  const sv = survey(pieces);
  const { metas, band, along, across } = sv;
  const sorted = (key: (m: Meta) => number[]) => [...metas].sort(byKeys(key)).map((m) => m.index);

  if (order === 'bottomUp') return sorted((m) => [band(m.mid.z), band(along(m)), across(m), m.index]);
  if (order === 'along') return sorted((m) => [band(along(m)), band(m.mid.z), across(m), m.index]);
  if (order === 'byType') return sorted((m) => [rankOf(m.piece), band(m.mid.z), band(along(m)), across(m), m.index]);
  if (order === 'radial') {
    const c = sv.centre;
    return sorted((m) => [band(Math.hypot(m.mid.x - c.x, m.mid.y - c.y, m.mid.z - c.z)), band(m.mid.z), m.index]);
  }
  return constructionSeq(sv, metas, groundOf(sv, supported));
}

/**
 * When each piece appears. Pieces start one after the other at an even pace, and each takes a
 * moment to grow, so several are growing at once on a large model.
 */
export function buildSchedule(pieces: readonly BuildPiece[], order: BuildOrder, opts: ScheduleOptions): BuildSchedule {
  const phased = (order === 'stages' || order === 'groups') && opts.phases && opts.phases.length > 0;
  // Each phase: the pieces it puts up, in order, and the ones it takes down.
  const plan: Array<{ name: string; add: number[]; remove: number[] }> = [];
  if (phased) {
    const sv = survey(pieces);
    const indexOf = new Map(pieces.map((p, i) => [p.key, i]));
    const standing = groundOf(sv, opts.supportedNodes ?? new Set());
    const placed = new Set<number>();
    const gone = new Set<number>();
    for (const ph of opts.phases!) {
      const adds: Meta[] = [];
      for (const k of ph.add) {
        const i = indexOf.get(k);
        if (i === undefined || placed.has(i)) continue;
        placed.add(i);
        adds.push(sv.metas[i]!);
      }
      const remove: number[] = [];
      for (const k of ph.remove ?? []) {
        const i = indexOf.get(k);
        // Only what is standing can be taken down, and only once.
        if (i === undefined || !placed.has(i) || gone.has(i) || adds.some((m) => m.index === i)) continue;
        gone.add(i);
        remove.push(i);
      }
      if (adds.length || remove.length) plan.push({ name: ph.name, add: constructionSeq(sv, adds, standing), remove });
    }
  } else {
    plan.push({ name: '', add: buildOrder(pieces, order, opts.supportedNodes), remove: [] });
  }

  const n = plan.reduce((k, ph) => k + ph.add.length, 0);
  if (n === 0) return { steps: [], total: 0, zMin: 0, zMax: 0, phases: [], partial: pieces.length > 0 };
  let zMin = Infinity, zMax = -Infinity;
  for (const p of pieces) for (const q of piecePoints(p)) { zMin = Math.min(zMin, q.z); zMax = Math.max(zMax, q.z); }
  const duration = Math.max(1, opts.duration);
  // A take-down is one beat, however many pieces it takes.
  const slots = n + plan.filter((ph) => ph.remove.length > 0).length;
  const grow = Math.min(0.9, Math.max(0.18, (duration / slots) * 6), duration * 0.5);
  // A short breath between phases, so each reads as one.
  const gaps = plan.length - 1;
  const gap = gaps > 0 ? Math.min(0.6, (duration * 0.25) / gaps) : 0;
  const spacing = slots > 1 ? Math.max(0, duration - grow - gap * gaps) / (slots - 1) : 0;

  const steps: BuildStep[] = [];
  const stepAt = new Map<number, BuildStep>();
  const phases: BuildSchedule['phases'] = [];
  let t = 0;
  plan.forEach((ph, k) => {
    if (k > 0) t += gap;
    phases.push({ name: ph.name, start: t });
    for (const i of ph.add) {
      const step = stepFor(pieces[i]!, t, t + grow);
      steps.push(step);
      stepAt.set(i, step);
      t += spacing;
    }
    if (ph.remove.length) {
      for (const i of ph.remove) {
        const step = stepAt.get(i)!;
        step.outStart = t; step.outEnd = t + grow;
      }
      t += spacing;
    }
  });
  return {
    steps, total: duration, zMin, zMax,
    phases: phased ? phases : [],
    partial: phased ? stepAt.size < pieces.length : false,
  };
}

function stepFor(p: BuildPiece, start: number, end: number): BuildStep {
  if (p.kind === 'member') {
    // From the lower end, so a column rises and a beam runs from where it starts.
    const fromA = p.a.z <= p.b.z + 1e-9;
    const o = fromA ? p.a : p.b, e = fromA ? p.b : p.a;
    return { key: p.key, start, end, origin: o, dir: unit(sub(e, o)), kind: 'member', top: Math.max(p.a.z, p.b.z) };
  }
  const c = centroid(p.pts);
  const nrm = p.pts.length >= 3 ? unit(cross(sub(p.pts[1]!, p.pts[0]!), sub(p.pts[2]!, p.pts[0]!))) : { x: 0, y: 0, z: 1 };
  return { key: p.key, start, end, origin: c, dir: nrm, kind: 'shell', top: Math.max(...p.pts.map((q) => q.z)) };
}

/** How far a piece has grown at time `t`, 0 to 1, eased so it settles into place. */
export function growthAt(step: BuildStep, t: number): number {
  if (step.outStart !== undefined && t > step.outStart) {
    // Taken down: it eases out the way it came in.
    const u = Math.min(1, (t - step.outStart) / Math.max(1e-6, step.outEnd! - step.outStart));
    return 1 - u * u * u;
  }
  if (t <= step.start) return 0;
  if (t >= step.end) return 1;
  const u = (t - step.start) / (step.end - step.start);
  return 1 - Math.pow(1 - u, 3);
}

/** The moment a piece counts as landed: well into its growth, where it visibly settles. */
export function landAt(step: BuildStep): number {
  return step.start + LAND_U * (step.end - step.start);
}

/** How many pieces land in (t0, t1]: the hits a frame plays. */
export function landedBetween(schedule: BuildSchedule, t0: number, t1: number): number {
  if (t1 <= t0) return 0;
  let n = 0;
  for (const s of schedule.steps) { const l = landAt(s); if (l > t0 && l <= t1) n++; }
  return n;
}

/** The phase under way at `t`: the last one begun. Null outside a phased order. */
export function phaseAt(schedule: BuildSchedule, t: number): string | null {
  let name: string | null = null;
  for (const ph of schedule.phases) if (ph.start <= t) name = ph.name;
  return name;
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

/**
 * The transform that draws a piece being lowered into place: its final shape, `dist` above
 * where it goes at s = 0, settling onto it at s = 1.
 */
export function dropMatrix(dist: number, s: number, out: number[] = new Array(16)): number[] {
  out.fill(0);
  out[0] = 1; out[5] = 1; out[10] = 1; out[15] = 1;
  out[14] = dist * (1 - s);
  return out;
}

/**
 * How high the build has got at time `t`: the top of the highest piece that has started. The
 * rising camera follows it; before anything starts it is the foot of the structure.
 */
export function buildFront(schedule: BuildSchedule, t: number): number {
  let front = schedule.zMin;
  for (const s of schedule.steps) if (s.start <= t && s.top > front) front = s.top;
  return front;
}

/** How many pieces have started appearing by time `t`. */
export function piecesStarted(schedule: BuildSchedule, t: number): number {
  let n = 0;
  for (const s of schedule.steps) if (s.start < t) n++;
  return n;
}
