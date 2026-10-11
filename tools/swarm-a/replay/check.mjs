#!/usr/bin/env node
// No npm packages. The simulation receives no imports and no JS memory pointer.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = fileURLToPath(new URL('../../../', import.meta.url));
function sourceDigest() {
  const files = ['crates/sim-core/Cargo.toml', 'crates/sim-core/Cargo.lock',
    'tools/swarm-a/replay/Cargo.toml', 'tools/swarm-a/replay/Cargo.lock', 'tools/swarm-a/replay/check.mjs'];
  function walk(relative) {
    for (const entry of readdirSync(join(repository, relative), { withFileTypes: true })) {
      const path = `${relative}/${entry.name}`;
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile() && entry.name.endsWith('.rs')) files.push(path);
    }
  }
  walk('crates/sim-core/src');
  walk('tools/swarm-a/replay/src');
  const digest = createHash('sha256');
  for (const path of files.sort()) {
    const bytes = readFileSync(join(repository, path));
    const length = Buffer.alloc(8);
    length.writeBigUInt64LE(BigInt(bytes.length));
    digest.update(path); digest.update(Buffer.from([0])); digest.update(length); digest.update(bytes);
  }
  return digest.digest('hex');
}

if (process.argv.length === 3 && process.argv[2] === '--source-digest') {
  console.log(sourceDigest());
  process.exit(0);
}

const [wasmPath, nativePath, ...options] = process.argv.slice(2);
if (!wasmPath || !nativePath) {
  throw new Error('Usage: node check.mjs MODULE.wasm NATIVE_RECORDS.txt [--source-digest-file BEFORE.txt] [--write-evidence RESULTS.json]');
}
let evidencePath;
let sourceDigestPath;
for (let at = 0; at < options.length; at += 2) {
  assert(options[at + 1], 'option requires a path');
  if (options[at] === '--write-evidence' && !evidencePath) evidencePath = options[at + 1];
  else if (options[at] === '--source-digest-file' && !sourceDigestPath) sourceDigestPath = options[at + 1];
  else throw new Error(`unknown or duplicate option ${options[at]}`);
}
const applicationSourceSha256 = sourceDigest();
if (sourceDigestPath) assert.equal(applicationSourceSha256, readFileSync(sourceDigestPath, 'utf8').trim(), 'application sources changed between the pre-build digest and execution');

const names = ['bhav-stack-sleep-rng', 'avatar-animation-motives', 'effect-fenced-retry', 'portal-route-callback'];
const tickCounts = [96, 181, 24, 64];
const firstTicks = [1n, 1n, 1n, 2n];
const snapshotCounts = [4, 5, 5, 6];
const seeds = [0n, 1n, 123n, 0xfedcba9876543210n];
const freshInstances = 2;
const repetitions = 2;
const maxReportBytes = 16 * 1024 * 1024;
const hash = value => createHash('sha256').update(value).digest();
const sha256 = value => hash(value).toString('hex');

