import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {drawableViewportReady,waitForDrawableViewport} from './viewport-ready.mjs';

const expected={width:400,height:300,dpr:2};
const snapshot=(width=800,height=600)=>Object.freeze({
  devicePixelRatio:2,sceneHash:'unchanged',tick:30,
  viewport:Object.freeze({cssWidth:400,cssHeight:300,width,height}),
});

// This assembly guard catches the original regression: committing a correct
// helper without ever calling it in the actual browser acceptance workflow.
// The behavioural tests below and actual browser run provide separate evidence.
test('browser resize gate invokes drawable readiness on resize AND restoration',async()=>{
  const source=await readFile(new URL('./browser-gate.mjs',import.meta.url),'utf8');
  assert.match(source,/import\s*\{[^}]*waitForDrawableViewport[^}]*\}\s*from\s*['"]\.\/viewport-ready\.mjs['"]/);
  const start=source.indexOf('await step(`resize-and-DPR-${avatars}`');
  const end=source.indexOf('await step(`presentation-suspend-${avatars}`',start);
  assert.ok(start>=0&&end>start);
  const resizeGate=source.slice(start,end);
  assert.equal((resizeGate.match(/await waitForDrawableViewport\(/g)||[]).length,2);
  assert.doesNotMatch(resizeGate,/waitForTimeout/);
  assert.match(resizeGate,/width:400,height:300,dpr/);
  assert.match(resizeGate,/width:640,height:480,dpr/);
});

test('reference workflow executes the viewport regression tests',async()=>{
  const source=await readFile(new URL('./verify.sh',import.meta.url),'utf8');
  assert.match(source,/task_check node --test tools\/swarm-c\/viewport-ready\.test\.mjs/);
});

test('DPR2 viewport requires BOTH CSS and actual backing-store dimensions',()=>{
  assert.equal(drawableViewportReady(snapshot(),expected),true);
  for(const value of [snapshot(1280,960),snapshot(400,300),snapshot(800,960),
    {...snapshot(),devicePixelRatio:1},null,{},
    {...snapshot(),viewport:{...snapshot().viewport,cssWidth:640}},snapshot(NaN,600)]){
    assert.equal(drawableViewportReady(value,expected),false);
  }
});

test('a delayed actual resize after 250ms is awaited without changing source state',async()=>{
  let clock=0,reads=0;const stale=snapshot(1280,960),ready=snapshot();
  const result=await waitForDrawableViewport(()=>{reads++;return clock<450?stale:ready;},expected,{
    timeout:1000,interval:50,now:()=>clock,sleep:async ms=>{clock+=ms;},
  });
  assert.equal(result,ready);assert.equal(clock,450);assert.equal(reads,10);
  assert.equal(result.sceneHash,stale.sceneHash);assert.equal(result.tick,stale.tick);
  assert.deepEqual(stale,snapshot(1280,960));
});

test('a permanently wrong backing store fails closed at the declared deadline',async()=>{
  let clock=0;
  await assert.rejects(waitForDrawableViewport(()=>snapshot(400,300),expected,{
    timeout:500,interval:75,now:()=>clock,sleep:async ms=>{clock+=ms;},
  }),error=>error.message.includes('Drawable resize did not settle')&&error.message.includes('"width":400')&&error.message.includes('"dpr":2'));
  assert.equal(clock,500);
});

test('an already correct drawable needs no arbitrary delay',async()=>{
  const ready=snapshot();let sleeps=0;
  assert.equal(await waitForDrawableViewport(()=>ready,expected,{sleep:async()=>{sleeps++;}}),ready);
  assert.equal(sleeps,0);
});

test('invalid wait requests are rejected before reading or mutating the page',async()=>{
  let reads=0;const read=()=>{reads++;return snapshot();};
  for(const options of [{timeout:0},{timeout:Infinity},{timeout:30001},{interval:0},{interval:1001}]){
    await assert.rejects(waitForDrawableViewport(read,expected,options),/Invalid drawable readiness request/);
  }
  for(const bad of [{...expected,width:0},{...expected,height:NaN},{...expected,dpr:-1},undefined]){
    await assert.rejects(waitForDrawableViewport(read,bad),/Invalid drawable readiness request/);
  }
  assert.equal(reads,0);
});
