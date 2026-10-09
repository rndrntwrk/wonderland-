import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {startFacadeWorker,pollFacadeWorker,cancelFacadeWorker} from '../../public/facade-worker-host.mjs';
const input=()=>new Uint8Array([1,2,3]);
class Worker {
 static made=[];
 constructor(url,options){this.url=String(url);this.options=options;this.terminated=0;Worker.made.push(this);}
 postMessage(message,transfer){this.message=message;this.transfer=transfer;}
 terminate(){this.terminated++;}
 emit(message){this.onmessage?.({data:message});}
}
const environment={Worker,baseURI:'https://wonderland.example/game/',setTimeout:()=>1,clearTimeout:()=>{}};
const last=()=>Worker.made.at(-1);
test('actual Rust UI dispatches constructor/render work only in the worker',async()=>{
 const source=await readFile(new URL('../../src/world_facade.rs',import.meta.url),'utf8');
 assert.doesNotMatch(source,/WorldFacadeJob::new|job\.step\(/);
 assert.match(source,/start_facade_worker\(/);
 assert.match(source,/cancel_facade_worker\(/);
});
test('single owned worker receives a copied transfer rather than borrowed WASM memory',()=>{
 const bytes=input(),id=startFacadeWorker(bytes,environment),w=last();
 assert.ok(Number.isInteger(id)&&id>0);
 assert.equal(w.options.type,'module');assert.equal(new URL(w.url).origin,'https://wonderland.example');
 assert.equal(pollFacadeWorker(id),'starting');
 bytes.fill(99);
 assert.deepEqual([...new Uint8Array(w.message.bytes)],[1,2,3]);
 assert.equal(w.transfer[0],w.message.bytes);
 cancelFacadeWorker(id);assert.equal(w.terminated,1);
});
test('cancellation terminates a worker that never responds, including during preparation',()=>{
 const id=startFacadeWorker(input(),environment),w=last();
 w.emit({schema:1,id,phase:'preparing'});assert.equal(pollFacadeWorker(id),'preparing');
 cancelFacadeWorker(id);assert.equal(w.terminated,1);
 assert.throws(()=>pollFacadeWorker(id),/cancelled|finished|unknown/i);
});
test('replacement rejects stale completion and old cancellation cannot stop the new job',()=>{
 const old=startFacadeWorker(input(),environment),a=last();cancelFacadeWorker(old);
 const current=startFacadeWorker(input(),environment),b=last();
 a.emit({schema:1,id:old,phase:'done',bytes:new Uint8Array([1]).buffer,metadata:'{}',key:'a'.repeat(64)});
 cancelFacadeWorker(old);assert.equal(b.terminated,0);assert.equal(pollFacadeWorker(current),'starting');
 cancelFacadeWorker(current);
});
test('concurrent ownership is bounded and malformed transfers create no worker',()=>{
 const id=startFacadeWorker(input(),environment);
 try {assert.throws(()=>startFacadeWorker(input(),environment),/active|busy|one/i);} finally {cancelFacadeWorker(id);}
 const n=Worker.made.length;
 for(const bad of [new Uint8Array(0),{},new Uint8Array(32*1024*1024+1)]) assert.throws(()=>startFacadeWorker(bad,environment),/input|budget/i);
 assert.equal(Worker.made.length,n);
});
test('wrong identity/protocol and oversized results fail closed and release the worker',()=>{
 for(const kind of ['id','schema','size']) {
  const id=startFacadeWorker(input(),environment),w=last();
  const message={schema:1,id,phase:'done',bytes:new Uint8Array([1]).buffer,metadata:'{}',key:'a'.repeat(64)};
  if(kind==='id')message.id++;if(kind==='schema')message.schema++;if(kind==='size')message.bytes=new ArrayBuffer(16*1024*1024+1);
  w.emit(message);assert.throws(()=>pollFacadeWorker(id),/result|protocol|identity|budget/i);assert.equal(w.terminated,1);
 }
});
test('terminal result is single-use, exact transferred bytes, and cleanup is idempotent',()=>{
 const id=startFacadeWorker(input(),environment),w=last(),bytes=new Uint8Array([70,83,79,102]).buffer;
 w.emit({schema:1,id,phase:'done',bytes,metadata:'{}',key:'a'.repeat(64)});
 const result=pollFacadeWorker(id);
 assert.equal(result[0].buffer,bytes);assert.equal(result[1],'{}');assert.equal(result[2],'a'.repeat(64));
 assert.equal(w.terminated,1);assert.throws(()=>pollFacadeWorker(id),/cancelled|finished|unknown/i);cancelFacadeWorker(id);assert.equal(w.terminated,1);
});
test('startup failure, runtime error and watchdog timeout release owned resources',()=>{
 for(const mode of ['post','error','timeout']) {
  let timeout;
  class FaultWorker extends Worker { postMessage(...args){if(mode==='post')throw Error('transfer failed');super.postMessage(...args);} }
  const env={...environment,Worker:FaultWorker,setTimeout:f=>{timeout=f;return 1;}};
  if(mode==='post'){assert.throws(()=>startFacadeWorker(input(),env),/transfer/);assert.equal(last().terminated,1);continue;}
  const id=startFacadeWorker(input(),env),w=last();
  if(mode==='error')w.onerror({preventDefault(){}});else timeout();
  assert.throws(()=>pollFacadeWorker(id),/failed|timeout/i);assert.equal(w.terminated,1);
 }
});