function readReport(bytes) {
  assert(bytes.length <= maxReportBytes, 'report size bound');
  let offset = 0;
  function take(length) {
    assert(Number.isSafeInteger(length) && length >= 0 && length <= bytes.length - offset, 'truncated report');
    const value = bytes.subarray(offset, offset + length);
    offset += length;
    return value;
  }
  const u32 = () => take(4).readUInt32LE();
  const u64 = () => take(8).readBigUInt64LE();
  const i64 = () => take(8).readBigInt64LE();
  const string = () => { const length = u32(); assert(length < 1024); return take(length).toString('utf8'); };
  assert.equal(take(8).toString('ascii'), 'SWARMRP1');
  const scenario = u32();
  assert(scenario < names.length, 'known scenario');
  const seed = u64();
  const tickCount = u32();
  assert.equal(tickCount, tickCounts[scenario], 'all accepted ticks are included');
  const firstTick = firstTicks[scenario];
  const lastTick = firstTick + BigInt(tickCount) - 1n;
  const ticks = [];
  for (let at = 0; at < tickCount; at++) {
    const tick = u64();
    const epoch = u64();
    const rng = u64();
    const instructions = u32();
    const acceptedHash = take(32).toString('hex');
    const stateHash = take(32).toString('hex');
    assert.equal(tick, firstTick + BigInt(at), 'accepted replay ticks are consecutive');
    assert(epoch >= 1n && epoch <= 3n, 'fixture epoch bound');
    ticks.push({ tick: tick.toString(), epoch: epoch.toString(), rng: rng.toString(), instructions, acceptedHash, stateHash });
  }
  const count = u32();
  assert.equal(count, snapshotCounts[scenario], 'all snapshot boundaries are included');
  const snapshots = [];
  for (let at = 0; at < count; at++) {
    const label = string();
    const tick = u64();
    const epoch = u64();
    const stateHash = take(32).toString('hex');
    const length = u32();
    const declaredDigest = take(32).toString('hex');
    const snapshot = take(length);
    assert(length >= 140, 'full snapshot header/checksum is present');
    assert.equal(sha256(snapshot), declaredDigest, `${label}: independent snapshot SHA-256`);
    assert.deepEqual(snapshot.subarray(0, 8), Buffer.from('WLDSNAP\0', 'ascii'), `${label}: snapshot magic`);
    assert.equal(snapshot.readUInt16LE(8), 1, `${label}: snapshot format`);
    assert.equal(snapshot.readBigUInt64LE(12), 0x53415245504c4159n, `${label}: lot ID`);
    assert.equal(snapshot.readBigUInt64LE(20), epoch, `${label}: epoch`);
    assert.equal(snapshot.readBigUInt64LE(28), tick, `${label}: completed tick`);
    const payloadLength = snapshot.readBigUInt64LE(100);
    assert.equal(BigInt(length), 108n + payloadLength + 32n, `${label}: independent payload length`);
    assert.deepEqual(snapshot.subarray(-32), hash(snapshot.subarray(0, -32)), `${label}: embedded corruption checksum`);
    assert.equal(sha256(snapshot.subarray(108, -32)), stateHash, `${label}: canonical payload state hash`);
    assert(tick >= 1n && tick <= lastTick);
    const accepted = tick >= firstTick ? ticks[Number(tick - firstTick)] : undefined;
    if (!accepted) {
      assert.equal(scenario, 3, 'only the portal case begins from normalized initial state');
      assert.equal(tick, 1n);
      assert.equal(epoch, 1n);
      assert.equal(label, 'normalized-initial-world');
    }
    // An authority takeover can create a valid new-epoch snapshot immediately
    // after tick N; that state intentionally differs from tick N's old epoch.
    if (accepted?.epoch === epoch.toString()) assert.equal(accepted.stateHash, stateHash, `${label}: per-tick state hash`);
    snapshots.push({ label, tick: tick.toString(), epoch: epoch.toString(), bytes: length, sha256: declaredDigest, stateHash });
  }
  assert.equal(snapshots.at(-1).label, 'final');
  assert.equal(snapshots.at(-1).stateHash, ticks.at(-1).stateHash);
  const probeCount = u32();
  assert(probeCount >= 4 && probeCount < 64, 'bounded semantic observations');
  const probes = {};
  for (let at = 0; at < probeCount; at++) {
    const name = string();
    assert(!(name in probes), 'unique probe names');
    probes[name] = i64().toString();
  }
  assert.equal(offset, bytes.length, 'report has no trailing data');
  assert(BigInt(probes.post_snapshot_replayed_ticks) > 0n, 'restored tail was replayed');
  assert(BigInt(probes.duplicate_tick_noops) > 0n, 'accepted tick duplicate was checked');
  if (scenario === 0) {
    assert.equal(probes.saved_stack_depth, '3');
    assert.equal(probes.completed_main_calls, '13');
    assert.equal(probes.isolated_rng_queries, '8');
  } else if (scenario === 1) {
    assert.equal(probes.normal_speed_bits, String(0x3f99999a));
    assert.equal(probes.hurry_speed_bits, String(0x4019999a));
    assert.equal(probes.normal_first_frame_bits, probes.normal_speed_bits);
    assert.equal(probes.hurry_first_frame_bits, probes.hurry_speed_bits);
    assert(BigInt(probes.max_queued_events) >= 2n);
    assert(BigInt(probes.forward_event_count) >= 6n);
    assert(BigInt(probes.reverse_event_count) >= 3n);
    assert(BigInt(probes.hunger_final) > -80n);
  } else if (scenario === 2) {
    assert.equal(probes.applied_effects, '2');
    assert.equal(probes.unique_operations, '3');
    assert.equal(probes.cancelled_operations, '1');
    assert.equal(probes.authority_takeovers, '2');
    assert.equal(probes.rejected_fenced_deliveries, '4');
    assert.equal(probes.duplicate_effect_deliveries, '2');
    assert.equal(probes.replacement_generation, '2');
    assert.equal(probes.xl_response_value, '24000');
  } else {
    assert.equal(probes.normalized_initial_tick, '1');
    assert.equal(probes.portal_callbacks, '1');
    assert.equal(probes.portal_wait_ticks, '3');
    assert.equal(probes.vm_suspended_frames, '1');
    assert.equal(probes.route_finished_count, '1');
    assert.equal(probes.vm_route_success_count, '1');
    assert.equal(probes.vm_route_failure_count, '0');
    assert.equal(probes.rejected_route_callbacks, '5');
    assert.equal(probes.portal_final_level, '2');
    assert.equal(probes.portal_final_x, '104');
    assert.equal(probes.portal_final_y, '40');
  }
  return { scenario, name: names[scenario], seed: seed.toString(), ticks, snapshots, probes };
}

