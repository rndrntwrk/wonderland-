import test from 'node:test';
import assert from 'node:assert/strict';
import {parseConfig, ProbeController} from './host-core.mjs';

test('immutable fixture URL selection validates exact view and bounded counts',()=>{
  const c=parseConfig('?variant=fyrox-webgl2&mode=full3d&avatars=64&tick=45');
  assert.equal(c.variant,'fyrox-webgl2');assert.equal(c.mode,'full3d');assert.equal(c.avatars,64);assert.equal(c.tick,45);
  assert.ok(Object.isFrozen(c));
  for(const query of ['?avatars=65','?tick=-1','?tick=1.5','?mode=webgl2','?variant=fyrox-webgpu'])assert.throws(()=>parseConfig(query));
});
test('backend requests do not become observations and snapshots are side-effect free',()=>{
  const c=new ProbeController(parseConfig('?variant=bevy-webgpu'));
  assert.equal(c.snapshot().actualBackend,null);assert.equal(c.snapshot().requestedBackend,'webgpu');
  c.observeBackend('webgpu',{vendor:'fixture-adapter'});
  c.submission('webgpu');
  const a=c.snapshot(),b=c.snapshot();
  assert.equal(a.gpuSubmissions,1);assert.equal(b.gpuSubmissions,1);assert.equal(b.actualBackend,'webgpu');
  a.errors.push('mutation');assert.equal(c.snapshot().errors.length,0);
  assert.throws(()=>c.observeBackend('webgl2',{}));
});
test('bounded command queue rejects out-of-range pick and stale input after suspension',()=>{
  const c=new ProbeController(parseConfig(''));
  assert.throws(()=>c.enqueue('selectAt',{x:640,y:0}));
  assert.throws(()=>c.enqueue('selectAt',{x:0,y:480}));
  assert.throws(()=>c.enqueue('selectAt',{x:-1,y:0}));
  assert.throws(()=>c.enqueue('selectAt',{x:'10',y:20}));
  assert.throws(()=>c.enqueue('setTick',{tick:'30'}));
  c.enqueue('selectAt',{x:100,y:90});c.enqueue('suspend');
  assert.throws(()=>c.enqueue('selectAt',{x:100,y:90}));
  assert.equal(c.drain().length,2);assert.equal(c.drain().length,0);
  c.enqueue('resume');
  for(let i=0;i<127;i++)c.enqueue('setTick',{tick:i});
  assert.throws(()=>c.enqueue('setTick',{tick:2}));
});
test('simulated loss is recorded separately and never certifies actual context loss',()=>{
  const c=new ProbeController(parseConfig(''));
  c.enqueue('simulateLoss');
  assert.equal(c.snapshot().simulatedLossCount,1);assert.equal(c.snapshot().actualLossCount,0);
  c.actualLoss('webgl2','browser event');
  assert.equal(c.snapshot().actualLossCount,1);assert.equal(c.snapshot().lifecycle,'lost');
});

test('partial renderer observations survive subsequent fixture-state publications',()=>{
  const c=new ProbeController(parseConfig(''));
  c.publish({engineResourceOwnership:{meshes:7,materials:7,images:14},fixtureDrawCalls:7});
  c.publish({sceneHash:'immutable-fixture',ready:true,mode:'hybrid2d'});
  assert.deepEqual(c.snapshot().engineResourceOwnership,{meshes:7,materials:7,images:14});
  assert.equal(c.snapshot().fixtureDrawCalls,7);
  c.publish({fixtureDrawCalls:8});
  assert.equal(c.snapshot().sceneHash,'immutable-fixture');
  assert.equal(c.snapshot().fixtureDrawCalls,8);
});
