import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

// Execute the exact inline module consumed by wasm-bindgen. Only the platform
// import promise is controlled; these are production startup/lifetime rules.
const rust = await readFile(new URL('../../../apps/web-shell/src/audio_bridge.rs', import.meta.url), 'utf8');
const inline = rust.match(/inline_js\s*=\s*r#"([\s\S]*?)"#/)[1];
const bridgeModule = await import(`data:text/javascript;base64,${Buffer.from(inline).toString('base64')}`);
function deferred() { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
function fixture() {
  const calls = { opened: 0, disposed: 0, delivered: [] };
  const host = { applyAll: items => calls.delivered.push(items), takeFinished: () => [{ generation: '9007199254740993', serial: '1' }], snapshot: () => ({ lastError: null, lastNotice: null }) };
  return { calls, module: { openSourceAudioControls() { calls.opened++; }, disposeSourceAudio() { calls.disposed++; }, acceptedAudioHost: () => host } };
}

test('concurrent startup and open share one module import and open when it finishes', async () => {
  const gate = deferred(), f = fixture(); let loads = 0;
  const bridge = bridgeModule.createSourceAudioBridge({ load: () => { loads++; return gate.promise; } });
  const startup = bridge.initialize(), opening = bridge.open(); await Promise.resolve();
  assert.equal(loads, 1); assert.equal(bridge.ready(), false); assert.equal(f.calls.opened, 0);
  gate.resolve(f.module); await Promise.all([startup, opening]); assert.equal(bridge.ready(), true); assert.equal(f.calls.opened, 1); assert.equal(bridge.error(), null);
});
test('module failure is visible and an explicit retry uses the same configured importer', async () => {
  const f = fixture(); let loads = 0;
  const bridge = bridgeModule.createSourceAudioBridge({ load: async () => { if (++loads === 1) throw Error('ERR_BLOCKED_BY_CLIENT'); return f.module; } });
  await assert.rejects(bridge.open(), /could not load/); assert.equal(bridge.ready(), false); assert.match(bridge.error(), /ERR_BLOCKED_BY_CLIENT/);
  await bridge.open(); assert.equal(loads, 2); assert.equal(bridge.error(), null); assert.equal(f.calls.opened, 1);
});
test('cleanup fences a late import and prevents a departed component opening a dialog', async () => {
  const gate = deferred(), f = fixture(); const bridge = bridgeModule.createSourceAudioBridge({ load: () => gate.promise });
  const opening = bridge.open(); await bridge.clean(); gate.resolve(f.module); await assert.rejects(opening, /cancelled/);
  assert.equal(bridge.ready(), false); assert.equal(f.calls.opened, 0); await bridge.open(); assert.equal(f.calls.opened, 1);
});
test('cleanup disposes the old host once while a fresh owner can initialize', async () => {
  const f = fixture(); const bridge = bridgeModule.createSourceAudioBridge({ load: async () => f.module });
  await bridge.initialize(); await bridge.clean(); await bridge.clean(); assert.equal(f.calls.disposed, 1);
  await bridge.open(); bridge.deliver('[{"Stop":{"voice":{"generation":"9007199254740993","serial":"1"}}}]');
  assert.equal(f.calls.delivered[0][0].Stop.voice.generation, '9007199254740993'); assert.equal(JSON.parse(bridge.finished())[0].generation, '9007199254740993');
});
test('invalid module shape cannot report ready and can be retried', async () => {
  const f = fixture(); let loads = 0; const bridge = bridgeModule.createSourceAudioBridge({ load: async () => ++loads === 1 ? {} : f.module });
  await assert.rejects(bridge.initialize(), /could not load/); assert.equal(bridge.ready(), false); assert.match(bridge.error(), /incompatible/i); await bridge.initialize(); assert.equal(bridge.ready(), true);
});

test('Trunk audio assets exactly match the production modules exercised by these tests', async () => {
  for (const filename of ['source-audio.mjs', 'browser-audio.mjs']) {
    const source = await readFile(new URL(filename, import.meta.url), 'utf8');
    const shipped = await readFile(new URL(`../../../apps/web-shell/public/audio/${filename}`, import.meta.url), 'utf8');
    assert.equal(shipped, source, `Update the Trunk copy of ${filename}`);
  }
});