const wasmBytes = readFileSync(wasmPath);
const nativeBytes = readFileSync(nativePath);
const module = await WebAssembly.compile(wasmBytes);
const imports = WebAssembly.Module.imports(module);
assert.deepEqual(imports, [], 'simulation module requires no WASI, clock, random, graphics or other host imports');
const expectedExports = ['replay_run', 'replay_len', 'replay_byte'];
for (const name of expectedExports) {
  assert(WebAssembly.Module.exports(module).some(item => item.name === name && item.kind === 'function'), `${name} export`);
}

const native = new Map();
for (const line of nativeBytes.toString('utf8').trim().split(/\r?\n/)) {
  const match = /^(\d+) (\d+) ([0-9a-f]+)$/.exec(line);
  assert(match && match[3].length % 2 === 0, 'native CLI record format');
  const scenario = Number(match[1]);
  const seed = BigInt(match[2]);
  const key = `${scenario}:${seed}`;
  assert(!native.has(key), `duplicate native record ${key}`);
  const bytes = Buffer.from(match[3], 'hex');
  const report = readReport(bytes);
  assert.equal(report.scenario, scenario);
  assert.equal(report.seed, seed.toString());
  native.set(key, { bytes, report });
}
assert.equal(native.size, names.length * seeds.length, 'complete scenario/seed matrix');

function output(instance) {
  const length = instance.exports.replay_len();
  assert(Number.isInteger(length) && length >= 0 && length <= maxReportBytes, 'bounded export length');
  const bytes = Buffer.alloc(length);
  for (let at = 0; at < length; at++) {
    const byte = instance.exports.replay_byte(at);
    assert(byte >= 0 && byte < 256, 'export returned a byte');
    bytes[at] = byte;
  }
  assert.equal(instance.exports.replay_byte(length), 256, 'index at end is rejected');
  assert.equal(instance.exports.replay_byte(0xffffffff), 256, 'large index is rejected');
  return bytes;
}

