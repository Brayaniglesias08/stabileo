/**
 * The WASM engine gives bit-identical results in every runtime it ships to
 * (docs/verification/TRUSTED_BASE.md, "Determinism across runtimes"): a fixed set of solves
 * and certificates (cases.mjs) runs in this browser and in Node, and every output must be
 * the same string, floats compared by bit pattern. Run with
 * `npx playwright test -c determinism/playwright.config.ts` from web/.
 */
import { test, expect } from '@playwright/test';
// @ts-expect-error plain ES module shared with the browser page
import { runInNode, sha256, digestOf } from './node-run.mjs';

type Result = { name: string; text: string };

/** Certificates and solves that must succeed on these models (not only agree). */
const MUST_SUCCEED = [
  /\/certify_linear_[23]d$/, /\/export_linear_bundle_[23]d$/, /\/certify_modal_[23]d$/,
  /^combo-2d\//, /^bundle-3d\//, /^plastic-/, /\/solve_[23]d$/,
];

/** Certificates that must come out certified on these models, so agreement is not vacuous. */
const MUST_CERTIFY = [
  /\/certify_(linear|modal|spectral|pdelta|multi_case|time_history)_[23]d$/,
  /^(portal|two-story|truss)-2d\/certify_buckling_2d$/, /-3d\/certify_buckling_3d$/,
  /^combo-2d\/certify_/, /^bundle-3d\/certify_/, /^plastic-.*\/certify_/,
];
const CERTIFIED = /"status":"certified"|"certified":true|"stabilityCertified":true/;

let reference: Result[];
test.beforeAll(async () => {
  reference = await runInNode();
});

test('engine results are bit-identical to Node', async ({ page, browserName }) => {
  test.setTimeout(300_000);
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { __determinism?: unknown }).__determinism, null, { timeout: 280_000 });
  const run = await page.evaluate(() => (window as unknown as { __determinism: { ok: boolean; error?: string; results?: Result[] } }).__determinism);
  expect(run.ok, run.error).toBe(true);
  const results = run.results!;

  const errors = reference.filter((r) => r.text.startsWith('ERR') && MUST_SUCCEED.some((re) => re.test(r.name)));
  expect(errors.map((r) => `${r.name}: ${r.text.slice(0, 200)}`)).toEqual([]);

  const uncertified = reference.filter((r) => MUST_CERTIFY.some((re) => re.test(r.name)) && !CERTIFIED.test(r.text));
  expect(uncertified.map((r) => r.name)).toEqual([]);

  expect(results.map((r) => r.name)).toEqual(reference.map((r) => r.name));
  const diffs = results
    .map((r, i) => ({ name: r.name, browser: sha256(r.text), node: sha256(reference[i].text), at: firstDiff(r.text, reference[i].text) }))
    .filter((d) => d.browser !== d.node);
  console.log(`${browserName}: ${results.length} cases, digest ${digestOf(results)} (node ${digestOf(reference)})`);
  expect(diffs, `${browserName} differs from Node`).toEqual([]);
});

function firstDiff(a: string, b: string): string {
  let i = 0;
  while (i < a.length && a[i] === b[i]) i++;
  return `@${i}: ${a.slice(Math.max(0, i - 40), i + 40)} | ${b.slice(Math.max(0, i - 40), i + 40)}`;
}
