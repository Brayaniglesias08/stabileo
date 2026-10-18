/**
 * Static server for the determinism page: /determinism/* from web/determinism, /wasm/* from
 * web/src/lib/wasm, /repo/<fixture> for the fixture files listed in cases.mjs only.
 * `node determinism/serve.mjs <port>`.
 */
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, extname } from 'node:path';
import { FIXTURES } from './cases.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const roots = { determinism: here, wasm: resolve(here, '../src/lib/wasm') };
const repo = resolve(here, '../..');
const fixturePaths = new Set(Object.values(FIXTURES));
const types = { '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json' };
const port = Number(process.argv[2] ?? 5195);

createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  let file = null;
  if (path === '/') file = resolve(here, 'index.html');
  else if (path.startsWith('/repo/') && fixturePaths.has(path.slice(6))) file = resolve(repo, path.slice(6));
  else {
    const [, top, ...rest] = path.split('/');
    const rel = rest.join('/');
    if (roots[top] && rel && !rel.split('/').includes('..')) file = resolve(roots[top], rel);
  }
  try {
    if (!file) throw new Error('not found');
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': types[extname(file)] ?? 'application/octet-stream', 'cache-control': 'no-store' });
    res.end(body);
  } catch {
    res.writeHead(404);
    res.end('not found');
  }
}).listen(port, '127.0.0.1', () => console.log(`determinism server on http://127.0.0.1:${port}`));
