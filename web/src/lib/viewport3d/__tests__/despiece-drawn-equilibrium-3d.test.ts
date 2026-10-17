/**
 * The 3D despiece balances as it is DRAWN: the end actions it draws (after its negligible filter,
 * at the share the viewport uses) against the loads it draws (`displayedLoads3D`, the result's
 * own, self-weight included), member by member and joint by joint. The other suites check the
 * actions against the loads the solver was given, which is how a filter that hid a needed shear,
 * a load of another case and a point moment that was never drawn all got through.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { modelStore, resultsStore, uiStore } from '../../store';
import { freeBodyActions3D } from '../free-body-3d';
import { displayedLoads3D, despieceNote3D } from '../../store/displayed-loads-3d';
import type { FBAction, FBLoad, P3 } from '../../canvas/free-body';
import type { AnalysisResults3D } from '../../engine/types-3d';
import { createFigure3DRenderer } from '../../export/figure-3d';
import { createSvgContext } from '../../export/svg-context';
import { DEFAULT_FIGURE_OPTIONS } from '../../export/view-figure';
import { t } from '../../i18n';

function figure(results?: AnalysisResults3D): string {
  const svg = createSvgContext(800, 600);
  createFigure3DRenderer().render(svg.ctx, 800, 600,
    { content: 'despiece', options: { ...DEFAULT_FIGURE_OPTIONS, titles: false, ...(results ? { results } : {}) } },
    { zoom: 100, panX: 100, panY: 300 });
  return svg.body();
}

type V = [number, number, number];
const add = (a: V, b: V): V => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a: V, b: V): V => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const scale = (a: V, k: number): V => [a[0] * k, a[1] * k, a[2] * k];
const cross = (a: V, b: V): V => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const norm = (a: V) => Math.hypot(...a);
const v3 = (p: P3): V => [p.x, p.y, p.z ?? 0];
const Z: V = [0, 0, 0];
const vecOf = (a: Pick<FBAction, 'dir' | 'magnitude'>): V => scale(v3(a.dir), a.magnitude);

/** The VIEWPORT's share: what is drawn on screen and in the figures. */
const SHARE = 0.02;

function drawn(r: AnalysisResults3D, o: { basis?: 'local' | 'global'; loadMode?: 'all' | 'resultant' } = {}) {
  return freeBodyActions3D({
    elements: modelStore.elements, nodes: modelStore.nodes, forces: r.elementForces, reactions: r.reactions ?? [],
    sections: modelStore.sections, project2D: false, leftHand: false, basis: o.basis ?? 'local', resultant: false,
    showReactions: true, loads: displayedLoads3D(r), loadText: (v) => String(v), negligible: SHARE, loadMode: o.loadMode ?? 'all',
  });
}

/** The force and the moment about `about` of one drawn load. */
function loadAction(ld: FBLoad, data: ReturnType<typeof drawn>, about: V): { f: V; m: V; memberId?: number; nodeId?: number } {
  if (ld.kind === 'nodal') {
    const v = scale(v3(ld.dir), parseFloat(ld.label));
    if (ld.action === 'moment') return { f: Z, m: v, nodeId: ld.nodeId };
    return { f: v, m: cross(sub(v3(data.nodes(ld.nodeId)!), about), v), nodeId: ld.nodeId };
  }
  const mem = data.members.find((m) => m.id === ld.memberId)!;
  const i = v3(mem.i), j = v3(mem.j), L = norm(sub(j, i));
  const at = (t: number) => add(i, scale(sub(j, i), t));
  if (ld.kind === 'point') {
    const v = scale(v3(ld.dir), parseFloat(ld.label));
    if (ld.action === 'moment') return { f: Z, m: v, memberId: ld.memberId };
    return { f: v, m: cross(sub(at(ld.t), about), v), memberId: ld.memberId };
  }
  // A linear load: its two triangles, each at its third point.
  const len = (ld.t1 - ld.t0) * L;
  let f = Z, m = Z;
  for (const [q, t] of [[v3(ld.qI), ld.t0 + (ld.t1 - ld.t0) / 3], [v3(ld.qJ), ld.t0 + (2 * (ld.t1 - ld.t0)) / 3]] as Array<[V, number]>) {
    const p = scale(q, len / 2);
    f = add(f, p); m = add(m, cross(sub(at(t), about), p));
  }
  return { f, m, memberId: ld.memberId };
}

