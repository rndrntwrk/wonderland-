// SPDX-License-Identifier: MPL-2.0
// Test-only wasm32-wasip1 runner. No guest directory or network capability.
import { readFile } from 'node:fs/promises';
import { WASI } from 'node:wasi';
const path = process.argv[2];
if (!path) throw new Error('usage: node run-wasi.mjs program.wasm');
const wasi = new WASI({ version: 'preview1', args: [], env: {}, preopens: {}, returnOnExit: true });
const module = await WebAssembly.compile(await readFile(path));
const imports = WebAssembly.Module.imports(module);
if (imports.some(i => i.module !== 'wasi_snapshot_preview1')) {
  throw new Error('unexpected non-WASI host import');
}
const instance = await WebAssembly.instantiate(module, { wasi_snapshot_preview1: wasi.wasiImport });
const code = wasi.start(instance);
if (code !== 0) process.exitCode = code;
