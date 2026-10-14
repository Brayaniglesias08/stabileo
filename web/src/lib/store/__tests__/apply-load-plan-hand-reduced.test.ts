/**
 * The generator's live load beside a live case reduced by hand (`LoadCase.reduction`).
 *
 * With its reduction on, the generator writes live loads reduced already (`load-plan.ts`). Filed
 * into the user's first L case, or into one that happens to share the generated name, they were
 * reduced a second time by the case's own ratio: 240 kN of live load read 144 under a hand ratio
 * of 0,6. The other order was refused already (`case-reduction.ts`, a case holding the
 * generator's reduced loads cannot be reduced by hand). So the generated load goes to a case of
 * its own, and the user's reduced case stays as it is, beside it in every live combination.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { modelStore } from '../model.svelte';
import '../index';
import { applyLoadPlan } from '../apply-load-plan';
import { buildLoadPlan, type LoadPlanInput } from '../../engine/loads/load-plan';
import { defaultRegulations, type ProjectRegulations } from '../../codes/roles';
import { withCaseEffects } from '../../engine/case-effects';
import es from '../../i18n/locales/es';

const nameOf = (key: string, params?: Record<string, string | number>) =>
  (es[key] ?? key).replace(/\{(\w+)\}/g, (_, k: string) => String(params?.[k] ?? ''));

const applied = (reg: ProjectRegulations): ProjectRegulations => Object.fromEntries(Object.entries(reg).map(([k, v]) =>
  [k, v.adapterId ? { ...v, configComplete: true, state: 'applied' } : v])) as ProjectRegulations;

/** One storey of 3 × 2 bays of 5 m at z = 3. Returns a beam id. */
function frame(): number {
  modelStore.clear();
  for (const c of [...modelStore.model.loadCases]) modelStore.removeLoadCase(c.id);
  const at = new Map<string, number>();
  for (const z of [0, 3]) for (let j = 0; j <= 2; j++) for (let i = 0; i <= 3; i++) at.set(`${i},${j},${z}`, modelStore.addNode(5 * i, 5 * j, z));
  for (let j = 0; j <= 2; j++) for (let i = 0; i <= 3; i++) modelStore.addElement(at.get(`${i},${j},0`)!, at.get(`${i},${j},3`)!);
  let beam = 0;
  for (let j = 0; j <= 2; j++) for (let i = 0; i < 3; i++) beam = modelStore.addElement(at.get(`${i},${j},3`)!, at.get(`${i + 1},${j},3`)!);
  for (let i = 0; i <= 3; i++) for (let j = 0; j < 2; j++) modelStore.addElement(at.get(`${i},${j},3`)!, at.get(`${i},${j + 1},3`)!);
  return beam;
}

function planInput(over: Partial<LoadPlanInput> = {}): LoadPlanInput {
  return {
    regulations: applied(defaultRegulations()),
    model: {
      nodes: modelStore.nodes as never, elements: modelStore.elements as never,
      sections: modelStore.model.sections as never, materials: modelStore.model.materials as never,
      loadCases: modelStore.model.loadCases, quads: modelStore.model.quads as never,
    },
    dead: [{ labelKey: 'a', q: 1.5 }], occupancyKey: 'vivienda', tributaryWidth: 3,
    reductionElementKind: 'interiorBeam', floorsSupported: 1, applyLiveReduction: true,
    generateCombinations: true, gravity: { mode: 'panels' },
    ...over,
  };
}

/** The vertical load of a case's distributed loads as solved, kN (uniform, over whole members of 5 m). */
const solvedLoadOf = (caseId: number) => {
  const m = modelStore.model;
  const solved = withCaseEffects({ ...m, nodes: m.nodes, elements: m.elements, loads: modelStore.loads } as never, m.loadCases, { includeSelfWeight: false, leftHand: false });
  return (solved as { loads: Array<{ type: string; data: { caseId?: number; qZI?: number } }> }).loads
    .filter((l) => l.type === 'distributed3d' && (l.data.caseId ?? 1) === caseId)
    .reduce((s, l) => s + (l.data.qZI ?? 0) * 5, 0);
};

describe("the generator's live load beside a case reduced by hand", () => {
  let beam = 0;
  beforeEach(() => { beam = frame(); });

  // The generated live case's own name, and another: findCase took the first L case by type,
  // and Apply's name match the one of the same name.
  for (const name of ['Sobrecarga', 'Mis sobrecargas']) {
    it(`is never written into it, nor reduced twice (${name})`, () => {
      expect(nameOf('autoLoad.liveCase')).toBe('Sobrecarga');
      const mine = modelStore.addLoadCase(name, 'L');
      modelStore.addDistributedLoad3D(beam, 0, 0, -10, -10, undefined, undefined, mine);
      modelStore.updateLoadCaseFields(mine, { reduction: { ratio: 0.6, tributaryAreaM2: 80, elementKind: 'interiorBeam', floorsSupported: 1 } });

      for (const clearExisting of [false, true]) {
        const plan = buildLoadPlan(planInput());
        expect(plan.outcome).toBe('READY');
        applyLoadPlan(plan, { clearExisting, bothSenses: true, nameOf });

        const generated = modelStore.loads.filter((l) => !!(l.data as { generatedBy?: string }).generatedBy);
        const live = modelStore.model.loadCases.filter((c) => c.type === 'L');
        // The user's case keeps its own load and its reduction, and nothing the generator wrote.
        expect(generated.some((l) => (l.data.caseId ?? 1) === mine)).toBe(false);
        expect(modelStore.loads.filter((l) => (l.data.caseId ?? 1) === mine)).toHaveLength(1);
        expect(live.find((c) => c.id === mine)!.reduction?.ratio).toBe(0.6);
        expect(solvedLoadOf(mine)).toBeCloseTo(-10 * 5 * 0.6, 9);
        // One case of its own for the generated live load, also on the second apply.
        const theirs = live.filter((c) => c.id !== mine);
        expect(theirs).toHaveLength(1);
        const gen = theirs[0]!;
        expect(gen.reduction).toBeUndefined();
        // Solved once reduced: what the plan wrote, not that times 0,6.
        const planned = plan.distributed.filter((d) => d.caseType === 'L').reduce((s, d) => s + d.q * 5, 0);
        expect(planned).toBeLessThan(0);
        expect(solvedLoadOf(gen.id)).toBeCloseTo(planned, 9);
        // The user's case enters every live combination beside it, at its factor.
        const withLive = modelStore.model.combinations.filter((c) => c.factors.some((f) => f.caseId === gen.id && f.factor !== 0));
        expect(withLive.length).toBeGreaterThan(0);
        for (const c of withLive) {
          const k = c.factors.find((f) => f.caseId === gen.id)!.factor;
          expect(c.factors.find((f) => f.caseId === mine)?.factor).toBe(k);
        }
      }
    });
  }

  it('a live case without a hand reduction still takes the generated load, as before', () => {
    const mine = modelStore.addLoadCase('Mis sobrecargas', 'L');
    const plan = buildLoadPlan(planInput());
    applyLoadPlan(plan, { clearExisting: false, bothSenses: true, nameOf });
    expect(modelStore.model.loadCases.filter((c) => c.type === 'L').map((c) => c.id)).toEqual([mine]);
    expect(modelStore.loads.some((l) => (l.data.caseId ?? 1) === mine)).toBe(true);
  });
});
