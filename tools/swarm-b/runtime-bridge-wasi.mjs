// SPDX-License-Identifier: MPL-2.0
// Execute the real source probe with one explicitly preopened source directory.
import { readFile } from 'node:fs/promises';
import { basename, dirname, resolve } from 'node:path';
import { WASI } from 'node:wasi';

const [wasmPath, sourcePath] = process.argv.slice(2);
if (!wasmPath || !sourcePath) throw new Error('usage: node runtime-bridge-wasi.mjs PROGRAM.wasm INPUT.iff');
const source = resolve(sourcePath);
const wasi = new WASI({
  version: 'preview1',
  args: ['source-replay', `/source/${basename(source)}`],
  env: {},
  preopens: { '/source': dirname(source) },
  returnOnExit: true,
});
const module = await WebAssembly.compile(await readFile(wasmPath));
if (WebAssembly.Module.imports(module).some(item => item.module !== 'wasi_snapshot_preview1')) {
  throw new Error('unexpected non-WASI host import');
}
const instance = await WebAssembly.instantiate(module, { wasi_snapshot_preview1: wasi.wasiImport });
const code = wasi.start(instance);
if (code !== 0) process.exitCode = code;
