/**
 * The discretization-error estimate in the real app (docs/verification/DISCRETIZATION.md):
 * a shell model solved from the ribbon shows "discretization error: estimated ~Y% (… not
 * certified)" next to the certificate, and the convergence check re-solves and fills its table.
 *
 * Both run through the worker pool in a real browser: the estimate exports once failed there
 * ("Worker certificate … not available") while every unit test, which runs on the main thread,
 * passed.
 */
import { test, expect, type Page } from '@playwright/test';

async function boot(page: Page): Promise<void> {
  await page.goto('/app/basic?e2e=1');
  await page.waitForFunction(() => !!window.__stabileoActions, null, { timeout: 60_000 });
  await expect.poll(() => page.evaluate(() => window.__stabileo.solverReady()), { timeout: 60_000 }).toBe(true);
}

test('a shell model shows its estimated discretization error and runs the convergence check', async ({ page }) => {
  test.setTimeout(240_000);
  await boot(page);
  await page.evaluate((n) => window.__stabileoActions.loadExample(n), 'deep-beam-shell');
  await expect.poll(() => page.evaluate(() => window.__stabileo.elementIds().length + 1)).toBeGreaterThan(0);
  const before = await page.evaluate(() => window.__stabileo.solveCount());
  await page.getByTestId('rb-cmd-solve').click();
  await expect.poll(() => page.evaluate(() => window.__stabileo.solveCount()), { timeout: 60_000 }).toBeGreaterThan(before);

  const status = page.getByTestId('discretization-status').first();
  await expect(status).toBeVisible({ timeout: 60_000 });
  // The estimate itself, not "not estimated (Worker certificate … not available)".
  const estimate = page.getByTestId('discretization-estimate').first();
  await expect(estimate).toBeVisible({ timeout: 60_000 });
  await expect(estimate).toContainText('%');
  await expect(estimate).toContainText(/certif/i);
  await expect(status).not.toContainText(/not available/i);

  await page.getByTestId('discretization-run-study').first().click();
  const table = page.getByTestId('discretization-study').first();
  await expect(table).toBeVisible({ timeout: 180_000 });
  await expect(table.locator('tbody tr').first()).toBeVisible();
  await expect(status).not.toContainText(/not available|not run/i);
});
