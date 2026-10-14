/**
 * A combination that takes notional cases in the direct analysis: the cases are its notional
 * loads (C2.2b), so the analysis adds none of its own, and their horizontal load is not the
 * combination's lateral load, whatever the drift ratio. Before, the notional case made the
 * combination "lateral" and, past Δ₂/Δ₁ = 1.7, the analysis put its own 0.002 on top of the case's.
 */
import { describe, it, expect, beforeAll, beforeEach } from 'vitest';
import { modelStore } from '../../store/model.svelte';
import '../../store/index';
import { initSolver } from '../wasm-solver';
import { runDirectAnalysis } from '../direct-analysis';

beforeAll(async () => { await initSolver(); });
beforeEach(() => { modelStore.clear(); });

const data = () => {
  const m = modelStore.model;
  return { nodes: m.nodes, elements: m.elements, supports: m.supports, loads: m.loads, materials: m.materials, sections: m.sections };
};

describe('notional cases in the direct analysis', () => {
  // A 4 m cantilever column of the default section: 60 kN puts Δ₂/Δ₁ past 1.7, 20 kN under it.
  for (const [P, regime] of [[60, 'past 1.7'], [20, 'under 1.7']] as const) {
    it(`are the combination's notional loads, Δ₂/Δ₁ ${regime}`, async () => {
      const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(0, 0, 4);
      modelStore.addElement(a, b, 'frame');
      modelStore.addSupport(a, 'fixed3d');
      const D = modelStore.model.loadCases[0]!.id;
      modelStore.addNodalLoad3D(b, 0, 0, -P, 0, 0, 0, D);
      const gravityOnly = modelStore.addCombination('D', [{ caseId: D, factor: 1 }]);
      const first = await runDirectAnalysis(data() as never, modelStore.model.loadCases, modelStore.model.combinations, { includeSelfWeight: false });
      if (typeof first === 'string') throw new Error(first);
      const dir = first.info.get(gravityOnly)!.notional as '+X' | '-X' | '+Y' | '-Y';
      expect(['+X', '-X', '+Y', '-Y']).toContain(dir);
      const b2 = first.info.get(gravityOnly)!.b2;
      expect(P === 60 ? b2 > 1.7 : b2 < 1.7).toBe(true);

      // The same notional load, as a case.
      const N = modelStore.addLoadCase('N', 'N');
      modelStore.updateLoadCaseFields(N, { notional: { sourceCaseId: D, ratio: 0.002, dir } });
      const withN = modelStore.addCombination('D + N', [{ caseId: D, factor: 1 }, { caseId: N, factor: 1 }]);
      const r = await runDirectAnalysis(data() as never, modelStore.model.loadCases, modelStore.model.combinations, { includeSelfWeight: false });
      if (typeof r === 'string') throw new Error(r);
      const u = (combo: number) => {
        const d = r.perCombo.get(combo)!.displacements.find((x) => x.nodeId === b)!;
        return dir.endsWith('X') ? d.ux : d.uy;
      };
      // Once, not twice: the case's 0.002 is the analysis's 0.002.
      expect(Math.abs(u(gravityOnly))).toBeGreaterThan(0);
      expect(u(withN)).toBeCloseTo(u(gravityOnly), 9);
      expect(r.info.get(withN)!.notional).toBe('cases');
    });
  }

  /*
   * A composite case that takes the notional case in, as the generator writes one when it adds
   * notional variants and creates composite cases: the combination takes notional loads through
   * it, at any depth. Before, only a case typed N or with a notional definition counted, so the
   * composite's notional load read as the combination's lateral load and, past 1.7, the analysis
   * added its own 0.002 on top (uy 0.01532 against 0.00766 at 60 kN, reported "lateral").
   */
  for (const P of [20, 40, 60]) {
    it(`reach a combination through a composite case, at any depth (P = ${P} kN)`, async () => {
      const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(0, 0, 4);
      modelStore.addElement(a, b, 'frame');
      modelStore.addSupport(a, 'fixed3d');
      const D = modelStore.model.loadCases[0]!.id;
      modelStore.addNodalLoad3D(b, 0, 0, -P, 0, 0, 0, D);
      const N = modelStore.addLoadCase('N', 'N');
      modelStore.updateLoadCaseFields(N, { notional: { sourceCaseId: D, ratio: 0.002, dir: '+Y' } });
      const C = modelStore.addLoadCase('D + N', '');
      modelStore.updateLoadCaseFields(C, { includes: [{ caseId: D, factor: 1 }, { caseId: N, factor: 1 }] });
      const C2 = modelStore.addLoadCase('outer', '');
      modelStore.updateLoadCaseFields(C2, { includes: [{ caseId: C, factor: 1 }] });
      const direct = modelStore.addCombination('D + N', [{ caseId: D, factor: 1 }, { caseId: N, factor: 1 }]);
      const viaC = modelStore.addCombination('C', [{ caseId: C, factor: 1 }]);
      const viaC2 = modelStore.addCombination('C2', [{ caseId: C2, factor: 1 }]);
      const r = await runDirectAnalysis(data() as never, modelStore.model.loadCases, modelStore.model.combinations, { includeSelfWeight: false });
      if (typeof r === 'string') throw new Error(r);
      const uy = (combo: number) => r.perCombo.get(combo)!.displacements.find((x) => x.nodeId === b)!.uy;
      expect(Math.abs(uy(direct))).toBeGreaterThan(0);
      for (const combo of [viaC, viaC2]) {
        expect(uy(combo)).toBeCloseTo(uy(direct), 9);
        expect(r.info.get(combo)!.notional).toBe('cases');
        expect(r.info.get(combo)!.b2).toBeCloseTo(r.info.get(direct)!.b2, 6);
      }
    });
  }
});
