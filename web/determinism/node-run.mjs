/**
 * Run the determinism cases in Node (the reference runtime) and print one SHA-256 per case
 * plus an overall digest. `node determinism/node-run.mjs [--json out.json]` from web/.
 * Needs the WASM build in src/lib/wasm (`npm run wasm`).
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { FIXTURES, runCases } from './cases.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
export const repoRoot = resolve(here, '../..');
export const wasmDir = resolve(here, '../src/lib/wasm');

export function loadFixtures() {
  return Object.fromEntries(Object.entries(FIXTURES).map(([k, p]) => [k, JSON.parse(readFileSync(resolve(repoRoot, p), 'utf8'))]));
}

export const sha256 = (s) => createHash('sha256').update(s).digest('hex');

/** Digest of a whole run: the case names and per-case digests, in order. */
export function digestOf(results) {
  return sha256(results.map((r) => `${r.name} ${sha256(r.text)}`).join('\n'));
}

export async function runInNode({ values = false } = {}) {
  const wasm = await import(resolve(wasmDir, 'dedaliano_engine.js'));
  wasm.initSync({ module: readFileSync(resolve(wasmDir, 'dedaliano_engine_bg.wasm')) });
  return runCases(wasm, loadFixtures(), { values });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const results = await runInNode();
  for (const r of results) console.log(`${sha256(r.text).slice(0, 16)}  ${r.name}${r.text.startsWith('ERR') ? `  ${r.text.slice(0, 120)}` : ''}`);
  console.log(`${results.length} cases, ${results.filter((r) => r.text.startsWith('ERR')).length} errors, digest ${digestOf(results)}`);
  const i = process.argv.indexOf('--json');
  if (i > 0) writeFileSync(process.argv[i + 1], JSON.stringify(results.map((r) => ({ name: r.name, sha256: sha256(r.text) })), null, 1));
}
