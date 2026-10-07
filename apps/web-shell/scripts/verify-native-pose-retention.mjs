#!/usr/bin/env node
// Execute an ordinary, import-free WASM probe and compare COMPLETE output bytes.
// This is runtime/geometry conformance, not rendered-browser or original-art QA.
import assert from 'node:assert/strict';
import {readFile, stat, writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';

const MAX_RECORD = 4 * 1024 * 1024, MAX_MODULE = 32 * 1024 * 1024;
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
async function bounded(path, maximum) {
  const info = await stat(path);
  assert.ok(info.isFile() && info.size > 0 && info.size <= maximum, `Invalid input size/type: ${path}`);
  const bytes = await readFile(path);
  assert.ok(bytes.length <= maximum, 'Input grew past its budget');
  return bytes;
}
function validate(text) {
  const data = JSON.parse(text), result = data.result;
  assert.equal(data.schema, 1);
  assert.equal(data.scope, 'synthetic original-format geometry with actual native accepted ticks');
  assert.deepEqual(data.equal_batch_sizes, [1, 3, 18]);
  assert.equal(result.tick, 19);
  assert.equal(result.duplicate_frames, 0);
  assert.equal(result.repeat_draws, 120);
  assert.match(result.server_hash, /^[0-9a-f]{64}$/);
  assert.match(result.retained_sha256, /^[0-9a-f]{64}$/);
  assert.equal(result.trace.length, 18);
  for (const [index, frame] of result.trace.entries()) {
    assert.equal(frame.tick, index + 2);
    assert.equal(frame.avatars.length, 1);
    const avatar = frame.avatars[0], layer = avatar.animations.layers[0];
    assert.deepEqual(avatar.entity, {generation: 1, object_id: 1});
    assert.equal(avatar.guid, 0x7FD96B54);
    assert.equal(avatar.animations.layers.length, 1);
    assert.equal(layer.resource, 'base.anim');
    assert.equal(layer.current_frame, Math.min((index + 1) / 4, 2));
    assert.equal(layer.end_reached, index >= 7);
  }
  const retained = result.retained_model, reset = result.checkpoint_reset_model;
  for (const model of [retained, reset]) {
    assert.equal(model.context.kind, 'vitaboy');
    assert.equal(model.groups.length, 1);
    assert.equal(model.groups[0].length, 4);
  }
  // The fixture's final translated bone retains x=-4 in source coordinates.
  // Recovery without a bone checkpoint intentionally starts again at bind x=0.
  for (let part = 0; part < 4; part++) {
    const a = retained.groups[0][part].mesh, b = reset.groups[0][part].mesh;
    assert.deepEqual(a.indices, [0, 1, 2]);
    assert.deepEqual(b.indices, a.indices);
    assert.equal(a.vertices.length, 3);
    assert.equal(b.vertices.length, 3);
    for (let vertex = 0; vertex < 3; vertex++) {
      assert.equal(a.vertices[vertex].position.x, b.vertices[vertex].position.x - 4);
      assert.equal(a.vertices[vertex].position.y, b.vertices[vertex].position.y);
      assert.equal(a.vertices[vertex].position.z, b.vertices[vertex].position.z);
      assert.deepEqual(a.vertices[vertex].normal, b.vertices[vertex].normal);
      assert.deepEqual(a.vertices[vertex].uv, b.vertices[vertex].uv);
    }
  }
  assert.deepEqual(reset.groups[0][0].mesh.vertices.map(v => v.position), [
    {x: 0.5, y: 0, z: 0}, {x: -0.5, y: 0, z: 0}, {x: 0, y: 4, z: 0},
  ]);
  return data;
}
function compare(native, wasm) {
  validate(native); validate(wasm);
  assert.equal(wasm, native, 'Native/WASM complete output bytes differ');
}
const [nativePath, wasmPath, outputPath, ...extra] = process.argv.slice(2);
assert.ok(nativePath && wasmPath && !extra.length,
  'Usage: verify-native-pose-retention.mjs native.json probe.wasm [wasm.json]');
const decoder = new TextDecoder('utf-8', {fatal: true});
const native = decoder.decode(await bounded(nativePath, MAX_RECORD)).replace(/\r?\n$/, '');
validate(native);
const moduleBytes = await bounded(wasmPath, MAX_MODULE);
const module = await WebAssembly.compile(moduleBytes);
assert.deepEqual(WebAssembly.Module.imports(module), [], 'Unexpected host imports');
const {exports} = await WebAssembly.instantiate(module, {});
assert.ok(exports.memory instanceof WebAssembly.Memory);
assert.equal(typeof exports.wonderland_pose_probe_ptr, 'function');
assert.equal(typeof exports.wonderland_pose_probe_len, 'function');
const start = exports.wonderland_pose_probe_ptr() >>> 0;
const size = exports.wonderland_pose_probe_len() >>> 0;
assert.ok(size > 0 && size <= MAX_RECORD);
assert.ok(start <= exports.memory.buffer.byteLength && size <= exports.memory.buffer.byteLength - start);
assert.ok(!(exports.memory.buffer instanceof SharedArrayBuffer));
const wasm = decoder.decode(new Uint8Array(exports.memory.buffer, start, size));
compare(native, wasm);
assert.equal(exports.wonderland_pose_probe_ptr() >>> 0, start);
assert.equal(exports.wonderland_pose_probe_len() >>> 0, size);
assert.equal(decoder.decode(new Uint8Array(exports.memory.buffer, start, size)), wasm);
const mutations = [
  record => {record.result.trace.splice(4, 1);},
  record => {record.result.retained_model = record.result.checkpoint_reset_model;},
  record => {record.result.trace.at(-1).avatars[0].entity.generation++;},
  record => {record.result.trace.at(-1).avatars[0].animations.layers[0].end_reached = false;},
  record => {record.result.duplicate_frames = 1;},
  record => {record.result.checkpoint_reset_model = record.result.retained_model;},
  record => {record.equal_batch_sizes = [1, 3];},
];
for (const mutation of mutations) {
  const record = JSON.parse(native); mutation(record);
  const bad = JSON.stringify(record);
  assert.throws(() => compare(bad, bad), 'Equal-but-wrong result was admitted');
}
const different = JSON.parse(native);
different.result.server_hash = '00'.repeat(32);
assert.throws(() => compare(native, JSON.stringify(different)), 'Changed authority hash was admitted');
if (outputPath) await writeFile(outputPath, wasm + '\n', {flag: 'wx'});
console.log(JSON.stringify({
  result: 'pass', complete_record_bytes: size, output_sha256: digest(Buffer.from(wasm)),
  wasm_sha256: digest(moduleBytes), native_wasm_equal: true, wasm_imports: 0,
  equal_batch_sizes: [1, 3, 18], accepted_avatar_frames: 18, repeated_draws: 120,
  semantic_negative_controls: mutations.length, changed_output_negative_controls: 1,
  shared_memory: false, browser_interaction_test: false,
}, null, 2));
