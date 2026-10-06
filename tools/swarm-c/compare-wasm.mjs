import { readFile, writeFile, mkdir } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import path from 'node:path';

const [wasmPath, nativePath, outputPath] = process.argv.slice(2);
assert.ok(wasmPath && nativePath && outputPath, 'Usage: node compare-wasm.mjs probe.wasm native.jsonl report.json');
const bytes = await readFile(wasmPath);
const module = await WebAssembly.compile(bytes);
assert.deepEqual(WebAssembly.Module.imports(module), [], 'Presentation reference must have no host imports');
const instance = await WebAssembly.instantiate(module, {});
assert.equal(instance.exports.memory.buffer instanceof SharedArrayBuffer, false, 'Baseline uses ordinary non-shared WASM memory');
const records = (await readFile(nativePath, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
assert.equal(records.length, 18);
const results = [];
const started = performance.now();
for (const expected of records) {
  const { mode, avatars, tick } = expected;
  assert.equal(instance.exports.c_probe_run(mode, avatars, tick), 0, `scenario ${mode}/${avatars}/${tick}`);
  const view = new Uint8Array(instance.exports.memory.buffer, instance.exports.c_probe_ptr(), instance.exports.c_probe_len());
  const observed = JSON.parse(new TextDecoder().decode(view));
  assert.deepEqual(observed, expected, `native/WASM mismatch in mode=${mode}, avatars=${avatars}, tick=${tick}`);
  results.push(observed);
}
assert.equal(instance.exports.c_probe_run(3, 32, 0), 1, 'invalid mode must reject without a trap');
assert.equal(instance.exports.c_probe_run(0, 65, 0), 1, 'invalid workload must reject without a trap');
const report = {
  evidence: 'synthetic-native-wasm-algorithms', passed: true, scenarios: results.length,
  imports: 0, sharedMemory: false, wasmSha256: createHash('sha256').update(bytes).digest('hex'),
  elapsedMs: performance.now() - started, memoryBytes: instance.exports.memory.buffer.byteLength,
  results,
};
await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify({ ...report, results: undefined }));
