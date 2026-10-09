import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {makeFacadeLinks,releaseFacadeLinks} from '../../public/facade-links.mjs';
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
// This is a header-only transport fixture; Rust tests exercise the actual codec.
const bytes=()=>new Uint8Array([70,83,79,102,1,0,0,0,1,23,42]);
const key='a'.repeat(64);
const receipt=data=>JSON.stringify({kind:'presentation_facade',source_hash:key,bytes:data.length,sha256:hash(data),not_a_game_save:true});

test('facade links reject corrupted bytes even when length and magic still match',async()=>{
 const original=bytes(),info=receipt(original),corrupt=original.slice();corrupt[10]^=1;
 await assert.rejects(async()=>makeFacadeLinks(corrupt,info,key),/digest|checksum/i);
});
test('facade links require a SHA-256 receipt before exposing any download',async()=>{
 const data=bytes(),info=JSON.parse(receipt(data));delete info.sha256;
 await assert.rejects(async()=>makeFacadeLinks(data,JSON.stringify(info),key),/digest|checksum/i);
});
test('facade link integrity checks snapshot temporary caller bytes across the await',async()=>{
 const data=bytes(),initial=data.slice(),promise=makeFacadeLinks(data,receipt(data),key);
 data[9]^=1;
 const urls=await promise;
 try {assert.deepEqual(new Uint8Array(await (await fetch(urls[0])).arrayBuffer()),initial);}
 finally {releaseFacadeLinks(...urls);}
});
test('valid digest downloads preserve the exact supplied source bytes and receipt',async()=>{
 const data=bytes(),info=receipt(data),urls=await makeFacadeLinks(data,info,key);
 try {
  assert.deepEqual(new Uint8Array(await (await fetch(urls[0])).arrayBuffer()),data);
  assert.equal(await (await fetch(urls[1])).text(),info);
 } finally {releaseFacadeLinks(...urls);}
});

test('Rust awaits checksum preparation and frees late URLs after source replacement',async()=>{
 const {readFile}=await import('node:fs/promises');
 const source=await readFile(new URL('../../src/world_facade.rs',import.meta.url),'utf8');
 assert.match(source,/fn make_links[\s\S]*?Result<js_sys::Promise, JsValue>/);
 assert.match(source,/JsFuture::from\(promise\)\s*\.await/);
 assert.match(source,/if !current\(\)\s*\{\s*if let Ok\(Some\(links\)\) = result\s*\{\s*links\.release\(\)/);
 assert.match(source,/Arc::ptr_eq/);
});

test('shared or malformed digests are rejected before asynchronous processing',async()=>{
 const data=bytes();
 if(typeof SharedArrayBuffer!=='undefined') {
  const shared=new Uint8Array(new SharedArrayBuffer(data.length));shared.set(data);
  await assert.rejects(makeFacadeLinks(shared,receipt(data),key),/Invalid facade/);
 }
 for(const value of [null,'f'.repeat(63),'F'.repeat(64),'g'.repeat(64),123]){
  const info=JSON.parse(receipt(data));info.sha256=value;
  await assert.rejects(makeFacadeLinks(data,JSON.stringify(info),key),/checksum/);
 }
});

test('at most two hashes can retain source copies and a failed hash releases capacity',async()=>{
 const subtle=globalThis.crypto.subtle,original=subtle.digest;
 const waiting=[];subtle.digest=()=>new Promise((resolve,reject)=>waiting.push({resolve,reject}));
 const data=bytes(),info=receipt(data),completed=[];
 try {
  const first=makeFacadeLinks(data,info,key),second=makeFacadeLinks(data,info,key);
  await assert.rejects(makeFacadeLinks(data,info,key),/queue is full/);
  assert.equal(waiting.length,2);
  // Attach the rejection handler before releasing the test's asynchronous gate.
  const rejected=assert.rejects(first,/simulated digest failure/);
  waiting[0].reject(new Error('simulated digest failure'));await rejected;
  const third=makeFacadeLinks(data,info,key);assert.equal(waiting.length,3);
  const digest=createHash('sha256').update(data).digest();
  waiting[1].resolve(digest);completed.push(await second);
  waiting[2].resolve(digest);completed.push(await third);
 } finally {
  subtle.digest=original;
  for(const urls of completed)releaseFacadeLinks(...urls);
 }
 const urls=await makeFacadeLinks(data,info,key);releaseFacadeLinks(...urls);
});

test('digest rejection creates no download URL',async()=>{
 const original=URL.createObjectURL;let calls=0;
 URL.createObjectURL=()=>{calls++;throw new Error('unexpected publication');};
 try {
  const data=bytes(),meta=JSON.parse(receipt(data));meta.sha256='0'.repeat(64);
  await assert.rejects(makeFacadeLinks(data,JSON.stringify(meta),key),/checksum mismatch/);
  assert.equal(calls,0);
 } finally {URL.createObjectURL=original;}
});
