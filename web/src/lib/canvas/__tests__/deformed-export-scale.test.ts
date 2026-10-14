import { beforeEach, expect, it } from 'vitest';
import { modelStore, resultsStore, uiStore } from '../../store';
import { maxDeformedDisplacement2D } from '../draw-deformed';
import { createSvgContext } from '../../export/svg-context';
import { effectiveBendingInertia } from '../../engine/solver-service';

beforeEach(() => {
  modelStore.clear(); resultsStore.clear();
  uiStore.analysisMode = '2d';
});

it.each(['simple', 'fixed'] as const)('measures interior deflection of a %s span with stationary nodes', support => {
  const a = modelStore.addNode(0, 0), b = modelStore.addNode(6, 0);
  const e = modelStore.addElement(a, b, 'frame');
  modelStore.addSupport(a, support === 'simple' ? 'pinned' : 'fixed');
  modelStore.addSupport(b, support === 'simple' ? 'rollerX' : 'fixed');
  modelStore.addDistributedLoad(e, -10, -10);
  const r = modelStore.solve(false);
  if (!r || typeof r === 'string') throw new Error(String(r));
  resultsStore.setResults(r);
  expect(r.displacements.every(d => Math.hypot(d.ux, d.uz) < 1e-12)).toBe(true);
  const elem = modelStore.elements.get(e)!;
  const ei = modelStore.materials.get(elem.materialId)!.e * 1000 * effectiveBendingInertia(modelStore.sections.get(elem.sectionId)!);
  const max = maxDeformedDisplacement2D(resultsStore.results!, {
    ctx: createSvgContext(800, 600).ctx,
    worldToScreen: (x, y) => ({ x, y }),
    getNode: id => modelStore.nodes.get(id),
    getElement: id => modelStore.elements.get(id),
    getMaterial: id => modelStore.materials.get(id),
    getSection: id => { const s = modelStore.sections.get(id); return s ? { iz: effectiveBendingInertia(s) } : undefined; },
  });
  expect(max).toBeCloseTo((support === 'simple' ? 5 : 1) * 10 * 6 ** 4 / (384 * ei), 8);
});
