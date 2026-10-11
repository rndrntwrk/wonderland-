import test from 'node:test';
import assert from 'node:assert/strict';
import { connectNativeSocket } from '../public/native-socket.mjs';

class Socket {
  static all = [];
  constructor(url, protocols) {
    this.url = url; this.protocols = protocols; this.readyState = 0;
    this.bufferedAmount = 0; this.sent = []; this.closed = 0;
    Socket.all.push(this);
  }
  send(value) { if (this.readyState !== 1) throw Error('not open'); this.sent.push(value); }
  close() { this.closed++; this.readyState = 3; }
  open() { this.readyState = 1; this.onopen?.({}); }
  frame(value) { this.onmessage?.({data: value}); }
}
function setup(extra = {}) {
  const frames = [], states = [], timers = new Map();
  let next = 0;
  const client = connectNativeSocket('wss://game.example/v1/native-lot/stream', 'ticket-only-in-memory',
    value => { frames.push([...value]); return null; }, state => states.push(state), {
      allowedOrigin: 'https://game.example', WebSocket: Socket, maxPacketBytes: 1024,
      setTimeout: fn => { const id = ++next; timers.set(id, fn); return id; },
      clearTimeout: id => timers.delete(id), ...extra,
    });
  return { client, socket: Socket.all.at(-1), frames, states, timers };
}
const packet = (...bytes) => new Uint8Array(bytes).buffer;

test('ticket is sent in the first frame, never in URL or subprotocol', () => {
  const s = setup(); s.socket.open();
  assert.equal(s.socket.binaryType, 'arraybuffer');
  assert.equal(s.socket.url, 'wss://game.example/v1/native-lot/stream');
  assert.deepEqual(JSON.parse(s.socket.sent[0]), {type: 'native_auth', ticket: 'ticket-only-in-memory'});
  assert.equal(s.socket.protocols, undefined);
  assert.ok(!JSON.stringify(s.states).includes('ticket-only-in-memory'));
  s.client.dispose(); assert.equal(s.timers.size, 0);
});
test('every binary message is delivered synchronously in socket order', () => {
  const s = setup(); s.socket.open();
  for (let i = 0; i < 20; i++) s.socket.frame(packet(i));
  assert.deepEqual(s.frames, Array.from({length: 20}, (_, i) => [i]));
  s.client.dispose();
});
test('byte-identical frames are not deduplicated by the browser host', () => {
  const s = setup(); s.socket.open(); s.socket.frame(packet(1)); s.socket.frame(packet(1));
  assert.deepEqual(s.frames, [[1], [1]]); s.client.dispose();
});
test('text and over-budget frames close once before reaching the native decoder', () => {
  for (const bad of ['untrusted text', new ArrayBuffer(1025)]) {
    const s = setup(); s.socket.open(); const saved = s.socket.onmessage;
    s.socket.frame(bad); saved({data: packet(1)});
    assert.equal(s.frames.length, 0); assert.equal(s.socket.closed, 1);
    assert.equal(s.states.at(-1), 'failed'); assert.equal(s.timers.size, 0);
  }
});
test('disposed callbacks cannot revive or affect a replacement connection', () => {
  const old = setup(); old.socket.open(); const late = old.socket.onmessage;
  old.client.dispose(); const fresh = setup(); fresh.socket.open();
  late({data: packet(9)}); fresh.socket.frame(packet(7));
  assert.deepEqual(old.frames, []); assert.deepEqual(fresh.frames, [[7]]);
  assert.equal(old.socket.onmessage, null); fresh.client.dispose();
});
test('connect/handshake timeout closes resources even without a server reply', () => {
  const s = setup(); [...s.timers.values()][0]();
  assert.equal(s.socket.closed, 1); assert.equal(s.states.at(-1), 'failed');
  assert.equal(s.timers.size, 0);
});
test('backpressure rejects writes without silently queuing unbounded intents', () => {
  const s = setup(); s.socket.open(); s.client.ready();
  s.socket.bufferedAmount = 131072;
  assert.equal(s.client.send(new Uint8Array([1])), false);
  assert.equal(s.socket.sent.length, 1); s.client.dispose();
});
test('writes before readiness, after close, and oversized writes reject', () => {
  const s = setup();
  assert.equal(s.client.send(new Uint8Array([1])), false);
  s.socket.open(); assert.equal(s.client.send(new Uint8Array([1])), false);
  s.client.ready(); assert.equal(s.client.send(new Uint8Array([1])), true);
  assert.equal(s.client.send(new Uint8Array(65537)), false);
  s.client.dispose(); assert.equal(s.client.send(new Uint8Array([1])), false);
});
test('wrong origins, URL tokens, credentials, plaintext public sockets and invalid bounds reject', () => {
  for (const url of ['wss://elsewhere.example/x','wss://u:p@game.example/x',
    'wss://game.example/x?ticket=secret','wss://game.example/x#secret','ws://game.example/x',
    'wss://game.example/x\\evil']) {
    assert.throws(() => connectNativeSocket(url, 'secret', () => {}, () => {},
      {allowedOrigin:'https://game.example', WebSocket: Socket}), /Invalid native connection/);
  }
  for (const maxPacketBytes of [0, -1, NaN, Infinity, 1.5, 100 * 1024 * 1024]) {
    assert.throws(() => setup({maxPacketBytes}));
  }
});
test('a decoder exception terminates current connection without exposing its text', () => {
  const states = [];
  const client = connectNativeSocket('wss://game.example/live', 'hidden',
    () => { throw Error('do-not-show-secret'); }, state => states.push(state),
    {allowedOrigin: 'https://game.example', WebSocket: Socket});
  const socket = Socket.all.at(-1); socket.open(); socket.frame(packet(1));
  assert.equal(states.at(-1), 'failed'); assert.ok(!JSON.stringify(states).includes('secret'));
  client.dispose();
});
