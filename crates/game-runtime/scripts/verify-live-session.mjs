#!/usr/bin/env node
// Execute the ordinary (non-WASI) WASM module and compare the COMPLETE record.
// Usage: node verify-live-session.mjs native.json live_session_probe.wasm [wasm.json]
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, stat, writeFile } from 'node:fs/promises';

const MAX_RECORD = 16 * 1024 * 1024;
const MAX_MODULE = 32 * 1024 * 1024;
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
async function boundedFile(path, limit) {
  const info = await stat(path);
  assert.ok(info.isFile() && info.size > 0 && info.size <= limit, `Invalid input size/type: ${path}`);
  const bytes = await readFile(path);
  assert.ok(bytes.length <= limit, 'Input grew past its limit');
  return bytes;
}
function validateSnapshot(record) {
  assert.ok(Number.isSafeInteger(record.tick) && record.tick >= 0);
  assert.match(record.state_hash, /^[0-9a-f]{64}$/);
  assert.match(record.snapshot_hex, /^(?:[0-9a-f]{2})+$/);
  const bytes = Buffer.from(record.snapshot_hex, 'hex');
  assert.equal(bytes.subarray(0, 8).toString(), 'WLDSNAP\0');
  assert.equal(digest(bytes), record.snapshot_sha256);
  assert.equal(bytes.length, record.snapshot_hex.length / 2);
}
function validate(text) {
  const record = JSON.parse(text);
  assert.equal(record.schema, 1);
  assert.equal(record.protocol, 'native-A-completed-tick');
  assert.equal(record.fixture, 'declared-harness-original-BHAV');
  assert.deepEqual(record.actions.attributes, [0, 0, 30, 0]);
  assert.equal(record.actions.source, 'Casino_2-Tile_Bar_CC.iff/BHAV/4110');
  assert.equal(record.actions.recovered_tick, 64);
  assert.equal(record.actions.atomic_failure_preserved, true);
  assert.equal(record.actions.duplicate_outcomes, 0);
  assert.equal(record.actions.trace.length, 61);
  for (const [index, tick] of record.actions.trace.entries()) {
    assert.equal(tick.tick, index + 3);
    assert.match(tick.hash, /^[0-9a-f]{64}$/);
    assert.equal(typeof tick.events, 'string');
    assert.ok(Number.isSafeInteger(tick.instructions) && tick.instructions >= 0);
  }
  assert.match(record.actions.trace[1].events, /Finished/);
  assert.match(record.actions.trace[1].events, /Succeeded/);
  assert.equal(record.actions.completed_action.tick, 4);
  assert.deepEqual(record.actions.completed_action.queues[0].entries, []);
  assert.equal(record.actions.replica_a.tick, 66);
  assert.deepEqual(record.actions.replica_a, record.actions.replica_b);
  assert.equal(record.menu.source, 'fso_christmas_flag.iff/BHAV/4108');
  assert.deepEqual(record.menu.offers.map(({ label, param0 }) => [label, param0]), [
    ['Debug/Set Team/None', 0], ['Debug/Set Team/Elves', 1], ['Debug/Set Team/Reindeer', 2],
  ]);
  assert.equal(record.menu.invented_parameter_rejected, true);
  assert.equal(record.needs.source, 'cursebook_set_permission.iff/BHAV/4107');
  for (const key of ['energy', 'hunger', 'hygiene']) {
    assert.equal(record.needs.before[key], 50);
    assert.equal(record.needs.after[key], 100);
  }
  assert.equal(record.needs.after.room, null);
  assert.deepEqual(record.needs.final.entities[0].needs, record.needs.after);
  assert.equal(record.needs.final.entities[0].raw_motives[7], 100);
  assert.deepEqual(record.needs.final.queues[0].entries, []);
  for (const snapshot of [record.actions.completed_action, record.actions.replica_a,
    record.actions.replica_b, record.menu.final, record.needs.final]) validateSnapshot(snapshot);
  return record;
}
function compare(native, wasm) {
  validate(native);
  validate(wasm);
  assert.equal(wasm, native, 'Native/WASM complete output bytes differ');
}

const [nativePath, wasmPath, outputPath, ...extra] = process.argv.slice(2);
if (!nativePath || !wasmPath || extra.length) {
  throw new Error('Usage: verify-live-session.mjs native.json live_session_probe.wasm [wasm.json]');
}
const decoder = new TextDecoder('utf-8', { fatal: true });
const nativeBytes = await boundedFile(nativePath, MAX_RECORD);
const native = decoder.decode(nativeBytes).replace(/\r?\n$/, '');
validate(native);
const moduleBytes = await boundedFile(wasmPath, MAX_MODULE);
const module = await WebAssembly.compile(moduleBytes);
assert.deepEqual(WebAssembly.Module.imports(module), [], 'WASM probe unexpectedly needs host imports');
const instance = await WebAssembly.instantiate(module, {});
const { memory, wonderland_live_probe_ptr: pointer, wonderland_live_probe_len: length } = instance.exports;
assert.ok(memory instanceof WebAssembly.Memory);
assert.equal(typeof pointer, 'function');
assert.equal(typeof length, 'function');
const start = pointer() >>> 0;
const size = length() >>> 0;
assert.ok(size > 0 && size <= MAX_RECORD, 'WASM output limit');
assert.ok(start <= memory.buffer.byteLength && size <= memory.buffer.byteLength - start);
assert.ok(!(memory.buffer instanceof SharedArrayBuffer), 'Probe must use ordinary non-shared memory');
const wasm = decoder.decode(new Uint8Array(memory.buffer, start, size));
compare(native, wasm);
// Repeated reads do not execute another scenario or append another event stream.
assert.equal(pointer() >>> 0, start);
assert.equal(length() >>> 0, size);
assert.equal(decoder.decode(new Uint8Array(memory.buffer, start, size)), wasm);

const mutations = [
  value => { value.actions.attributes[0] = 99; },
  value => { value.actions.trace.pop(); },
  value => { value.menu.offers[1].param0 = 37; },
  value => { value.needs.after.hunger = 50; },
  value => { value.actions.recovered_tick = 63; },
  value => { value.actions.replica_a.snapshot_hex = '00' + value.actions.replica_a.snapshot_hex.slice(2); },
];
for (const change of mutations) {
  const bad = JSON.parse(native);
  change(bad);
  const text = JSON.stringify(bad);
  assert.throws(() => compare(text, text), 'Equal-but-wrong output was admitted');
}
const changedEvents = JSON.parse(native);
changedEvents.actions.trace[0].events += 'unrecorded';
assert.throws(() => compare(native, JSON.stringify(changedEvents)), 'Changed ordered events were admitted');
if (outputPath) await writeFile(outputPath, wasm + '\n', { flag: 'wx' });
console.log(JSON.stringify({
  result: 'pass', schema: 1, native_wasm_equal: true, complete_record_bytes: size,
  output_sha256: digest(Buffer.from(wasm)), wasm_sha256: digest(moduleBytes),
  wasm_imports: 0, shared_memory: false, semantic_negative_controls: mutations.length,
  changed_output_negative_controls: 1, repeated_reads: 'equal',
}, null, 2));
