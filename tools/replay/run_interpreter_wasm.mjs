// Execute the real wasm32-unknown-unknown interpreter witness in a fresh V8 worker.
// No WASI or host imports are supplied. This is execution, not a rendered UI test.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, existsSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads';

const digest = value => createHash('sha256').update(value).digest('hex');
if (!isMainThread) {
  const bytes = workerData;
  const module = new WebAssembly.Module(bytes);
  assert.deepEqual(WebAssembly.Module.imports(module), [], 'unexpected WASM imports');
  const instance = new WebAssembly.Instance(module, {});
  const e = instance.exports;
  assert(e.memory instanceof WebAssembly.Memory, 'missing WASM memory');
  for (const key of ['interpreter_ptr', 'interpreter_len', 'interpreter_hash_ptr', 'interpreter_hash_len']) {
    assert.equal(typeof e[key], 'function', `missing WASM export ${key}`);
  }
  function take(pointer, length) {
    const p = pointer(), n = length();
    assert(Number.isInteger(p) && Number.isInteger(n) && p >= 0 && n > 0 && n <= 1024*1024, 'invalid buffer bounds');
    assert(e.memory.buffer.byteLength <= 256*1024*1024, 'observed WASM memory limit');
    assert(p + n <= e.memory.buffer.byteLength, 'buffer exceeds WASM memory');
    const value = Buffer.from(new Uint8Array(e.memory.buffer, p, n));
    assert.equal(pointer(), p, 'unstable pointer');
    assert.equal(length(), n, 'unstable length');
    assert.deepEqual(Buffer.from(new Uint8Array(e.memory.buffer,p,n)), value, 'mutating evidence buffer');
    return value;
  }
  const trace = take(e.interpreter_ptr, e.interpreter_len);
  const hashes = take(e.interpreter_hash_ptr, e.interpreter_hash_len);
  parentPort.postMessage({
    trace, hashes, memoryBytes: e.memory.buffer.byteLength,
    exports: WebAssembly.Module.exports(module).map(x => x.name),
  });
} else {
  try {
    assert.equal(process.argv.length, 5, 'supply wasm, new trace and new hash output paths');
    const [wasm, tracePath, hashPath] = process.argv.slice(2);
    assert(!existsSync(tracePath) && !existsSync(hashPath), 'refusing to overwrite evidence');
    assert(statSync(wasm).size <= 64*1024*1024, 'WASM file size limit');
    const bytes = readFileSync(wasm);
    const worker = new Worker(new URL(import.meta.url), {
      workerData: bytes, resourceLimits: { maxOldGenerationSizeMb: 64, stackSizeMb: 8 },
    });
    // JS heap limits do NOT bound WebAssembly linear memory; the observed
    // memory check is explicit above. The parent enforces a wall-clock limit.
    const result = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => { worker.terminate(); reject(new Error('WASM deadline')); }, 20000);
      worker.once('message', message => { clearTimeout(timer); resolve(message); });
      worker.once('error', error => { clearTimeout(timer); reject(error); });
      worker.once('exit', code => { clearTimeout(timer); if (code !== 0) reject(new Error(`WASM worker exit ${code}`)); });
    });
    await worker.terminate();
    const trace = Buffer.from(result.trace), hashes = Buffer.from(result.hashes);
    writeFileSync(tracePath, trace, { flag: 'wx' });
    writeFileSync(hashPath, hashes, { flag: 'wx' });
    console.log(JSON.stringify({
      engine: 'node-v8-webassembly', node: process.version, v8: process.versions.v8,
      imports: [], exports: result.exports, memory_bytes: result.memoryBytes,
      wasm_sha256: digest(bytes), trace_sha256: digest(trace), state_hashes_sha256: digest(hashes),
    }));
  } catch (error) {
    console.error(String(error));
    process.exitCode = 1;
  }
}
