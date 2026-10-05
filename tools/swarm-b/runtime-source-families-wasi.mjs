// SPDX-License-Identifier: MPL-2.0
// The source-family executable reads only its three pinned paths under /source.
import { readFile, stat } from 'node:fs/promises';
import { resolve } from 'node:path';
import { WASI } from 'node:wasi';

const [wasmPath, sourceRoot] = process.argv.slice(2);
if (!wasmPath || !sourceRoot) throw new Error('usage: node runtime-source-families-wasi.mjs PROGRAM.wasm REPOSITORY_ROOT');
const metadata = await stat(wasmPath);
if (!metadata.isFile() || metadata.size > 64 * 1024 * 1024) throw new Error('WASI program must be a bounded regular file');
const wasi = new WASI({
  version: 'preview1', args: ['source-families', '/source'], env: {},
  preopens: { '/source': resolve(sourceRoot) }, returnOnExit: true,
});
const module = await WebAssembly.compile(await readFile(wasmPath));
if (WebAssembly.Module.imports(module).some(item => item.module !== 'wasi_snapshot_preview1')) {
  throw new Error('unexpected non-WASI host import');
}
const instance = await WebAssembly.instantiate(module, { wasi_snapshot_preview1: wasi.wasiImport });
const code = wasi.start(instance);
if (code !== 0) process.exitCode = code;