let comparedTicks = 0;
let comparedSnapshots = 0;
const cases = [];
for (let scenario = 0; scenario < names.length; scenario++) {
  for (const seed of seeds) {
    const key = `${scenario}:${seed}`;
    const expected = native.get(key);
    assert(expected, `missing native case ${key}`);
    for (let fresh = 0; fresh < freshInstances; fresh++) {
      const instance = await WebAssembly.instantiate(module, {});
      assert.equal(instance.exports.replay_len(), 0, 'fresh instance has no previous result');
      assert.equal(instance.exports.replay_byte(0), 256, 'fresh instance bounds check');
      for (let repeat = 0; repeat < repetitions; repeat++) {
        const status = instance.exports.replay_run(scenario, Number(seed & 0xffffffffn), Number(seed >> 32n));
        const bytes = output(instance);
        assert.equal(status, 0, `WASM fixture error ${key}: ${status ? bytes.toString('utf8') : ''}`);
        const actual = readReport(bytes);
        for (let tick = 0; tick < actual.ticks.length; tick++) {
          assert.deepEqual(actual.ticks[tick], expected.report.ticks[tick], `${key}, tick ${tick + 1}, instance ${fresh}, repetition ${repeat}`);
        }
        assert.deepEqual(actual.snapshots, expected.report.snapshots, `${key}: every snapshot length/digest`);
        assert.deepEqual(actual.probes, expected.report.probes, `${key}: semantic observations`);
        assert(bytes.equals(expected.bytes), `${key}: all report and raw snapshot bytes`);
        comparedTicks += actual.ticks.length;
        comparedSnapshots += actual.snapshots.length;
        // Deliberately replace the result with a rejected request between runs.
        // The following valid call must initialize a completely new simulation.
        assert.equal(instance.exports.replay_run(0xffffffff, 0, 0), 1);
        assert.match(output(instance).toString('utf8'), /^unknown replay scenario /);
      }
    }
    const { ticks, ...summary } = expected.report;
    const result = { ...summary, tickCount: ticks.length,
      firstAcceptedTick: ticks[0].tick, lastAcceptedTick: ticks.at(-1).tick,
      instructions: ticks.reduce((sum, tick) => sum + tick.instructions, 0),
      finalStateHash: ticks.at(-1).stateHash, finalRng: ticks.at(-1).rng,
      reportBytes: expected.bytes.length, reportSha256: sha256(expected.bytes),
      // Compact proof that every ordered hash was included, in addition to the
      // individual comparisons above and full raw records in the native file.
      orderedTickHashSha256: sha256(Buffer.concat(ticks.map(tick => Buffer.from(tick.stateHash, 'hex')))) };
    cases.push(result);
    console.log(`PASS ${names[scenario]} seed=${seed} ticks=${ticks.length} snapshots=${summary.snapshots.length} final=${result.finalStateHash}`);
  }
}

const evidence = {
  format: 'swarm-a-native-wasm-replay-evidence-v1',
  recordedAtUtc: new Date().toISOString(),
  node: process.version,
  nativePlatform: `${process.platform}-${process.arch}`,
  applicationSourceSha256,
  rustc: execFileSync('rustc', ['--version', '--verbose'], { encoding: 'utf8' }).trim(),
  cargo: execFileSync('cargo', ['--version'], { encoding: 'utf8' }).trim(),
  wasmBytes: wasmBytes.length,
  wasmSha256: sha256(wasmBytes),
  wasmImports: imports,
  nativeRecordsBytes: nativeBytes.length,
  nativeRecordsSha256: sha256(nativeBytes),
  scenarioCount: names.length,
  seeds: seeds.map(String),
  freshInstancesPerCase: freshInstances,
  repetitionsPerInstance: repetitions,
  wasmScenarioExecutions: names.length * seeds.length * freshInstances * repetitions,
  comparedTickRecords: comparedTicks,
  comparedSnapshotRecords: comparedSnapshots,
  legacyEngineOracle: false,
  cases,
};
assert.equal(sourceDigest(), applicationSourceSha256, 'application sources changed while replay was executing');
if (evidencePath) writeFileSync(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`);
console.log(`PASS: ${evidence.wasmScenarioExecutions} WASM scenario executions, ${comparedTicks} exact per-tick comparisons, ${comparedSnapshots} raw snapshot comparisons; no host imports.`);
