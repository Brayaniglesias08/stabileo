/**
 * The 2D despiece balances as it is DRAWN: the end actions and reactions it draws (after its
 * negligible filter, at the viewport's share) against the loads it draws (`displayedLoads2D`, the
 * result's own, self-weight included), member by member and joint by joint, for the single
 * solve, each case and each combination. Every case's loads used to be drawn unfactored under
 * every result, which no free body but the single solve's balanced.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { modelStore, resultsStore, uiStore } from '../../store';
import { freeBodyActions2D } from '../free-body-2d';
import { computeLoadDirection } from '../draw-loads';
import { distributedResultantForces } from '../draw-despiece';
import { displayedLoads2D, despieceNote2D } from '../../store/displayed-loads-2d';
import type { AnalysisResults } from '../../engine/types';
import type { FBAction } from '../free-body';
import type { Load } from '../../store/model.svelte';

/** The VIEWPORT's share. */
const SHARE = 0.02;
type P = { x: number; y: number };
const node = (id: number): P => { const n = modelStore.getNode(id)!; return { x: n.x, y: n.y }; };
const cross = (r: P, f: P) => r.x * f.y - r.y * f.x;

function drawnActions(r: AnalysisResults, basis: 'local' | 'global' = 'local') {
  const reactions = new Map(r.reactions.map((x) => [x.nodeId, { rx: x.rx, rz: x.rz, my: x.my }]));
  return freeBodyActions2D({
    elements: [...modelStore.elements.values()].map((e) => ({ id: e.id, nodeI: e.nodeI, nodeJ: e.nodeJ })),
    getNode: node, getElementForces: (id) => r.elementForces.find((f) => f.elementId === id),
    reactions, basis, resultant: false, showReactions: true, negligible: SHARE,
  });
}

/** Force and moment (CCW +) about `about` of one drawn load, read as the drawing reads it. */
function loadAction(l: Load, about: P): { f: P; m: number; elementId?: number; nodeId?: number } {
  if (l.type === 'nodal') {
    const d = l.data, f = { x: d.fx, y: d.fz ?? d.fy ?? 0 }, at = node(d.nodeId);
    return { f, m: cross({ x: at.x - about.x, y: at.y - about.y }, f) + (d.my ?? d.mz ?? 0), nodeId: d.nodeId };
  }
  if (l.type !== 'distributed' && l.type !== 'pointOnElement') throw new Error(l.type);
  const el = modelStore.elements.get(l.data.elementId)!;
  const i = node(el.nodeI), j = node(el.nodeJ), L = Math.hypot(j.x - i.x, j.y - i.y);
  const c = (j.x - i.x) / L, s = (j.y - i.y) / L;
  const dir = computeLoadDirection(l.data.angle ?? 0, l.data.isGlobal ?? false, c, s);
  const at = (x: number): P => ({ x: i.x + c * x - about.x, y: i.y + s * x - about.y });
  if (l.type === 'pointOnElement') {
    const d = l.data, f = { x: d.p * dir.dx + (d.px ?? 0) * c, y: d.p * dir.dy + (d.px ?? 0) * s };
    return { f, m: cross(at(d.a), f) + (d.my ?? d.mz ?? 0), elementId: d.elementId };
  }
  const d = l.data, a = d.a ?? 0, b = d.b ?? L, len = b - a;
  let f = { x: 0, y: 0 }, m = 0;
  for (const [q, x] of [[d.qI, a + len / 3], [d.qJ, a + (2 * len) / 3]] as Array<[number, number]>) {
    const p = { x: (q * len / 2) * dir.dx, y: (q * len / 2) * dir.dy };
    f = { x: f.x + p.x, y: f.y + p.y }; m += cross(at(x), p);
  }
  return { f, m, elementId: d.elementId };
}

const vec = (a: FBAction): P => ({ x: a.dir.x * a.magnitude, y: a.dir.y * a.magnitude });
const turn = (a: FBAction) => (a.ccw ? 1 : -1) * a.magnitude;

