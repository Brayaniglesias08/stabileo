/**
 * Static server for the determinism page: /determinism/* from web/determinism, /wasm/* from
 * web/src/lib/wasm, /repo/<fixture> for the fixture files listed in cases.mjs only.
 * `node determinism/serve.mjs <port>`.
 */
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, relative, isAbsolute, extname, sep } from 'node:path';
import { FIXTURES } from './cases.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const roots = { determinism: here, wasm: resolve(here, '../src/lib/wasm') };
const repo = resolve(here, '../..');
const fixturePaths = new Set(Object.values(FIXTURES));
const types = { '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json' };

/**
 * The file a request path maps to, or null. Only the page, the listed fixtures and files
 * strictly inside the two roots are served: an absolute remainder (`/wasm//etc/passwd`),
 * a backslash or NUL, or anything that resolves outside its root is refused.
 */
export function fileFor(urlPath) {
  let path;
  try {
    path = decodeURIComponent(urlPath);
  } catch {
    return null;
  }
  if (path.includes('\0') || path.includes('\\')) return null;
  if (path === '/') return resolve(here, 'index.html');
  if (path.startsWith('/repo/')) return fixturePaths.has(path.slice(6)) ? resolve(repo, path.slice(6)) : null;
  const [, top, ...rest] = path.split('/');
  const rel = rest.join('/');
  if (!Object.hasOwn(roots, top) || !rel || isAbsolute(rel) || rel.startsWith('/')) return null;
  const root = roots[top];
  const file = resolve(root, rel);
  const inside = relative(root, file);
  if (!inside || inside.startsWith('..') || isAbsolute(inside) || inside.split(sep).includes('..')) return null;
  return file;
}

const port = Number(process.argv[2] ?? 5195);

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
createServer(async (req, res) => {
  const file = fileFor(new URL(req.url, 'http://x').pathname);
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
}
