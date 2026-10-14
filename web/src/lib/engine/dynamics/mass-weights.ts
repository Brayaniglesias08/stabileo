/**
 * Weights that are mass and no load case (`MassSource.weights`): written as the loads they stand
 * for, in a case of their own that the mass source takes with factor 1, so they become density the
 * same way a case's loads do (`mass-source.ts`).
 *
 *   members   kN/m along each member in the region, downward;
 *   slabs     kN/m² on each slab in the region, downward: a shell sloping at most 30°, the
 *             rule a member carrying an area load keeps (`plan-gravity.ts` `beamLike`), so a
 *             sloped roof is a slab and a wall is not (it took the weight too: a 5 × 4 m slab
 *             and a 5 × 3 m wall at 2 kN/m² gave 7,14 t for the slab's 4,08);
 *   floor     kN/m² on the region's floor, carried to its beams by tributary area
 *             (`floor-definitions.ts`): a zone, a group or a box of coordinates; on the whole
 *             model, every level its horizontal members make (it was dropped without a word).
 *
 * Pure.
 */
import type { Load } from '../../store/model.svelte';
import type { MassWeight } from './mass-source';
import { regionMembers, regionNodes } from '../../model/loads/region-nodes';
import { expandDefinition, type DefinitionModel, type FloorTarget } from '../../model/loads/floor-definitions';

/** A slab slopes at most 30° (|n_z| ≥ cos 30°), as `beamLike` reads a member (|Δz|/L ≤ 0,5). */
const SLAB_NZ = Math.cos(Math.PI / 6);

/** Whether a shell's plane (Newell's normal of its corners) is a slab's, not a wall's. */
function slabLike(m: DefinitionModel, ids: readonly number[]): boolean {
  const p = ids.map((id) => m.nodes.get(id));
  if (p.some((n) => !n)) return false;
  let nx = 0, ny = 0, nz = 0;
  for (let i = 0; i < p.length; i++) {
    const a = p[i]!, b = p[(i + 1) % p.length]!;
    const az = a.z ?? 0, bz = b.z ?? 0;
    nx += (a.y - b.y) * (az + bz); ny += (az - bz) * (a.x + b.x); nz += (a.x - b.x) * (a.y + b.y);
  }
  const n = Math.hypot(nx, ny, nz);
  return n > 1e-12 && Math.abs(nz) / n >= SLAB_NZ - 1e-9;
}

/** The levels of the model's floors: the heights its horizontal members lie at. */
function floorLevels(m: DefinitionModel): number[] {
  const out = new Set<number>();
  for (const e of m.elements.values()) {
    const a = m.nodes.get(e.nodeI), b = m.nodes.get(e.nodeJ);
    if (a && b && Math.abs((a.z ?? 0) - (b.z ?? 0)) <= 1e-3) out.add(Math.round((a.z ?? 0) * 1000) / 1000);
  }
  return [...out].sort((x, y) => x - y);
}

/** The case id the weights are written in: no model case has it. */
export const WEIGHT_CASE = -1001;

export function massWeightLoads(m: DefinitionModel, weights: readonly MassWeight[], leftHand: boolean): Load[] {
  const out: Load[] = [];
  let id = 1;
  for (const w of weights) {
    if (!(w.w !== 0)) continue;
    if (w.on === 'members') {
      const ids = regionMembers(m, w.region) ?? [...m.elements.keys()];
      for (const e of ids) out.push({ type: 'distributed3d', data: { id: id++, elementId: e, qYI: 0, qYJ: 0, qZI: -w.w, qZJ: -w.w, frame: 'global', caseId: WEIGHT_CASE } });
    } else if (w.on === 'slabs') {
      const nodes = regionNodes(m, w.region);
      const set = nodes ? new Set(nodes) : null;
      const inside = (ns: number[]) => !set || ns.every((n) => set.has(n));
      for (const q of m.quads.values()) if (inside(q.nodes) && slabLike(m, q.nodes)) out.push({ type: 'surface3d', data: { id: id++, quadId: q.id, q: w.w, caseId: WEIGHT_CASE } });
      for (const p of m.plates.values()) if (inside(p.nodes) && slabLike(m, p.nodes)) out.push({ type: 'surface3d', data: { id: id++, quadId: p.id, on: 'plate', q: w.w, caseId: WEIGHT_CASE } });
    } else {
      const r = w.region;
      const targets: FloorTarget[] = r.kind === 'zone' ? [{ by: 'zone', zoneId: r.zoneId }] : r.kind === 'group' ? [{ by: 'group', groupId: r.groupId }]
        : r.kind === 'box' ? [{ by: 'range', ...(r.x ? { x: r.x } : {}), ...(r.y ? { y: r.y } : {}), ...(r.z ? { z: r.z } : {}) }]
        : floorLevels(m).map((z) => ({ by: 'level' as const, z }));
      for (const target of targets) {
        const e = expandDefinition({ ...m, loadCases: [{ id: WEIGHT_CASE }] }, { caseId: WEIGHT_CASE, q: w.w, target, distribution: 'twoWay' }, {}, -1, { leftHand });
        for (const l of e.loads) out.push({ ...l, data: { ...l.data, id: id++ } } as Load);
      }
    }
  }
  return out;
}
