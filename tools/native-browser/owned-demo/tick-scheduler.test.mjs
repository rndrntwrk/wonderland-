import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

// Exercise the actual runner scheduler, without loading Miniflare or a native host.
const runner = readFileSync(new URL('../owned-demo.mjs', import.meta.url), 'utf8');
const scheduler = runner.slice(runner.indexOf('    const tickPeriodMs ='), runner.indexOf('    ready = true;', runner.indexOf('    const tickPeriodMs =')));
assert.ok(scheduler.includes('async function pumpTick()'));
const period = 1000 / 30;
function harness(options = {}) {
  let now = 0, nextId = 0, active = 0, maximum = 0, serial = Promise.resolve();
  const timers = new Map(), starts = [], commits = [], calls = [], queueDone = [], timerArms = [];
  const setTimeout = (fn, delay) => { const id = ++nextId; timerArms.push({at:now,delay,callback:fn.name}); timers.set(id, { at: now + Math.max(1, delay), fn }); return id; };
  const clearTimeout = (id) => timers.delete(id);
  const sleep = (delay) => new Promise(resolve => setTimeout(resolve, delay));
  const context = vm.createContext({
    performance: { now: () => now }, Date: { now: () => now },
    setTimeout, clearTimeout, Math, sockets: new Set(options.idle ? [] : [{ live: true }]),
    closing: false, halted: null, ticking: false, timer: undefined,
    departureWork: false, lastIdleFenceSweep: 0,
    serially(fn) {
      if (context.closing) return Promise.reject(Error('closed'));
      const task = serial.then(async () => {
        if (context.halted) throw Error('paused');
        if(options.queueWait)await sleep(options.queueWait);
        active++; maximum = Math.max(maximum, active); starts.push(now);
        try { return await fn(); } finally { active--; }
      });
      serial = task.catch(() => {}); return task;
    },
    async sweepFences() { calls.push('fences'); if (options.duration) await sleep(typeof options.duration === 'function' ? options.duration(starts.length) : options.duration); },
    async deliverDepartures() { calls.push('deliver'); return options.delivery !== false; },
    async drainOneDeparture() { calls.push('drain'); return options.drain === true; },
    async lot(method) { calls.push(method); return options.pending ? [{}] : []; },
    async prepare(kind) { calls.push(kind); return {}; },
    async commit() { calls.push('commit'); if (options.fail) throw Error('unknown durable decision'); const result = { kind: options.rejected ? 'rejected' : 'committed' }; if (result.kind === 'committed') commits.push(now); return result; },
    stop(error) { context.halted = error.message; },
  });
  vm.runInContext(scheduler, context);
  const flush = async () => { for (let i = 0; i < 40; i++) await Promise.resolve(); };
  const advance = async (until) => {
    await flush();
    for (;;) {
      const next = [...timers].sort((a,b) => a[1].at - b[1].at)[0];
      if (!next || next[1].at > until + 1e-9) break;
      now = next[1].at; timers.delete(next[0]); next[1].fn(); await flush();
    }
    now = until; await flush();
  };
  return { context, starts, commits, calls, timers, queueDone, timerArms, advance, max: () => maximum,
    blockQueue(ms) { serial = serial.then(async () => {await sleep(ms);queueDone.push(now);}); },
    pause(ms) { now += ms; for (const timer of timers.values()) timer.at = Math.max(timer.at, now); },
    close() { context.closing = true; clearTimeout(context.timer); },
  };
}

