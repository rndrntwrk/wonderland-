import test from 'node:test';
import assert from 'node:assert/strict';
import { connectNativeSocket } from '../public/native-socket.mjs';

// Deterministic clock: no wall-clock sleeps, leaked Node timers or timeout edits.
function clock() {
  let now = 0, next = 0;
  const timers = new Map();
  return {
    timers,
    setTimeout(fn, delay) { const id = ++next; timers.set(id, {fn, at: now + delay}); return id; },
    clearTimeout(id) { timers.delete(id); },
    advance(ms) {
      const end = now + ms;
      for (let i = 0; i < 1000; i++) {
        const first = [...timers].sort((a, b) => a[1].at - b[1].at)[0];
        if (!first || first[1].at > end) { now = end; return; }
        now = first[1].at; timers.delete(first[0]); first[1].fn();
      }
      throw Error('Unexpected infinite timer loop');
    },
  };
}
class Socket {
  static latest;
  constructor() { Socket.latest = this; this.readyState = 0; this.bufferedAmount = 0; this.sent = []; this.closed = 0; }
  open() { this.readyState = 1; this.onopen?.({}); }
  send(data) { this.sent.push(data); }
  frame() { this.onmessage?.({data: new Uint8Array([1]).buffer}); }
  close() { this.closed++; this.readyState = 3; }
}
function fixture() {
  const time = clock(), states = [], frames = [];
  const client = connectNativeSocket('wss://world.example/native', 'fixture-only',
    data => { frames.push([...data]); return null; }, state => states.push(state), {
      allowedOrigin: 'https://world.example', WebSocket: Socket,
      setTimeout: time.setTimeout, clearTimeout: time.clearTimeout,
    });
  const socket = Socket.latest;
  socket.open(); client.ready();
  return {time, client, socket, states, frames};
}
const bytes = () => new Uint8Array([7]);

test('ordinary ticks cannot keep a lost action receipt pending forever', () => {
  const s = fixture();
  assert.equal(s.client.send(bytes()), true);
  for (let i = 0; i < 14; i++) { s.time.advance(1000); s.socket.frame(); }
  assert.equal(s.socket.closed, 0);
  s.time.advance(1000);
  assert.equal(s.socket.closed, 1, 'the action has its own 15s deadline, independent of live ticks');
  assert.equal(s.states.at(-1), 'receipt-timeout');
  assert.equal(s.time.timers.size, 0);
  assert.equal(s.socket.sent.length, 2, 'authentication and exactly one action; no automatic retry');
  assert.equal(s.client.send(bytes()), false);
});

test('a second unresolved send cannot replace or extend the first deadline', () => {
  const s = fixture();
  assert.equal(s.client.send(bytes()), true);
  s.time.advance(14000); s.socket.frame();
  assert.equal(s.client.send(bytes()), false);
  s.time.advance(1000);
  assert.equal(s.states.at(-1), 'receipt-timeout');
  assert.equal(s.socket.sent.length, 2);
});

test('repeated readiness notifications do not extend the action deadline', () => {
  const s = fixture(); s.client.send(bytes());
  for (let i = 0; i < 14; i++) { s.time.advance(1000); s.client.ready(); }
  s.time.advance(1000);
  assert.equal(s.states.at(-1), 'receipt-timeout');
});

test('only an explicitly validated decision settles the action deadline', () => {
  const s = fixture(); s.client.send(bytes());
  s.time.advance(14000); s.socket.frame();
  assert.equal(s.client.settled(), true);
  assert.equal(s.client.settled(), false, 'settlement is consumed once');
  s.time.advance(1000);
  assert.equal(s.socket.closed, 0);
  assert.equal(s.time.timers.size, 1, 'the ordinary liveness timer remains');
  s.client.dispose();
});

test('settling an action leaves the connection liveness deadline in force', () => {
  const s = fixture(); s.client.send(bytes()); s.client.settled();
  s.time.advance(15000);
  assert.equal(s.states.at(-1), 'failed');
  assert.equal(s.socket.closed, 1);
  assert.equal(s.time.timers.size, 0);
});

test('an already queued deadline callback cannot close the next action', () => {
  const s = fixture(); s.client.send(bytes());
  const oldCallbacks = [...s.time.timers.values()].map(timer => timer.fn);
  s.client.settled(); s.socket.frame(); s.time.advance(5000);
  assert.equal(s.client.send(bytes()), true);
  for (const fn of oldCallbacks) fn();
  assert.equal(s.socket.closed, 0, 'obsolete timer generations are ignored');
  for (let i = 0; i < 14; i++) { s.time.advance(1000); s.socket.frame(); }
  assert.equal(s.socket.closed, 0);
  s.time.advance(1000);
  assert.equal(s.states.at(-1), 'receipt-timeout');
});

test('disposing cancels both deadlines and fences late timer callbacks', () => {
  const s = fixture(); s.client.send(bytes());
  assert.equal(s.time.timers.size, 2);
  const late = [...s.time.timers.values()].map(timer => timer.fn);
  const before = [...s.states];
  s.client.dispose(); s.client.dispose();
  assert.equal(s.time.timers.size, 0);
  for (const callback of late) callback();
  assert.equal(s.socket.closed, 1);
  assert.deepEqual(s.states, before);
  assert.equal(s.client.settled(), false);
});

test('failed, oversized and backpressured writes create no pending deadline', () => {
  const s = fixture();
  for (const value of [new Uint8Array(), new Uint8Array(65537), 'not binary']) {
    assert.equal(s.client.send(value), false);
    assert.equal(s.time.timers.size, 1);
  }
  s.socket.bufferedAmount = 131072;
  assert.equal(s.client.send(bytes()), false);
  assert.equal(s.time.timers.size, 1);
  s.client.dispose();
});

test('protocol control writes neither start nor clear an action deadline', () => {
  const s = fixture();
  assert.equal(s.client.control(bytes()), true);
  assert.equal(s.time.timers.size, 1);
  s.client.send(bytes()); s.time.advance(14000); s.socket.frame();
  assert.equal(s.client.control(bytes()), true);
  s.time.advance(1000);
  assert.equal(s.states.at(-1), 'receipt-timeout');
});

test('a browser send exception terminates without retaining either timer', () => {
  const s = fixture(); s.socket.send = () => { throw Error('private diagnostic'); };
  assert.equal(s.client.send(bytes()), false);
  assert.equal(s.time.timers.size, 0);
  assert.equal(s.socket.closed, 1);
  assert.equal(s.states.at(-1), 'failed');
  assert.ok(!JSON.stringify(s.states).includes('private'));
});
