/**
 * Re-check prompt for old projects, and reproducible results (roadmap: "Trust and verification
 * backlog", items 8 and 9).
 *
 *  - A project saved before projects recorded which version solved them opens with a notice
 *    that lists only the registered changes that concern THAT model, cannot be dismissed
 *    without acknowledging it, and leaves the results marked stale until they are re-run.
 *  - The same model solved twice, and on the worker vs the main thread, gives bit-identical
 *    results; the reproduce dialog confirms a report's provenance.
 */
import { test, expect } from './fixtures';
import type { Page } from '@playwright/test';

const STEEL = { id: 1, name: 'F-24', e: 200000, nu: 0.3, rho: 78.5, fy: 235 };
const CONCRETE = { id: 2, name: 'H-25', e: 30000, nu: 0.2, rho: 25, fy: 25 };

/** A `.ded` as a build before solve stamps wrote it: no `snapshot.solvedWith`. */
function oldProject(materialId: 1 | 2, type: 'truss' | 'frame') {
  const rel = { tx: false, ry: false, rz: false, my: false, mz: false, t: false };
  return {
    version: '2.0',
    name: materialId === 1 ? 'Old steel truss' : 'Old concrete frame',
    timestamp: '2026-09-01T12:00:00.000Z',
    analysisMode: 'pro',
    snapshot: {
      localAxisConvention: 'zUpStrongAxis',
      nodes: [[1, { id: 1, x: 0, y: 0, z: 0 }], [2, { id: 2, x: 4, y: 0, z: 0 }], [3, { id: 3, x: 2, y: 0, z: 2 }]],
      materials: [[1, STEEL], [2, CONCRETE]],
      sections: [[1, { id: 1, name: 'S', a: 0.01, iz: 1e-4, iy: 1e-4, j: 1e-5, b: 0.2, h: 0.3 }]],
      elements: [
        [1, { id: 1, type, nodeI: 1, nodeJ: 3, materialId, sectionId: 1, releaseI: rel, releaseJ: rel }],
        [2, { id: 2, type, nodeI: 3, nodeJ: 2, materialId, sectionId: 1, releaseI: rel, releaseJ: rel }],
        [3, { id: 3, type, nodeI: 1, nodeJ: 2, materialId, sectionId: 1, releaseI: rel, releaseJ: rel }],
      ],
      supports: [[1, { id: 1, nodeId: 1, type: 'pinned3d' }], [2, { id: 2, nodeId: 2, type: 'pinned3d' }]],
      loads: [{ type: 'nodal3d', data: { id: 1, nodeId: 3, fx: 0, fy: 0, fz: -20, mx: 0, my: 0, mz: 0, caseId: 1 } }],
      loadCases: [{ id: 1, type: 'D', name: 'Dead Load' }],
      combinations: [],
      nextId: { node: 4, material: 3, section: 2, element: 4, support: 3, load: 2, loadCase: 2, combination: 1 },
    },
  };
}

const recheck = (page: Page) => page.evaluate(() => window.__stabileo.recheck());

test.describe('@smoke re-check prompt for old projects', () => {
  test('a steel project lists the CIRSOC 301 tension change; the notice holds until acknowledged', async ({ pro: page }) => {
    expect(await page.evaluate((f) => window.__stabileoActions.loadProject(f), oldProject(1, 'truss'))).toBe(true);
    const notice = page.getByTestId('recheck-notice');
    await expect(notice).toBeVisible();
    await expect(notice.locator('[data-change-id="cirsoc301-tension-needs-connection"]')).toBeVisible();
    await expect(notice.locator('[data-change-id="cirsoc201-strain-compatibility"]')).toHaveCount(0);

    // Escape does not dismiss it, and neither button works before the box is ticked.
    await page.keyboard.press('Escape');
    await expect(notice).toBeVisible();
    await expect(page.getByTestId('recheck-later')).toBeDisabled();
    await expect(page.getByTestId('recheck-rerun')).toBeDisabled();

    await page.getByTestId('recheck-ack').check();
    await page.getByTestId('recheck-later').click();
    await expect(notice).toBeHidden();
    // Acknowledged, not re-run: still stale, and the banner says so.
    await expect(page.getByTestId('recheck-stale-banner')).toBeVisible();
    expect((await recheck(page)).stale).toBe(true);
    expect((await page.evaluate(() => window.__stabileo.resultProvenance()) as { stale: boolean }).stale).toBe(true);

    await page.getByTestId('recheck-banner-rerun').click();
    await expect(page.getByTestId('recheck-stale-banner')).toBeHidden({ timeout: 60_000 });
    const r = await recheck(page);
    expect(r.stale).toBe(false);
    expect(r.stampVersion).toBeGreaterThan(0);
  });

  test('a concrete project is told about CIRSOC 201, not about steel tension', async ({ pro: page }) => {
    expect(await page.evaluate((f) => window.__stabileoActions.loadProject(f), oldProject(2, 'frame'))).toBe(true);
    const notice = page.getByTestId('recheck-notice');
    await expect(notice).toBeVisible();
    await expect(notice.locator('[data-change-id="cirsoc201-strain-compatibility"]')).toBeVisible();
    await expect(notice.locator('[data-change-id="cirsoc301-tension-needs-connection"]')).toHaveCount(0);
    // Re-run from the notice itself.
    await page.getByTestId('recheck-ack').check();
    await page.getByTestId('recheck-rerun').click();
    await expect(notice).toBeHidden();
    await expect.poll(async () => (await recheck(page)).stale, { timeout: 60_000 }).toBe(false);
  });

  test('a project stamped with this version opens silently', async ({ pro: page }) => {
    const f = oldProject(1, 'truss');
    const stamp = await page.evaluate(() => window.__stabileo.recheck().stampVersion);
    (f.snapshot as Record<string, unknown>).solvedWith = { id: 'e2e', resultsVersion: stamp, analyses: ['linear'] };
    expect(await page.evaluate((x) => window.__stabileoActions.loadProject(x), f)).toBe(true);
    await expect(page.getByTestId('recheck-notice')).toHaveCount(0);
    expect((await recheck(page)).noticeOpen).toBe(false);
  });
});

test.describe('@smoke reproducible results', () => {
  test('worker and main thread, and two solves, give the same bits; reproduce confirms it', async ({ pro: page }) => {
    await page.evaluate(() => window.__stabileoActions.loadExample('3d-portal-frame'));
    await page.evaluate(() => window.__stabileoActions.solve());
    const worker = await page.evaluate(() => (window.__stabileo.resultProvenance() as { resultsHash: string | null }).resultsHash);
    expect(worker).toMatch(/^[0-9a-f]{64}$/);
    const main = await page.evaluate(() => window.__stabileoActions.solveOnMainThread());
    expect(main).toBe(worker);
    await page.evaluate(() => window.__stabileoActions.solve());
    const again = await page.evaluate(() => (window.__stabileo.resultProvenance() as { resultsHash: string | null }).resultsHash);
    expect(again).toBe(worker);

    const record = await page.evaluate(() => JSON.stringify(window.__stabileo.resultProvenance()));
    await page.evaluate(() => window.dispatchEvent(new Event('stabileo-reproduce')));
    await page.getByTestId('reproduce-input').fill(record);
    await page.getByTestId('reproduce-run').click();
    await expect(page.getByTestId('reproduce-verdict')).toHaveAttribute('data-reproduced', 'true', { timeout: 60_000 });
  });
});
