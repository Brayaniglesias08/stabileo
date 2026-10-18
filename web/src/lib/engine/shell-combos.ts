import { getShellCombinationKernel, type ShellRecord } from './shell-combination-kernel';
import { EngineUnavailableError } from './engine-unavailable';
// Combine per-case shell stresses into per-combination + envelope results.
//
// The WASM combination solver linearly combines displacements / reactions /
// element forces, but drops plate/quad stresses (combine_results_3d sets them
// to empty). Shell membrane stresses (σxx, σyy, τxy), bending moments
// (mx, my, mxy) and the quads' transverse shears (qx, qy) are LINEAR in the
// displacement field, so for a linear-elastic combo they equal
// Σ factorᵢ · (case-i value). The engine's shell combination kernel
// (`engine/src/shell_combination_kernel.rs`) recombines them from the per-case
// results, and recomputes σ1, σ2 and Von Mises as the engine defines them for
// each element: on the worse face (membrane ± 6M/t²) for a DKT triangle, from
// the membrane for a quad. Nodal Von Mises needs nodal components we don't
// carry, so combos fall back to the centroidal value.
//
// The kernel is the only path. It used to be skipped for a TypeScript copy of
// the same arithmetic whenever it was not registered, without a word; now a
// missing kernel throws `EngineUnavailableError`. The TypeScript copy is the
// tests' oracle (`__tests__/differential/shell-combos-oracle.ts`).

import type { AnalysisResults3D, PlateStress, QuadStress } from './types-3d';

export interface ComboFactor { caseId: number; factor: number; }

type Membrane = { sigmaXx: number; sigmaYy: number; tauXy: number; mx: number; my: number; mxy: number; qx?: number; qy?: number };

const toMembraneMap = (list: Array<{ elementId: number } & Membrane> | undefined): Map<number, Membrane> =>
  new Map((list ?? []).map(s => [s.elementId, { sigmaXx: s.sigmaXx, sigmaYy: s.sigmaYy, tauXy: s.tauXy, mx: s.mx, my: s.my, mxy: s.mxy, qx: s.qx, qy: s.qy }]));

/** The plates' thicknesses, which their face stresses need. */
export type PlateThickness = ReadonlyMap<number, { thickness: number }>;

function kernelOrThrow() {
  const kernel = getShellCombinationKernel();
  if (!kernel) throw new EngineUnavailableError('the shell stresses of the combinations');
  return kernel;
}

const rows = <T>(m: Map<number, Membrane> | undefined): T[] =>
  [...(m ?? new Map<number, Membrane>())].map(([elementId, v]) => ({ ...v, elementId }) as unknown as T);

/**
 * Recombine plate + quad stresses for one combo from per-case results, by the engine's kernel.
 *
 * `ids`, when given, is the union of the shells over every case (in case order), and the output
 * follows it; otherwise it follows the order the combination's own cases list them.
 */
export function combineShellStresses(
  factors: ComboFactor[],
  perCasePlates: Map<number, Map<number, Membrane>>,
  perCaseQuads: Map<number, Map<number, Membrane>>,
  plates: PlateThickness,
  ids?: { plates: Set<number>; quads: Set<number> },
): { plateStresses: PlateStress[]; quadStresses: QuadStress[] } {
  // No shells, nothing to combine: a frame needs no kernel.
  const any = (m: Map<number, Map<number, Membrane>>) => [...m.values()].some((c) => c.size > 0);
  if (!any(perCasePlates) && !any(perCaseQuads)) return { plateStresses: [], quadStresses: [] };
  const kernel = kernelOrThrow();
  const caseIds = ids
    ? [...new Set([...perCasePlates.keys(), ...perCaseQuads.keys()])]
    : [...new Set(factors.map((f) => f.caseId))];
  const cases: ShellRecord[] = caseIds.map((id) => ({
    id,
    plateStresses: rows<PlateStress>(perCasePlates.get(id)),
    quadStresses: rows<QuadStress>(perCaseQuads.get(id)),
  }));
  const result = kernel({
    cases,
    combinations: [{ id: 0, factors }],
    thicknesses: [...plates].map(([id, p]) => [id, p.thickness]),
    envelopeOrder: [],
  });
  const combo = result.combinations[0];
  return { plateStresses: combo?.plateStresses ?? [], quadStresses: combo?.quadStresses ?? [] };
}

/** Per-element governing (max Von Mises across combos) shell stresses for the
 *  envelope result, so the envelope view also contours shells. */
