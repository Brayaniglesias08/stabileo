/**
 * What Básico hands out: the workbook, the CSV, the figures and the calculation report.
 *
 * The drawing and the report's HTML are covered in unit tests (`lib/export/__tests__`,
 * `engine/__tests__/calc-report-content.test.ts`). What those cannot reach is the path from the
 * buttons to a file: that an export asked for before Calcular solves first, that one asked for
 * on a model that cannot be solved still comes out and says why, and that the figure dialog and
 * the report dialog produce what they offer.
 */
import { test, expect } from './fixtures';
import * as XLSX from 'xlsx';
import { readFileSync } from 'node:fs';

type Page = import('@playwright/test').Page;

async function openProject(page: Page) {
  await page.goto('/app/basic?e2e=1');
  await page.waitForFunction(() => !!window.__stabileo, null, { timeout: 60_000 });
  const panel = page.locator('[data-testid="basic-panel"][data-panel="project"]');
  if (await panel.count() === 0) await page.getByTestId('hdr-project').click();
  await expect(panel).toBeVisible();
}

async function download(page: Page, click: () => Promise<void>): Promise<{ name: string; path: string }> {
  const [d] = await Promise.all([page.waitForEvent('download'), click()]);
  return { name: d.suggestedFilename(), path: (await d.path())! };
}

function sheets(path: string): Record<string, string> {
  const wb = XLSX.read(readFileSync(path));
  return Object.fromEntries(wb.SheetNames.map((n) => [n, XLSX.utils.sheet_to_csv(wb.Sheets[n]!)]));
}

test.describe('Básico exports', () => {
  test('a workbook asked for before solving solves first; one that cannot solve says why', async ({ page }) => {
    test.setTimeout(120_000);
    await openProject(page);

    // Nothing to solve yet: the workbook still comes out, and its results sheets send the reader
    // to the summary, which carries the reason.
    let wb = sheets((await download(page, () => page.getByTestId('export-excel').click())).path);
    const names = Object.keys(wb);
    expect(wb[names[0]!]).toMatch(/could not be solved/i);
    expect(Object.values(wb).join('\n')).toMatch(/fix the error described in the Summary sheet/i);

    await page.evaluate(() => window.__stabileoActions.loadExample('portal-frame'));
    wb = sheets((await download(page, () => page.getByTestId('export-excel').click())).path);
    const reactions = Object.entries(wb).find(([n]) => /reaction/i.test(n))?.[1] ?? '';
    // Two fixed bases, with numbers: solved by the export itself.
    expect(reactions.split('\n').filter((l) => /^\d+,/.test(l))).toHaveLength(2);

    const csv = readFileSync((await download(page, () => page.getByTestId('export-csv').click())).path, 'utf8');
    expect(csv.split('\n')[0]).toMatch(/^# Stabileo/);
  });

  test('the view dialog makes vector figures of what is chosen, and there is no DXF', async ({ page }) => {
    test.setTimeout(120_000);
    await openProject(page);
    await page.evaluate(() => window.__stabileoActions.loadExample('portal-frame'));
    await expect(page.locator('[data-testid="basic-panel"]').getByRole('button', { name: 'DXF', exact: true })).toHaveCount(0);

    await page.getByTestId('export-svg').click();
    const dialog = page.getByTestId('view-export-dialog');
    await expect(dialog).toBeVisible();
    await page.getByTestId('view-export-content').locator('summary').click();
    await page.getByTestId('view-export-despiece').check();
    await page.getByTestId('view-export-moment').check();
    const svg = await download(page, () => page.getByTestId('view-export-download').click());
    expect(svg.name).toMatch(/\.svg$/);
    const text = readFileSync(svg.path, 'utf8');
    expect(text).toContain('<svg');
    // Drawn, not captured: paths and text, no raster.
    expect(text).toContain('<path');
    expect(text).toMatch(/kN·m/);
    expect(text).not.toContain('<image');
    await expect(dialog).toBeHidden();

    await page.getByTestId('export-png').click();
    const png = await download(page, () => page.getByTestId('view-export-download').click());
    expect(png.name).toMatch(/\.png$/);
    expect(readFileSync(png.path).subarray(1, 4).toString()).toBe('PNG');
  });

  test('the report carries the chosen parts: kinematics, free body and diagrams', async ({ page }) => {
    test.setTimeout(120_000);
    await openProject(page);
    await page.evaluate(() => window.__stabileoActions.loadExample('portal-frame'));
    await page.getByTestId('basic-panel').getByRole('button', { name: 'PDF', exact: true }).click();
    await expect(page.getByTestId('calc-report-dialog')).toBeVisible();
    // The members' dimensions, asked for in Figures.
    await page.getByTestId('calc-report-figures').locator('summary').click();
    await page.getByTestId('calc-report-lengths').check();
    const [report] = await Promise.all([page.waitForEvent('popup'), page.getByTestId('calc-report-generate').click()]);
    await report.waitForLoadState();
    const lengthTexts = await report.locator('figure.result-fig svg text').evaluateAll((ts) => ts.filter((x) => /^\d+\.\d{2} m$/.test(x.textContent ?? '')).length);
    expect(lengthTexts).toBeGreaterThan(0);
    await expect(report.locator('h1#sec-kinematics')).toBeVisible();
    await expect(report.locator('h1#sec-despiece')).toBeVisible();
    // The model figure, then the loaded structure, the free body, the deformed shape, M, V and N.
    await expect(report.locator('figure.result-fig svg')).toHaveCount(6);
    await expect(report.locator('h1#sec-forces')).toBeVisible();
  });
});