/** The largest out-of-balance force and moment over every member and every joint, as drawn. */
function residual(r: AnalysisResults, basis: 'local' | 'global' = 'local'): { f: number; m: number } {
  const { ends, supports } = drawnActions(r, basis);
  const loads = displayedLoads2D(r);
  let worstF = 0, worstM = 0;
  const close = (f: P, m: number) => { worstF = Math.max(worstF, Math.hypot(f.x, f.y)); worstM = Math.max(worstM, Math.abs(m)); };
  for (const el of modelStore.elements.values()) {
    const about = node(el.nodeI);
    let f = { x: 0, y: 0 }, m = 0;
    for (const e of ends.filter((x) => x.memberId === el.id && x.side === 'member')) {
      const at = node(e.nodeId);
      for (const a of e.actions) {
        if (a.kind === 'moment') { m += turn(a); continue; }
        f = { x: f.x + vec(a).x, y: f.y + vec(a).y }; m += cross({ x: at.x - about.x, y: at.y - about.y }, vec(a));
      }
    }
    for (const l of loads) {
      if ((l.type !== 'distributed' && l.type !== 'pointOnElement') || l.data.elementId !== el.id) continue;
      const a = loadAction(l, about);
      f = { x: f.x + a.f.x, y: f.y + a.f.y }; m += a.m;
    }
    close(f, m);
  }
  for (const id of modelStore.nodes.keys()) {
    let f = { x: 0, y: 0 }, m = 0;
    const acts = [...ends.filter((x) => x.nodeId === id && x.side === 'node').flatMap((x) => x.actions), ...supports.filter((s) => s.nodeId === id).flatMap((s) => s.actions)];
    for (const a of acts) if (a.kind === 'moment') m += turn(a); else f = { x: f.x + vec(a).x, y: f.y + vec(a).y };
    for (const l of loads) {
      if (l.type !== 'nodal' || l.data.nodeId !== id) continue;
      const a = loadAction(l, node(id));
      f = { x: f.x + a.f.x, y: f.y + a.f.y }; m += a.m;
    }
    close(f, m);
  }
  return { f: worstF, m: worstM };
}

const expectBalanced = (r: AnalysisResults, basis: 'local' | 'global' = 'local') => {
  const res = residual(r, basis);
  expect(res.f).toBeLessThan(1e-6);
  expect(res.m).toBeLessThan(1e-6);
};

function solve(includeSelfWeight = false) {
  uiStore.includeSelfWeight = includeSelfWeight;
  const single = modelStore.solve(includeSelfWeight);
  if (!single || typeof single === 'string') throw new Error(String(single));
  resultsStore.setResults(single);
  if (!modelStore.combinations.length) return;
  const c = modelStore.solveCombinations(includeSelfWeight);
  if (!c || typeof c === 'string') throw new Error(String(c));
  resultsStore.setCombinationResults(c.perCase, c.perCombo, c.envelope);
}

beforeEach(() => {
  modelStore.clear(); resultsStore.clear();
  uiStore.analysisMode = '2d'; uiStore.includeSelfWeight = false; uiStore.selfWeightCaseId = null;
  for (const c of [...modelStore.combinations]) modelStore.removeCombination(c.id);
});