export function envelopeShellStresses(
  combos: AnalysisResults3D[],
): { plateStresses: PlateStress[]; quadStresses: QuadStress[] } {
  const govP = new Map<number, PlateStress>();
  const govQ = new Map<number, QuadStress>();
  for (const r of combos) {
    for (const s of r.plateStresses ?? []) { const g = govP.get(s.elementId); if (!g || s.vonMises > g.vonMises) govP.set(s.elementId, s); }
    for (const s of r.quadStresses ?? []) { const g = govQ.get(s.elementId); if (!g || s.vonMises > g.vonMises) govQ.set(s.elementId, s); }
  }
  return { plateStresses: [...govP.values()], quadStresses: [...govQ.values()] };
}

/**
 * Mutate a combination bundle in place so per-combo and envelope results carry
 * shell stresses (recombined from per-case). No-op without shells.
 */
export function enrichComboShellStresses(
  perCase: Map<number, AnalysisResults3D>,
  perCombo: Map<number, AnalysisResults3D>,
  envelopeMaxAbs: AnalysisResults3D | undefined,
  combinations: Array<{ id: number; factors: Array<{ caseId: number; factor: number }> }>,
  plates: PlateThickness,
): void {
  if (![...perCase.values()].some(r => r.plateStresses?.length || r.quadStresses?.length)) return;
  const kernel = kernelOrThrow();
  const active = combinations.filter(c => perCombo.has(c.id));
  const replaced = new Set(active.map(c => c.id));
  const result = kernel({
    cases: [...perCase].map(([id, r]) => ({ id, plateStresses: r.plateStresses ?? [], quadStresses: r.quadStresses ?? [] })),
    combinations: active,
    thicknesses: [...plates].map(([id, p]) => [id, p.thickness]),
    envelopeOrder: envelopeMaxAbs ? [...perCombo].map(([id, r]) => ({ id,
      plateStresses: replaced.has(id) ? [] : r.plateStresses ?? [],
      quadStresses: replaced.has(id) ? [] : r.quadStresses ?? [],
    })) : [],
  });
  for (const r of result.combinations) {
    const destination = perCombo.get(r.id)!;
    destination.plateStresses = r.plateStresses; destination.quadStresses = r.quadStresses;
  }
  if (envelopeMaxAbs) {
    envelopeMaxAbs.plateStresses = result.envelope.plateStresses;
    envelopeMaxAbs.quadStresses = result.envelope.quadStresses;
  }
}

/**
 * Every load case at factor 1, as a full result — the "All loads" baseline. Or at the `factors`
 * given: the single solve's (`case-effects.ts` `singleSolveFactors`), where a composite case's
 * parts are in it already.
 *
 * `combined` is the engine's combination of the cases (displacements, reactions, member
 * forces). The engine leaves out everything else, and the baseline is read for more than the
 * frame: the floor design takes the slab moments from its quad stresses, the solve toasts its
 * solver diagnostics, and the diagnostics panel its structured findings. So, from the cases:
 *   · shell stresses and constraint forces are summed — both linear in the displacements;
 *   · the diagnostics, which describe the model and the solve rather than one case's loads,
 *     and the timings are the first case's.
 * Nodal shell stresses are not carried, as for every combination (see the header).
 */
export function allLoadsResult3D(
  combined: AnalysisResults3D,
  perCase: Map<number, AnalysisResults3D>,
  plateThickness: PlateThickness,
  factors: Array<{ caseId: number; factor: number }> = [...perCase.keys()].map((caseId) => ({ caseId, factor: 1 })),
): AnalysisResults3D {
  const cases = [...perCase.values()];
  const first = cases[0];
  const factorOf = new Map(factors.map((f) => [f.caseId, f.factor]));
  const plates = new Map([...perCase].map(([id, r]) => [id, toMembraneMap(r.plateStresses)]));
  const quads = new Map([...perCase].map(([id, r]) => [id, toMembraneMap(r.quadStresses)]));
  const shells = combineShellStresses(factors, plates, quads, plateThickness);
  const constraint = new Map<string, { nodeId: number; dof: string; force: number }>();
  for (const [id, r] of perCase) {
    const k = factorOf.get(id) ?? 0;
    if (k === 0) continue;
    for (const c of r.constraintForces ?? []) {
      const key = `${c.nodeId}:${c.dof}`;
      const acc = constraint.get(key);
      if (acc) acc.force += k * c.force; else constraint.set(key, { ...c, force: k * c.force });
    }
  }
  return {
    ...combined,
    ...(shells.plateStresses.length ? { plateStresses: shells.plateStresses } : {}),
    ...(shells.quadStresses.length ? { quadStresses: shells.quadStresses } : {}),
    ...(constraint.size ? { constraintForces: [...constraint.values()] } : {}),
    ...(first?.diagnostics ? { diagnostics: first.diagnostics } : {}),
    ...(first?.solverDiagnostics ? { solverDiagnostics: first.solverDiagnostics } : {}),
    ...(first?.structuredDiagnostics ? { structuredDiagnostics: first.structuredDiagnostics } : {}),
    ...(first?.timings ? { timings: first.timings } : {}),
  };
}
