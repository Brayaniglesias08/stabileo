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

const { discretizationStore } = await import('../discretization.svelte');

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
});
