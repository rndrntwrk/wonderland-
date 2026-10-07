// Execute, do not emulate, the ordinary shipping-library WASM placement probe.
import assert from 'node:assert/strict';
import {readFile, stat, writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {Worker, isMainThread, parentPort, workerData} from 'node:worker_threads';
const digest = value => createHash('sha256').update(value).digest('hex');

if (!isMainThread) {
  try {
    const module = await WebAssembly.compile(workerData);
    assert.deepEqual(WebAssembly.Module.imports(module), [], 'unexpected WASM imports');
    const instance = await WebAssembly.instantiate(module, {});
    const {memory, placement_ptr: pointer, placement_len: length} = instance.exports;
    assert.ok(memory instanceof WebAssembly.Memory, 'missing WASM memory');
    assert.equal(typeof pointer, 'function', 'missing placement pointer');
    assert.equal(typeof length, 'function', 'missing placement length');
    const start = pointer() >>> 0, size = length() >>> 0;
    assert.ok(!(memory.buffer instanceof SharedArrayBuffer), 'shared WASM memory');
    assert.ok(size > 0 && size <= 1024 * 1024, 'placement output size');
    assert.ok(start <= memory.buffer.byteLength && size <= memory.buffer.byteLength - start, 'placement output bounds');
    const trace = Uint8Array.from(new Uint8Array(memory.buffer, start, size));
    new TextDecoder('utf-8', {fatal: true}).decode(trace);
    assert.equal(pointer() >>> 0, start); assert.equal(length() >>> 0, size);
    assert.deepEqual(new Uint8Array(memory.buffer, start, size), trace, 'repeated reads changed state');
    parentPort.postMessage({trace, exports: WebAssembly.Module.exports(module).map(e => e.name)});
  } catch (error) { throw new Error('Placement WASM execution failed: ' + error.message); }
} else {
  const [path, output, ...extra] = process.argv.slice(2);
  assert.ok(path && output && extra.length === 0, 'Usage: run_placement_wasm.mjs module.wasm trace.tsv');
  const info = await stat(path);
  assert.ok(info.isFile() && info.size > 0 && info.size <= 32 * 1024 * 1024, 'WASM file size');
  const bytes = await readFile(path);
  assert.ok(bytes.length <= 32 * 1024 * 1024, 'WASM file grew beyond limit');
  const worker = new Worker(new URL(import.meta.url), {workerData: bytes});
  const result = await new Promise((resolve, reject) => {
    let received = false;
    const timer = setTimeout(() => {void worker.terminate(); reject(Error('Placement WASM deadline exceeded'));}, 15000);
    worker.once('message', value => {received = true; clearTimeout(timer); resolve(value);});
    worker.once('error', error => {clearTimeout(timer); reject(error);});
    worker.once('exit', code => {clearTimeout(timer); if (!received || code !== 0) reject(Error('Placement worker exited without valid completion: ' + code));});
  });
  await worker.terminate();
  await writeFile(output, result.trace, {flag: 'wx'});
  console.log(JSON.stringify({schema: 1, node: process.version, v8: process.versions.v8,
    wasm_sha256: digest(bytes), trace_sha256: digest(result.trace), trace_bytes: result.trace.length,
    imports: [], exports: result.exports, shared_memory: false, repeated_reads: 'identical'}, null, 2));
}
