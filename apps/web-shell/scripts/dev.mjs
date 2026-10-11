// Node starts Trunk or serves its output. Application rendering and behavior are Rust/WASM.
import { spawn, spawnSync } from 'node:child_process';
import { serveBuiltWasm } from './serve-built.mjs';

let host = '0.0.0.0';
let port = '4173';
const args = process.argv.slice(2);
for (let index = 0; index < args.length; index += 1) {
  const arg = args[index];
  if (arg === '--host' && args[index + 1]) host = args[++index];
  else if (arg === '--port' && args[index + 1]) port = args[++index];
  else if (arg !== '--strictPort') {
    console.error(`Unknown or incomplete development option: ${arg}`);
    process.exit(2);
  }
}
if (!/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65535) {
  console.error('The development port must be an integer from 1 through 65535.');
  process.exit(2);
}
// Trunk's boolean option parser expects "true", while NO_COLOR commonly uses "1".
const childEnv = { ...process.env };
if (childEnv.NO_COLOR && childEnv.NO_COLOR !== 'false') childEnv.NO_COLOR = 'true';
const available = spawnSync('trunk', ['--version'], { env: childEnv, stdio: 'ignore' });
if (available.error?.code === 'ENOENT') {
  // Restricted preview hosts can serve a completed build without a Rust compiler.
  // They never substitute JS state or markup for the actual WASM application.
  serveBuiltWasm({ host, port: Number(port) });
} else {
  const child = spawn('trunk', ['serve', '--address', host, '--port', port, '--locked'], {
    cwd: new URL('..', import.meta.url),
    stdio: 'inherit',
    env: childEnv,
  });
  child.on('error', error => {
    console.error(`Could not start Trunk: ${error.message}. Install the documented Rust toolchain and Trunk first.`);
    process.exitCode = 1;
  });
  child.on('exit', (code, signal) => {
    process.exitCode = code ?? (signal ? 1 : 0);
  });
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => child.kill(signal));
  }
}
