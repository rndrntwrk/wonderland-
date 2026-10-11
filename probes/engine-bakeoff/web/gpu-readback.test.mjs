import test from 'node:test';
import assert from 'node:assert/strict';
import {EngineCanvasReadback} from './gpu-readback.mjs';

globalThis.GPUTextureUsage={COPY_SRC:1,RENDER_ATTACHMENT:16};
globalThis.GPUBufferUsage={MAP_READ:1,COPY_DST:8};
globalThis.GPUMapMode={READ:1};
function fixture({format='rgba8unorm',map=async()=>{},timeoutMs=1000,validationError=null,validation=null}={}){
  const events=[],raw=new Uint8Array(512);raw.set([1,2,3,4,5,6,7,8,9,10,11,12],0);raw.set([13,14,15,16,17,18,19,20,21,22,23,24],256);
  const buffer={mapAsync:map,getMappedRange:()=>raw.buffer,unmap:()=>events.push('unmap'),destroy:()=>events.push('destroy')};
  const texture={width:3,height:2},canvas={width:3,height:2};
  const device={pushErrorScope:kind=>{assert.equal(kind,'validation');events.push('scope-push');},popErrorScope:()=>{events.push('scope-pop');return validation??Promise.resolve(validationError);},
    createBuffer:descriptor=>{assert.equal(descriptor.size,512);events.push('buffer');return buffer;},createCommandEncoder:()=>({
    copyTextureToBuffer:(source,target,size)=>{assert.equal(source.texture,texture);assert.equal(target.bytesPerRow,256);assert.deepEqual(size,{width:3,height:2,depthOrArrayLayers:1});events.push('copy');},
    finish:()=>({diagnostic:true})})};
  const reader=new EngineCanvasReadback({timeoutMs,diagnostic:(stage,detail)=>events.push({stage,...detail}),stamp:()=>({mode:'full3d',pass:'pick'})});
  const context={configure:configuration=>{events.push('configure');reader.configured(context,canvas,configuration);}};
  reader.configured(context,canvas,{device,format,usage:16});
  const submit=()=>events.push('diagnostic-submit');
  return {reader,device,context,texture,canvas,events,submit};
}
test('readback waits for a new engine-acquired texture and copies padded rows after engine submissions',async()=>{
  const f=fixture();f.reader.acquired(f.context,f.texture);
  const pending=f.reader.read();assert.equal(f.reader.session.configuration.usage,17);
  f.reader.submitted(f.device,f.submit);await Promise.resolve();assert.ok(!f.events.includes('copy'));
  f.reader.acquired(f.context,f.texture);f.events.push('engine-submit-1');f.reader.submitted(f.device,f.submit);
  f.events.push('engine-submit-2');f.reader.submitted(f.device,f.submit);
  const result=await pending;
  assert.deepEqual(Array.from(Buffer.from(result.base64,'base64')),Array.from({length:24},(_,i)=>i+1));
  assert.ok(f.events.indexOf('copy')>f.events.indexOf('engine-submit-2'));
  assert.equal(f.events.filter(e=>e==='diagnostic-submit').length,1);
  assert.ok(f.events.indexOf('scope-pop')>f.events.indexOf('diagnostic-submit'));
  assert.deepEqual(result.stamp,{mode:'full3d',pass:'pick'});assert.equal(result.width,3);assert.equal(result.height,2);
  assert.deepEqual(f.events.slice(-2),['unmap','destroy']);
});
test('diagnostic validation errors reject instead of reporting an unwritten mapped buffer as engine pixels',async()=>{
  let mapped=false;const f=fixture({validationError:{message:'COPY_SRC not permitted'},map:async()=>{mapped=true;}}),pending=f.reader.read();
  f.reader.acquired(f.context,f.texture);f.reader.submitted(f.device,f.submit);
  await assert.rejects(pending,/Diagnostic GPU validation failed: COPY_SRC not permitted/);
  assert.equal(mapped,false);assert.ok(f.events.includes('destroy'));
  assert.equal(f.events.filter(e=>e==='scope-push').length,1);assert.equal(f.events.filter(e=>e==='scope-pop').length,1);
});
test('BGRA canvas bytes are explicitly converted to RGBA without premultiplication',async()=>{
  const f=fixture({format:'bgra8unorm'}),pending=f.reader.read();f.reader.acquired(f.context,f.texture);f.reader.submitted(f.device,f.submit);
  assert.deepEqual(Array.from(Buffer.from((await pending).base64,'base64').subarray(0,8)),[3,2,1,4,7,6,5,8]);
});
test('readback rejects concurrent requests and reconfiguration without consuming unrelated submissions',async()=>{
  const f=fixture(),pending=f.reader.read();await assert.rejects(f.reader.read(),/already pending/);
  f.reader.acquired(f.context,f.texture);f.reader.submitted({},f.submit);await Promise.resolve();assert.ok(!f.events.includes('copy'));
  f.context.configure({...f.reader.session.configuration});await assert.rejects(pending,/reconfigured/);
});
test('readback deadline rejects a stopped engine and releases a stalled mapped buffer',async()=>{
  const stopped=fixture({timeoutMs:5});await assert.rejects(stopped.reader.read(),/deadline/);
  const stalled=fixture({timeoutMs:5,map:()=>new Promise(()=>{})}),pending=stalled.reader.read();
  stalled.reader.acquired(stalled.context,stalled.texture);stalled.reader.submitted(stalled.device,stalled.submit);
  await assert.rejects(pending,/deadline/);assert.ok(stalled.events.includes('destroy'));
});
test('readback rejects dimensions or format unsupported by the diagnostic',async()=>{
  const f=fixture(),pending=f.reader.read();f.texture.width=4;f.reader.acquired(f.context,f.texture);f.reader.submitted(f.device,f.submit);
  await assert.rejects(pending,/dimensions differ/);
  await assert.rejects(fixture({format:'rgba16float'}).reader.read(),/Unsupported/);
  await assert.rejects(new EngineCanvasReadback().read(),/No configured/);
});
test('deadline identifies texture acquisition, engine submission, validation and mapping stalls separately',async()=>{
  const waiting=fixture({timeoutMs:5});
  await assert.rejects(waiting.reader.read(),/deadline.*awaiting-texture/);
  const noSubmit=fixture({timeoutMs:5}),unsubmitted=noSubmit.reader.read();
  noSubmit.reader.acquired(noSubmit.context,noSubmit.texture);
  await assert.rejects(unsubmitted,/deadline.*awaiting-submission/);
  const validation=fixture({timeoutMs:5,validation:new Promise(()=>{})}),unvalidated=validation.reader.read();
  validation.reader.acquired(validation.context,validation.texture);validation.reader.submitted(validation.device,validation.submit);
  await assert.rejects(unvalidated,/deadline.*awaiting-validation/);
  const mapping=fixture({timeoutMs:5,map:()=>new Promise(()=>{})}),unmapped=mapping.reader.read();
  mapping.reader.acquired(mapping.context,mapping.texture);mapping.reader.submitted(mapping.device,mapping.submit);
  await assert.rejects(unmapped,/deadline.*awaiting-map/);
});
test('cancelled mapping releases its buffer and cannot complete a subsequent request',async()=>{
  let releaseMap;
  const f=fixture({map:()=>new Promise(resolve=>{releaseMap=resolve;})}),old=f.reader.read();
  f.reader.acquired(f.context,f.texture);f.reader.submitted(f.device,f.submit);
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(typeof f.reader.cancel,'function','lifecycle owners need a cancellation boundary');
  f.reader.cancel('fixture replaced');await assert.rejects(old,/fixture replaced/);
  assert.equal(f.events.filter(e=>e==='destroy').length,1);
  const next=f.reader.read();releaseMap();await new Promise(resolve=>setImmediate(resolve));
  assert.ok(f.reader.pending,'the old callback cannot consume the new request');
  f.reader.cancel('device lost');await assert.rejects(next,/device lost/);
  assert.equal(f.events.filter(e=>e==='destroy').length,1,'late callbacks must not release a buffer twice');
});
test('retiring an old device or context cannot cancel the current device read',async()=>{
  const f=fixture(),pending=f.reader.read();
  f.reader.deviceLost({},'old device retired');f.reader.unconfigured({});
  assert.ok(f.reader.pending);assert.equal(f.reader.snapshot().configured,true);
  f.reader.deviceLost(f.device,'current device retired');
  await assert.rejects(pending,/current device retired/);
  assert.equal(f.reader.snapshot().configured,false);assert.equal(f.reader.snapshot().pending,null);
  await assert.rejects(f.reader.read(),/No configured/);
});
