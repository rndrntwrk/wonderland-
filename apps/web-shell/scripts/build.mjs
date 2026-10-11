import { spawnSync } from 'node:child_process';

const childEnv = { ...process.env };
if (childEnv.NO_COLOR && childEnv.NO_COLOR !== 'false') childEnv.NO_COLOR = 'true';
const result = spawnSync('trunk', ['build', '--release', '--locked'], {
  cwd: new URL('..', import.meta.url),
  stdio: 'inherit',
  env: childEnv,
});
if (result.error) console.error(`Could not build the Rust/WASM app: ${result.error.message}`);
process.exitCode = result.status ?? 1;
