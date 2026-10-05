import { spawnSync } from 'node:child_process';
import { mkdirSync, copyFileSync, cpSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('..', import.meta.url));
// Resolve configured directories once, independently of the caller's cwd.
const target = resolve(root, process.env.CREATOR_WEB_TARGET_DIR || 'target');
const out = resolve(root, process.env.CREATOR_WEB_DIST_DIR || 'dist');
const cargo = process.env.CREATOR_CARGO || 'cargo';
const wasmBindgen = process.env.CREATOR_WASM_BINDGEN || 'wasm-bindgen';
const release = !process.argv.includes('--debug');
function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', env: { ...process.env, CARGO_INCREMENTAL: '0', CARGO_PROFILE_DEV_DEBUG: '0', CARGO_PROFILE_RELEASE_DEBUG: '0' } });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status || 1);
}
run(cargo, ['build', '--locked', '--target', 'wasm32-unknown-unknown', '--target-dir', target, ...(release ? ['--release'] : [])]);
mkdirSync(resolve(out, 'pkg'), { recursive: true });
run(wasmBindgen, [resolve(target, 'wasm32-unknown-unknown', release ? 'release' : 'debug', 'wonderland_creator_web.wasm'), '--target', 'web', '--out-dir', resolve(out, 'pkg'), '--out-name', 'wonderland_creator_web']);
copyFileSync(resolve(root, 'index.html'), resolve(out, 'index.html'));
for (const name of ['styles.css', 'bootstrap.js']) copyFileSync(resolve(root, 'public', name), resolve(out, name));
cpSync(resolve(root, 'public', 'fonts'), resolve(out, 'fonts'), { recursive: true });
console.log(`Creator web built in ${out}`);