/** The largest out-of-balance force and moment over every drawn member and every drawn joint. */
function residuals(data: ReturnType<typeof drawn>): { member: { f: number; m: number }; joint: { f: number; m: number } } {
  const out = { member: { f: 0, m: 0 }, joint: { f: 0, m: 0 } };
  for (const mem of data.members) {
    const about = v3(mem.i);
    let f = Z, m = Z;
    for (const e of data.ends.filter((x) => x.memberId === mem.id && x.side === 'member')) {
      const at = v3(data.nodes(e.nodeId)!);
      for (const a of e.actions) {
        if (a.kind === 'force') { f = add(f, vecOf(a)); m = add(m, cross(sub(at, about), vecOf(a))); } else m = add(m, vecOf(a));
      }
    }
    for (const ld of data.loads) {
      if (ld.kind === 'nodal' || ld.memberId !== mem.id) continue;
      const l = loadAction(ld, data, about);
      f = add(f, l.f); m = add(m, l.m);
    }
    out.member.f = Math.max(out.member.f, norm(f)); out.member.m = Math.max(out.member.m, norm(m));
  }
  const nodes = new Set(data.members.flatMap((m) => [m.nodeI, m.nodeJ]));
  for (const n of nodes) {
    let f = Z, m = Z;
    for (const e of data.ends.filter((x) => x.nodeId === n && x.side === 'node')) {
      for (const a of e.actions) if (a.kind === 'force') f = add(f, vecOf(a)); else m = add(m, vecOf(a));
    }
    for (const s of data.supports.filter((x) => x.nodeId === n)) {
      for (const a of s.actions) if (a.kind === 'force') f = add(f, vecOf(a)); else m = add(m, vecOf(a));
    }
    for (const ld of data.loads) {
      if (ld.kind !== 'nodal' || ld.nodeId !== n) continue;
      const l = loadAction(ld, data, v3(data.nodes(n)!));
      f = add(f, l.f); m = add(m, l.m);
    }
    out.joint.f = Math.max(out.joint.f, norm(f)); out.joint.m = Math.max(out.joint.m, norm(m));
  }
  return out;
}

const balanced = (r: ReturnType<typeof residuals>, tol = 1e-6) => {
  expect(r.member.f).toBeLessThan(tol); expect(r.member.m).toBeLessThan(tol);
  expect(r.joint.f).toBeLessThan(tol); expect(r.joint.m).toBeLessThan(tol);
};

beforeEach(() => {
  modelStore.clear(); resultsStore.clear();
  uiStore.analysisMode = '3d'; uiStore.includeSelfWeight = false; uiStore.axisConvention3D = 'rightHand';
  uiStore.unitSystem = 'SI';
  uiStore.cameraPosition3D = { x: 2, y: -10, z: 0 };
  uiStore.cameraTarget3D = { x: 2, y: 0, z: 0 };
  for (const c of [...modelStore.combinations]) modelStore.removeCombination(c.id);
});

function solveSingle(includeSelfWeight = false): AnalysisResults3D {
  uiStore.includeSelfWeight = includeSelfWeight;
  const r = modelStore.solve3D(includeSelfWeight, false);
  if (!r || typeof r === 'string') throw new Error(String(r));
  resultsStore.setResults3D(r);
  return resultsStore.results3D!;
}

/** A fixed portal, 4 m columns and a 6 m beam, 1000 kN down each column head and 15 kN of sway. */
function portal() {
  const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(0, 0, 4), c = modelStore.addNode(6, 0, 4), d = modelStore.addNode(6, 0, 0);
  modelStore.addElement(a, b, 'frame'); modelStore.addElement(b, c, 'frame'); modelStore.addElement(c, d, 'frame');
  modelStore.addSupport(a, 'fixed3d'); modelStore.addSupport(d, 'fixed3d');
  modelStore.addNodalLoad3D(b, 15, 0, -1000, 0, 0, 0);
  modelStore.addNodalLoad3D(c, 0, 0, -1000, 0, 0, 0);
  return { a, b, c, d };
}