describe('the 2D despiece balances as drawn', () => {
  it('draws the loads of the result on screen: a case, a combination, the single solve', () => {
    // A 6 m beam: dead 10 kN/m, live 20 kN at midspan.
    const a = modelStore.addNode(0, 0), b = modelStore.addNode(6, 0);
    const e = modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'pinned'); modelStore.addSupport(b, 'rollerX');
    const [D, L] = modelStore.loadCases;
    modelStore.addDistributedLoad(e, -10, -10, 0, false, D.id);
    modelStore.addPointLoadOnElement(e, 3, -20, { caseId: L.id });
    const combo = modelStore.addCombination('1.2D + 1.6L', [{ caseId: D.id, factor: 1.2 }, { caseId: L.id, factor: 1.6 }]);
    solve();
    const up = (r: AnalysisResults) => r.reactions.reduce((t, x) => t + x.rz, 0);
    const down = (r: AnalysisResults) => -displayedLoads2D(r).reduce((t, l) => t + loadAction(l, { x: 0, y: 0 }).f.y, 0);
    const views: Array<[string, AnalysisResults]> = [
      ['single', resultsStore.singleResults!], ['D', resultsStore.perCase.get(D.id)!],
      ['L', resultsStore.perCase.get(L.id)!], ['combo', resultsStore.perCombo.get(combo)!],
    ];
    expect(views.map(([, r]) => up(r))).toEqual([expect.closeTo(80), expect.closeTo(60), expect.closeTo(20), expect.closeTo(104)]);
    for (const [, r] of views) {
      expect(down(r)).toBeCloseTo(up(r), 9);
      expectBalanced(r);
    }
    // Through the selection on screen, too.
    resultsStore.activeView = 'combo'; resultsStore.activeComboId = combo;
    expect(down(resultsStore.results!)).toBeCloseTo(104, 9);
    resultsStore.activeView = 'single'; resultsStore.activeCaseId = L.id;
    expect(down(resultsStore.results!)).toBeCloseTo(20, 9);
  });

  it('an envelope is no state of equilibrium: no loads, and said', () => {
    const a = modelStore.addNode(0, 0), b = modelStore.addNode(6, 0);
    const e = modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'fixed');
    const [D] = modelStore.loadCases;
    modelStore.addDistributedLoad(e, -10, -10, 0, false, D.id);
    modelStore.addCombination('1.4D', [{ caseId: D.id, factor: 1.4 }]);
    solve();
    resultsStore.activeView = 'envelope';
    expect(despieceNote2D()).toBe('envelope');
    expect(displayedLoads2D()).toEqual([]);
    resultsStore.activeView = 'single'; resultsStore.activeCaseId = D.id;
    expect(despieceNote2D()).toBeNull();
  });

  it.each(['local', 'global'] as const)('a portal under heavy gravity and a small sway keeps its shears and reactions (%s)', (basis) => {
    const a = modelStore.addNode(0, 0), b = modelStore.addNode(0, 4), c = modelStore.addNode(6, 4), d = modelStore.addNode(6, 0);
    modelStore.addElement(a, b, 'frame'); modelStore.addElement(b, c, 'frame'); modelStore.addElement(c, d, 'frame');
    modelStore.addSupport(a, 'fixed'); modelStore.addSupport(d, 'fixed');
    modelStore.addNodalLoad(b, 15, -1000); modelStore.addNodalLoad(c, 0, -1000);
    solve();
    const rx = drawnActions(resultsStore.results!, basis).supports.flatMap((s) => s.actions).filter((x) => x.name === 'Rx');
    expect(rx).toHaveLength(2);
    expectBalanced(resultsStore.results!, basis);
  });

  it('draws the self-weight its reactions carry: the single solve, and the case Basic puts it in', () => {
    // Level members: on an inclined one the plane solve lumps the weight's axial part into its end
    // nodes (`buildSolverLoads2D`), which its free body cannot show; that is the solve's to fix.
    const a = modelStore.addNode(0, 0), b = modelStore.addNode(6, 0), c = modelStore.addNode(9, 0);
    modelStore.addElement(a, b, 'frame'); modelStore.addElement(b, c, 'frame');
    modelStore.addSupport(a, 'fixed'); modelStore.addSupport(c, 'pinned');
    const [D, L] = modelStore.loadCases;
    modelStore.addNodalLoad(b, 0, -5, 0, L.id);
    const combo = modelStore.addCombination('1.2D + 1.6L', [{ caseId: D.id, factor: 1.2 }, { caseId: L.id, factor: 1.6 }]);
    solve(true);
    expect(displayedLoads2D(resultsStore.singleResults!).filter((l) => l.type === 'distributed')).toHaveLength(2);
    expect(displayedLoads2D(resultsStore.perCase.get(L.id)!).filter((l) => l.type === 'distributed')).toHaveLength(0);
    for (const r of [resultsStore.singleResults!, resultsStore.perCase.get(D.id)!, resultsStore.perCase.get(L.id)!, resultsStore.perCombo.get(combo)!]) {
      expectBalanced(r);
    }
  });

  it('a point moment on a member balances it as drawn', () => {
    const a = modelStore.addNode(0, 0), b = modelStore.addNode(6, 0);
    const e = modelStore.addElement(a, b, 'frame');
    modelStore.addSupport(a, 'fixed'); modelStore.addSupport(b, 'fixed');
    modelStore.addPointLoadOnElement(e, 2, 0, { my: 12 });
    solve();
    expectBalanced(resultsStore.results!);
  });
});

describe('the resultant of a distributed load, as drawn', () => {
  /** The drawn forces' total and their moment about I (CCW +), on a beam along +x. */
  const drawn = (qI: number, qJ: number, a: number, b: number) => {
    let f = 0, m = 0;
    for (const r of distributedResultantForces(qI, qJ, a, b, 0, false, 1, 0)) {
      const fy = r.wy * Math.abs(r.magnitude);
      f += fy; m += r.at * fy;
    }
    return { f, m };
  };
  /** The load's exact total and moment about I, by integration. */
  const exact = (qI: number, qJ: number, a: number, b: number) => {
    const n = 2000, h = (b - a) / n;
    let f = 0, m = 0;
    for (let k = 0; k < n; k++) { const x = a + (k + 0.5) * h, q = qI + ((qJ - qI) * (x - a)) / (b - a); f += q * h; m += q * x * h; }
    return { f, m };
  };
  it.each([[-10, 6, 0, 6], [-10, 10, 0, 6], [-4, -8, 1, 5], [3, -9, 2, 6], [0, -6, 0, 3]])(
    'keeps the force and the moment of q %d → %d on [%d, %d]', (qI, qJ, a, b) => {
      const d = drawn(qI, qJ, a, b), x = exact(qI, qJ, a, b);
      expect(d.f).toBeCloseTo(x.f, 6);
      expect(d.m).toBeCloseTo(x.m, 4);
    });
});