test('persistent overruns yield with at most six immediate recovery turns', async () => {
  const h = harness({ duration: 35 }); await h.advance(420);
  assert.ok(h.starts.length >= 8); assert.equal(h.max(), 1);
  for (let i=1;i<=6;i++) assert.ok(Math.abs(h.starts[i]-h.starts[i-1]-36)<1e-6);
  assert.ok(Math.abs(h.starts[7]-h.starts[6]-(35+period))<1e-6, 'seventh immediate recovery must be refused');
});
test('under-budget bodies wait the remaining period', async () => {
  const h = harness({ duration: 10 }); await h.advance(180);
  for (let i=1;i<h.starts.length;i++) assert.ok(Math.abs(h.starts[i]-h.starts[i-1]-period)<1e-6);
  assert.equal(h.max(), 1);
});
test('queue wait does not create tick backlog or use enqueue time for pacing', async () => {
  const h = harness({ duration: 10 }); h.blockQueue(100); await h.advance(180);
  assert.equal(h.starts[0],100); assert.ok(Math.abs(h.starts[1]-100-period)<1e-6); assert.equal(h.max(),1);
});
test('a 200ms body discards excessive debt and resumes ordinary pacing', async () => {
  const h = harness({ duration: (n) => n===1 ? 200 : 2 }); await h.advance(400);
  assert.ok(Math.abs(h.starts[1]-h.starts[0]-200-period)<1e-6);
  for(let i=2;i<h.starts.length;i++) assert.ok(Math.abs(h.starts[i]-h.starts[i-1]-period)<1e-6);
});
for (const tail of [69.137974, 109.735042]) {
  test(`bounded recovery repays measured/eligible body tail ${tail}ms`, async () => {
    const h = harness({duration:n=>n===1?tail:19.43}); await h.advance(450);
    assert.equal(h.max(),1);
    const aligned = h.starts.findIndex((value,index)=>index>0 && Math.abs(value-(h.starts[0]+index*period))<1e-6);
    assert.ok(aligned>0 && aligned<=7, 'eligible debt must return to its original cadence');
    const expected=['fences','deliver','drain','pendingOwnedDeparturesV1','prepare_player_tick','commit'];
    for(let i=0;i<h.commits.length;i++) assert.deepEqual(h.calls.slice(i*6,i*6+6),expected,'each recovered tick repeats all guards');
  });
}
test('the six-turn limit discards debt even when a 122ms body remains under the debt cap', async () => {
  const h=harness({duration:n=>n===1?122:19.43}); await h.advance(450);
  for(let i=2;i<=6;i++)assert.ok(Math.abs(h.starts[i]-h.starts[i-1]-20.43)<1e-6);
  assert.ok(Math.abs(h.starts[7]-h.starts[6]-period)<1e-6);
});
for(const body of [24.137709,18.387542,20.687125,15.818167]) {
  test(`burst cap resumes ordinary start spacing after observed ${body}ms body`, async () => {
    const h=harness({duration:n=>n===1?period+100:body}); h.blockQueue(100); await h.advance(800);
    assert.equal(h.max(),1);
    assert.ok(Math.abs(h.starts[7]-h.starts[6]-period)<1e-6);
    assert.ok(Math.abs(h.starts[8]-h.starts[7]-period)<1e-6,'discarded debt must not return after cooldown');
    const recoveryArms=h.timerArms.filter(x=>x.callback==='pumpTick');let run=0,max=0;
    for(const arm of recoveryArms){run=arm.delay===0?run+1:0;max=Math.max(max,run);}
    assert.equal(max,6);
  });
}
for (const debt of [99.999,100,100.001]) {
  test(`100ms debt boundary: ${debt}ms`, async () => {
    const h=harness({duration:n=>n===1?period+debt:10}); h.blockQueue(100); await h.advance(350);
    const spacing=h.starts[1]-h.starts[0];
    assert.ok(Math.abs(spacing-(period+debt+(debt>100?period:1)))<1e-6);
  });
}
test('a short active FIFO wait recovers its original cadence through yielded turns', async () => {
  const h=harness({duration:10}); await h.advance(50); h.blockQueue(50); await h.advance(180);
  assert.ok(Math.abs(h.starts[1]-100)<1e-6);
  assert.ok(Math.abs(h.starts[2]-111)<1e-6);
  assert.ok(Math.abs(h.starts[3]-(h.starts[0]+3*period))<1e-6);
  assert.equal(h.max(),1);
});
test('short waits before the first active body earn no inactive-time credit', async () => {
  const h=harness({duration:10}); h.blockQueue(60); await h.advance(200);
  assert.equal(h.starts[0],60);
  assert.ok(Math.abs(h.starts[1]-60-period)<1e-6);
});
test('queued work precedes a recovery turn and source bodies never overlap', async () => {
  const h=harness({duration:n=>n===1?69.137974:19.43}); await h.advance(104); h.blockQueue(20); await h.advance(350);
  assert.equal(h.starts[2],h.queueDone[0]);
  assert.ok(h.starts[2]>h.starts[1]+19.43);
  assert.equal(h.max(),1);
});
test('repeated short FIFO waits cannot evade the six-turn burst cap', async () => {
  const h=harness({duration:10,queueWait:35}); await h.advance(900);
  let run=0,max=0;
  for(const arm of h.timerArms.filter(x=>x.callback==='pumpTick')) {run=arm.delay===0?run+1:0;max=Math.max(max,run);}
  assert.equal(max,6);
  assert.equal(h.max(),1);
});
test('a long FIFO wait discards existing debt before the next body', async () => {
  const h=harness({duration:n=>n===1?69.137974:19.43}); await h.advance(50); h.blockQueue(150); await h.advance(390);
  assert.ok(Math.abs(h.starts[1]-(period+69.137974+150))<1e-6);
  assert.ok(Math.abs(h.starts[2]-h.starts[1]-period)<1e-6);
});
for(const wait of [99.999,100]) {
  test(`FIFO pause reset boundary: ${wait}ms`, async () => {
    const h=harness({duration:10}); await h.advance(50); h.blockQueue(wait+2*period-50); await h.advance(350);
    const spacing=h.starts[2]-h.starts[1];
    assert.ok(Math.abs(spacing-(wait>=100?period:11))<1e-6);
  });
}
test('short FIFO wait plus existing work debt still obeys the total 100ms cap', async () => {
  const h=harness({duration:n=>n===1?69.137974:19.43}); await h.advance(50); h.blockQueue(100.999); await h.advance(350);
  assert.ok(Math.abs(h.starts[2]-h.starts[1]-19.43-period)<1e-6);
});
test('an excessive pre-execution pause resets cadence instead of producing a burst', async () => {
  const h=harness({duration:10}); await h.advance(50); h.pause(150); await h.advance(300);
  assert.equal(h.starts[1],200);
  assert.ok(Math.abs(h.starts[2]-200-period)<1e-6);
});
for(const lateness of [99.999,100]) {
  test(`armed callback pause boundary: ${lateness}ms`, async () => {
    const h=harness({duration:10}); await h.advance(50); h.pause(2*period+lateness-50); await h.advance(310);
    assert.ok(Math.abs(h.starts[2]-h.starts[1]-(lateness>=100?period:11))<1e-6);
  });
}
for (const gate of ['idle','halt','departure']) {
  test(`${gate} clears previously earned debt before resuming`, async () => {
    const options={duration:n=>n===1?69.137974:10},h=harness(options);
    await h.advance(102.5);
    if(gate==='idle')h.context.sockets.clear();
    if(gate==='halt')h.context.halted='paused';
    if(gate==='departure')options.delivery=false;
    await h.advance(150);
    if(gate==='idle')h.context.sockets.add({live:true});
    if(gate==='halt')h.context.halted=null;
    if(gate==='departure')options.delivery=true;
    const before=h.commits.length; await h.advance(300);
    const resumed=h.commits.slice(before);
    assert.ok(resumed.length>=2);
    assert.ok(Math.abs(resumed[1]-resumed[0]-period)<1e-6);
  });
}
test('explicit recovery resets phase even between timer polls', async () => {
  const h=harness({duration:n=>n===1?69.137974:10}); await h.advance(102.5);
  assert.equal(typeof h.context.resetTickCadence,'function'); h.context.resetTickCadence();
  await h.advance(220);
  assert.ok(Math.abs(h.starts[2]-h.starts[1]-period)<1e-6);
  for (const anchor of ['const stop = (error) => {\n    resetTickCadence();','async function recoverHost() {\n    resetTickCadence();','closing = true;\n    resetTickCadence();'])assert.ok(runner.includes(anchor));
});
for (const options of [{ delivery:false },{ drain:true },{ pending:true }]) {
  test(`departure gate prevents ordinary ticks: ${JSON.stringify(options)}`, async () => {
    const h=harness(options); await h.advance(100); assert.equal(h.commits.length,0);
    assert.equal(h.calls.includes('prepare_player_tick'),false); assert.equal(h.calls[0],'fences');
  });
}
for (const options of [{ fail:true },{ rejected:true }]) {
  test(`commit failure halts without counted progress: ${JSON.stringify(options)}`, async () => {
    const h=harness(options); await h.advance(200); assert.ok(h.context.halted); assert.equal(h.commits.length,0); assert.equal(h.starts.length,1);
  });
}
test('halt polling resumes after recovery clears halted', async () => {
  const h=harness(); h.context.halted='paused'; await h.advance(100); assert.equal(h.starts.length,0);
  h.context.halted=null; await h.advance(150); assert.ok(h.commits.length>0);
});
test('close during pending timeout prevents execution', async () => {
  const h=harness(); h.close(); await h.advance(200); assert.equal(h.starts.length,0); assert.equal(h.timers.size,0);
});
test('close during body prevents rearming', async () => {
  const h=harness({duration:35}); await h.advance(40); h.close(); await h.advance(200); assert.equal(h.starts.length,1); assert.equal(h.timers.size,0);
});
test('close while queued prevents source work and rearming', async () => {
  const h=harness(); h.blockQueue(100); await h.advance(40); h.close(); await h.advance(200); assert.equal(h.starts.length,1); assert.deepEqual(h.calls,[]); assert.equal(h.timers.size,0);
});
test('idle security sweeps retain five-second gate without ordinary ticks', async () => {
  const h=harness({idle:true}); await h.advance(4999); assert.equal(h.starts.length,0);
  await h.advance(10100); assert.equal(h.starts.length,2); assert.equal(h.commits.length,0);
});
test('runner closes the timeout and preserves production serial queue/fence boundary', () => {
  assert.ok(runner.includes('if (timer) clearTimeout(timer);'));
  assert.ok(runner.includes('if (halted && !allowPaused) throw Error("Durable host is paused.");'));
  assert.ok(scheduler.indexOf('await sweepFences()') < scheduler.indexOf('prepare("prepare_player_tick")'));
  assert.ok(!scheduler.includes('setInterval'));
});