describe('the 3D despiece balances as drawn', () => {
  it.each(['local', 'global'] as const)('a portal under heavy gravity and a small sway keeps its shears and reactions (%s)', (basis) => {
    portal();
    const r = solveSingle();
    const data = drawn(r, { basis });
    // The sway's reactions, 0.75 % of the gravity ones, are drawn: the joints and the whole balance.
    const rx = data.supports.flatMap((s) => s.actions).filter((a) => a.name === 'Rx');
    expect(rx).toHaveLength(2);
    expect(rx.reduce((t, a) => t + vecOf(a)[0], 0)).toBeCloseTo(-15, 6);
    balanced(residuals(data));
  });

  it('draws a point moment on a member, and the member balances with it', () => {
    const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(6, 0, 0);
    const e = modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'fixed3d'); modelStore.addSupport(b, 'fixed3d');
    modelStore.addLoadEntry({ type: 'pointOnElement3d', data: { id: 0, elementId: e, a: 2, py: 0, pz: 0, my: 12 } });
    const data = drawn(solveSingle());
    expect(data.loads).toMatchObject([{ kind: 'point', action: 'moment', memberId: e, label: '12' }]);
    balanced(residuals(data));
    // And it is on the drawing (its end moments are 0 and 4 kN·m: the 12 is the load's).
    expect(figure()).toContain('12.0 kN');
  });

  it("draws the single solve's self-weight, which its reactions carry", () => {
    const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(6, 0, 0);
    modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'fixed3d'); modelStore.addSupport(b, 'fixed3d');
    const r = solveSingle(true);
    const up = (r.reactions ?? []).reduce((t, x) => t + x.fz, 0);
    expect(up).toBeGreaterThan(1);
    const loads = displayedLoads3D(r);
    expect(loads).toHaveLength(1);
    expect(loads[0]).toMatchObject({ type: 'distributed3d', data: { frame: 'global' } });
    balanced(residuals(drawn(r)));
  });

  it("a figure of a combination draws the combination's loads, whatever is on screen", () => {
    const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(4, 0, 0);
    modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'fixed3d');
    const [dead, live] = modelStore.loadCases;
    modelStore.addNodalLoad3D(b, 0, 0, -10, 0, 0, 0, dead.id);
    modelStore.addNodalLoad3D(b, 0, 0, -30, 0, 0, 0, live.id);
    const combo = modelStore.addCombination('D + L', [{ caseId: dead.id, factor: 1 }, { caseId: live.id, factor: 1 }]);
    uiStore.includeSelfWeight = true;
    const solved = modelStore.solveCombinations3D(true, false);
    if (!solved || typeof solved === 'string') throw new Error(String(solved));
    resultsStore.setCombinationResults3D(solved.perCase, solved.perCombo, solved.envelope);
    resultsStore.activeView = 'single'; resultsStore.activeCaseId = dead.id;
    const comboResult = resultsStore.perCombo3D.get(combo)!;
    const fz = (r: AnalysisResults3D) => (r.reactions ?? []).reduce((t, x) => t + x.fz, 0);
    expect(fz(comboResult)).toBeGreaterThan(40);
    // The combination's own loads (both cases, and the dead case's weight) against its reactions.
    balanced(residuals(drawn(comboResult)));
    // And the case on screen still draws its own.
    balanced(residuals(drawn(resultsStore.results3D!)));
    expect(fz(resultsStore.results3D!)).toBeLessThan(fz(comboResult) - 29);
    // The figure of the combination, exported with the case on screen, carries the live load.
    expect(figure(comboResult)).toContain('30.0 kN');
    expect(figure()).not.toContain('30.0 kN');
  });

  it('an envelope or a magnitude is no state of equilibrium: no loads, and said', () => {
    const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(4, 0, 0);
    modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'fixed3d');
    const [dead] = modelStore.loadCases;
    modelStore.addNodalLoad3D(b, 0, 0, -10, 0, 0, 0, dead.id);
    modelStore.addCombination('1.4D', [{ caseId: dead.id, factor: 1.4 }]);
    const solved = modelStore.solveCombinations3D(false, false);
    if (!solved || typeof solved === 'string') throw new Error(String(solved));
    resultsStore.setCombinationResults3D(solved.perCase, solved.perCombo, solved.envelope);
    resultsStore.activeView = 'envelope';
    expect(despieceNote3D()).toBe('envelope');
    expect(displayedLoads3D()).toEqual([]);
    // Its figure is the pieces and the note: no maxima drawn as one state.
    const env = figure();
    expect(env).toContain(t('despiece.noteEnvelope'));
    expect(env).not.toContain('kN');
    expect(despieceNote3D({ displacements: [], reactions: [], elementForces: [], magnitude: { method: 'srss' } } as AnalysisResults3D)).toBe('magnitude');
    resultsStore.activeView = 'single'; resultsStore.activeCaseId = dead.id;
    expect(despieceNote3D()).toBeNull();
  });

  it('a combination taking a spectral case says that only its static loads are drawn', () => {
    const a = modelStore.addNode(0, 0, 0), b = modelStore.addNode(4, 0, 0);
    modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'fixed3d');
    const [dead, live] = modelStore.loadCases;
    modelStore.addNodalLoad3D(b, 0, 0, -10, 0, 0, 0, dead.id);
    const combo = modelStore.addCombination('D + E', [{ caseId: dead.id, factor: 1 }, { caseId: live.id, factor: 1 }]);
    const solved = modelStore.solveCombinations3D(false, false);
    if (!solved || typeof solved === 'string') throw new Error(String(solved));
    resultsStore.setCombinationResults3D(solved.perCase, solved.perCombo, solved.envelope);
    // The case read as a spectral one (its result here a static stand-in: only the note is checked).
    Object.assign(modelStore.loadCases.find((c) => c.id === live.id)!, { spectral: { source: { kind: 'code' }, factors: { x: 1, y: 0.3, z: 0 }, rule: 'cqc', xi: 0.05, scale: 1 } });
    expect(despieceNote3D(resultsStore.perCombo3D.get(combo))).toBe('spectral');
    expect(despieceNote3D(resultsStore.perCase3D.get(live.id))).toBe('spectral');
    expect(despieceNote3D(resultsStore.perCase3D.get(dead.id))).toBeNull();
  });
});
