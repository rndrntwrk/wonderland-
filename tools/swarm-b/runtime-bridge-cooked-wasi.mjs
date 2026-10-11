// SPDX-License-Identifier: MPL-2.0
// The real runtime receives only the explicitly preopened cooked release.
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { WASI } from 'node:wasi';

const [wasmPath, releasePath, bindingHash] = process.argv.slice(2);
if (!wasmPath || !releasePath || !/^[0-9a-f]{64}$/.test(bindingHash ?? '')) {
  throw new Error('usage: node runtime-bridge-cooked-wasi.mjs PROGRAM.wasm RELEASE_DIR BINDING_SHA256');
}
const wasi = new WASI({
  version: 'preview1',
  args: ['runtime-bridge', 'release-replay', '/release/binding.json',
    '--binding-sha256', bindingHash, '--release-dir', '/release',
    '--scenario', '/release/scenario.json'],
  env: {},
  preopens: { '/release': resolve(releasePath) },
  returnOnExit: true,
});
const module = await WebAssembly.compile(await readFile(wasmPath));
if (WebAssembly.Module.imports(module).some(item => item.module !== 'wasi_snapshot_preview1')) {
  throw new Error('unexpected non-WASI host import');
}
const instance = await WebAssembly.instantiate(module, { wasi_snapshot_preview1: wasi.wasiImport });
const code = wasi.start(instance);
if (code !== 0) process.exitCode = code;
