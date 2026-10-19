/**
 * The discretization store: estimates only 3D base solves with shells, keeps the estimate
 * labelled uncertified, keys the element map as the viewport keys shells, and scales the map
 * from zero to at least twice the threshold.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

const estimate = {
  certified: false, method: 'zz-spr', relativeError: 0.08, errorNorm: 1, solutionNorm: 12, threshold: 0.05,
  exceedsThreshold: true, frameMembers: 0, notes: ['notCertified'],
  elements: [
    { kind: 'quad', elementId: 3, eta: 0.5, energyNorm: 2, localPercent: 14 },
    { kind: 'plate', elementId: 4, eta: 0.1, energyNorm: 2, localPercent: 2 },
    { kind: 'curvedShell', elementId: 5, eta: 0.1, energyNorm: 2, localPercent: 3 },
  ],
};
const runCertify = vi.fn(async () => estimate);
let wire: Record<string, unknown> = {};

vi.mock('../../engine/certify-jobs', () => ({ runCertify: (...a: unknown[]) => runCertify(...(a as [])), hasCertify: () => true }));
vi.mock('../../engine/solver-service', () => ({ certifiableInputOf: () => ({ dim: '3d', wire: () => wire }) }));

const { discretizationStore, discretizationClaims, isInconclusive } = await import('../discretization.svelte');
const { resultsStore } = await import('../results.svelte');

async function settle() {
  await vi.advanceTimersByTimeAsync(1000);
}

describe('discretization store', () => {
  beforeEach(() => { vi.useFakeTimers(); runCertify.mockClear(); });

  it('estimates a 3D solve with shells, and labels it uncertified', async () => {
    wire = { quads: { 3: {} }, plates: { 4: {} } };
    discretizationStore.track({}, '3d');
    expect(discretizationStore.estimate.status).toBe('pending');
    await settle();
    const s = discretizationStore.estimate;
    expect(s.status).toBe('done');
    if (s.status !== 'done') return;
    expect(s.estimate.certified).toBe(false);
    expect([...discretizationStore.elementMap()]).toEqual([['q3', 14], ['p4', 2]]);
    expect(discretizationStore.mapRange()).toEqual({ min: 0, max: 14 });
  });

  it('leaves models without shells, and 2D solves, alone', async () => {
    wire = { quads: {}, plates: {} };
    discretizationStore.track({}, '3d');
    await settle();
    expect(discretizationStore.estimate.status).toBe('idle');
    discretizationStore.track({}, '2d');
    await settle();
    expect(discretizationStore.estimate.status).toBe('idle');
    expect(runCertify).not.toHaveBeenCalled();
  });

  it('applies only while the estimated base result is the one on screen', async () => {
    wire = { quads: { 3: {} } };
    const base = { displacements: [], reactions: [], elementForces: [] } as never;
    discretizationStore.track(base, '3d');
    resultsStore.setResults3D(base);
    await settle();
    expect(discretizationStore.applies()).toBe(true);
    expect(discretizationStore.canRunStudy()).toBe(true);
    // A P-Δ result replaces the results on screen: the linear estimate no longer describes them.
    resultsStore.setPDeltaResult3D({ results: { displacements: [], reactions: [], elementForces: [] }, iterations: 1, converged: true } as never);
    expect(discretizationStore.applies()).toBe(false);
    expect(discretizationStore.canRunStudy()).toBe(false);
  });

  it('never shows 0 % for an estimate that could not be made, nor maps it', async () => {
    const unest = { ...estimate, relativeError: undefined, unestimableReason: 'noStrainEnergy', exceedsThreshold: true };
    const c = discretizationClaims(unest as never, 0);
    expect(c.percent).toBeNull();
    expect(c.unestimable).toBe('noStrainEnergy');
    expect(c.warnUnestimable).toBe('discretization.warningUnestimable');
    expect(discretizationClaims({ ...unest, unestimableReason: 'tooCoarse' } as never, 0).warnUnestimable).toBe('discretization.warningTooCoarse');
    expect(discretizationClaims({ ...unest, unestimableReason: 'unloaded', exceedsThreshold: false } as never, 0).warnUnestimable).toBeNull();
    runCertify.mockImplementationOnce(async () => unest as never);
    wire = { quads: { 3: {} } };
    discretizationStore.track({}, '3d');
    await settle();
    expect(discretizationStore.elementMap().size).toBe(0);
  });

  it('claims exact members only when every member is, and none was cut into pieces', () => {
    const exact = { ...estimate, notes: ['notCertified', 'framesExact'], frameMembers: 3, inexactMembers: [] };
    expect(discretizationClaims(exact as never, 0).framesExact).toBe(true);
    // A member of variable section is solved as prismatic pieces: not exact.
    const tapered = discretizationClaims(exact as never, 1);
    expect(tapered.framesExact).toBe(false);
    expect(tapered.variableMembers).toBe(1);
    // Warping members: the engine withholds the claim and lists them.
    const warping = { ...estimate, notes: ['notCertified', 'warpingMembersNotEstimated'], frameMembers: 3, inexactMembers: [2] };
    const w = discretizationClaims(warping as never, 0);
    expect(w.framesExact).toBe(false);
    expect(w.warpingMembers).toBe(1);
    // Curved beams.
    const curved = { ...estimate, notes: ['notCertified', 'curvedBeamsSegmented'], frameMembers: 1 };
    expect(discretizationClaims(curved as never, 0).framesExact).toBe(false);
    expect(discretizationClaims(curved as never, 0).curvedBeams).toBe(true);
  });

  it('leaves elements that were not estimated off the map, and counts them', async () => {
    const partial = {
      ...estimate,
      isolatedElements: [9],
      elements: [...estimate.elements, { kind: 'quad', elementId: 9, eta: 0, energyNorm: 1, localPercent: undefined }],
    };
    expect(discretizationClaims(partial as never, 0).isolatedElements).toBe(1);
    runCertify.mockImplementationOnce(async () => partial as never);
    wire = { quads: { 3: {}, 9: {} } };
    discretizationStore.track({}, '3d');
    await settle();
    expect(discretizationStore.elementMap().has('q9')).toBe(false);
    expect(discretizationStore.elementMap().get('q3')).toBe(14);
  });

  it('treats oscillating, non-converging, implausible and incomplete sequences as inconclusive', () => {
    for (const s of ['oscillating', 'notConverging', 'irregularOrder', 'incomplete'] as const) expect(isInconclusive(s)).toBe(true);
    for (const s of ['converging', 'converged', 'assumedOrder', 'zero'] as const) expect(isInconclusive(s)).toBe(false);
  });
});
