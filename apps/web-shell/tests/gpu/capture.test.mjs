import test from 'node:test';
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
import {readFile} from 'node:fs/promises';
import * as api from '../../public/world-gpu.mjs';

// Valid, intentionally asymmetric 2x2 PNG; tests exercise real Blob and SHA-256.
const PNG=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAF0lEQVR4nGP4z8Dwn+E/w38GBij4DwBfsgn3nqa+JwAAAABJRU5ErkJggg==','base64');
const metadata=()=>JSON.stringify({provenance:{origin:'local/source.xml'},revision:{tick:'9007199254740993'},view:{zoom:1},diagnostics:['Missing original scenery']});
function fixture(options={}){
  assert.equal(typeof api.WorldPngCapture,'function','A bounded capture owner must be exported');
  const created=[],revoked=[],callbacks=[];
  const urlApi={createObjectURL(blob){const url='blob:capture-'+created.length;created.push({url,blob});return url;},revokeObjectURL(url){revoked.push(url);}};
  const store=new api.WorldPngCapture({urlApi,cryptoApi:webcrypto,...options});
  const canvas={width:2,height:2,toBlob(callback,type){assert.equal(type,'image/png');callbacks.push(callback);}};
  let current=true;
  return {store,canvas,created,revoked,callbacks,start:(generation='1',details=metadata())=>store.capture(canvas,generation,details,()=>current),stale:()=>{current=false;}};
}
const png=()=>new Blob([PNG],{type:'image/png'});

test('source owner exposes capture and explicit release without replacing a frame',()=>{
  assert.equal(typeof api.captureSourceWorld,'function');assert.equal(typeof api.clearSourceWorldCapture,'function');
});
test('Rust viewport and source-lot UI wire the real capture action',async()=>{
  const view=await readFile(new URL('../../src/world_renderer.rs',import.meta.url),'utf8');
  const screen=await readFile(new URL('../../src/source_world_screen.rs',import.meta.url),'utf8');
  assert.match(view,/js_name\s*=\s*captureSourceWorld/);assert.match(view,/clear_capture\(/);
  assert.match(screen,/<WorldCapturePanel/);assert.match(screen,/<WorldViewport[^>]*capture/);
});
test('captures exact dimensions, lossless generation, real PNG bytes and provenance hash',async()=>{
  const f=fixture(),promise=f.start('18446744073709551615');f.callbacks[0](png());
  const result=JSON.parse(await promise);assert.equal(result.generation,'18446744073709551615');
  assert.equal(result.width,2);assert.equal(result.height,2);assert.equal(f.created.length,2);
  assert.equal(result.filename,'wonderland-source-view-18446744073709551615.png');
  assert.deepEqual(Buffer.from(await f.created[0].blob.arrayBuffer()),PNG);
  const manifest=JSON.parse(await f.created[1].blob.text());
  assert.equal(manifest.source.revision.tick,'9007199254740993');assert.deepEqual(manifest.source.diagnostics,['Missing original scenery']);
  assert.equal(manifest.image.sha256,Buffer.from(await webcrypto.subtle.digest('SHA-256',PNG)).toString('hex'));
  assert.equal(manifest.image.bytes,PNG.length);assert.equal(manifest.kind,'source-view-capture');
  f.store.clear();assert.deepEqual(f.revoked,f.created.map(x=>x.url));f.store.clear();assert.equal(f.revoked.length,2);
});
test('newer capture cancels old encoder and late callback cannot create URLs',async()=>{
  const f=fixture(),first=f.start().catch(e=>e.name),second=f.start('2');
  assert.equal(await first,'AbortError');f.callbacks[0](png());assert.equal(f.created.length,0);
  f.callbacks[1](png());assert.equal(JSON.parse(await second).generation,'2');f.store.clear();
});
test('clear cancels an unresolved capture immediately and releases ready images',async()=>{
  const f=fixture(),pending=f.start().catch(e=>e.name);f.store.clear('View replaced');
  assert.equal(await pending,'AbortError');f.callbacks[0](png());assert.equal(f.created.length,0);
});
test('source/frame identity is checked again after asynchronous encoding',async()=>{
  const f=fixture(),pending=f.start();f.stale();f.callbacks[0](png());
  await assert.rejects(pending,{name:'AbortError'});assert.equal(f.created.length,0);
});
test('superseded SHA-256 completion cannot publish an old screenshot',async()=>{
  let complete;const digestStarted=new Promise(resolve=>{complete=resolve;});let release;
  const f=fixture({cryptoApi:{subtle:{digest:()=>{complete();return new Promise(resolve=>{release=resolve;});}}}});
  const pending=f.start().catch(e=>e.name);f.callbacks[0](png());await digestStarted;
  f.store.clear();release(new ArrayBuffer(32));assert.equal(await pending,'AbortError');
  await new Promise(resolve=>setImmediate(resolve));assert.equal(f.created.length,0);
});
test('null encoding result reports failure and allows a retry',async()=>{
  const f=fixture(),first=f.start();f.callbacks[0](null);await assert.rejects(first,/PNG encoding failed/);
  const retry=f.start();f.callbacks[1](png());await retry;f.store.clear();
});
test('throwing encoder is rejected without retained URLs',async()=>{
  const f=fixture();f.canvas.toBlob=()=>{throw new DOMException('Not origin clean','SecurityError');};
  await assert.rejects(f.start(),{name:'SecurityError'});assert.equal(f.created.length,0);
});
test('stalled encoding has a bounded deadline and late results are ignored',async()=>{
  const f=fixture({timeoutMs:10}),pending=f.start();await assert.rejects(pending,{name:'TimeoutError'});
  f.callbacks[0](png());assert.equal(f.created.length,0);
});
test('surface bounds and malformed identities fail before encoder invocation',async()=>{
  for(const dimensions of [[0,2],[2,0],[4097,1],[1025,1025],[NaN,2],[2.5,2]]){
    const f=fixture();[f.canvas.width,f.canvas.height]=dimensions;await assert.rejects(f.start(),/capture/i);assert.equal(f.callbacks.length,0);
  }
  for(const identity of [1,'0','01','1e2','18446744073709551616',null]){
    const f=fixture();await assert.rejects(f.start(identity),/generation/);assert.equal(f.callbacks.length,0);
  }
});
test('unbounded or non-object metadata is rejected before encoder invocation',async()=>{
  for(const value of ['{',null,'[]','null','"metadata"','x'.repeat(65537)]){
    const f=fixture();await assert.rejects(f.start('1',value),/metadata/i);assert.equal(f.callbacks.length,0);
  }
});
test('PNG MIME, signature, declared dimensions and byte budget are enforced',async()=>{
  const wrongSize=Buffer.from(PNG);wrongSize.writeUInt32BE(3,16);
  for(const blob of [new Blob([PNG],{type:'image/jpeg'}),new Blob(['not a PNG'],{type:'image/png'}),new Blob([wrongSize],{type:'image/png'}),new Blob([new Uint8Array(8*1024*1024+1)],{type:'image/png'})]){
    const f=fixture(),pending=f.start();f.callbacks[0](blob);await assert.rejects(pending,/PNG/);assert.equal(f.created.length,0);
  }
});
test('partial object URL creation failure releases the first URL',async()=>{
  const revoked=[];let count=0;
  const f=fixture({urlApi:{createObjectURL(){if(count++)throw new Error('URL budget');return 'blob:first';},revokeObjectURL(url){revoked.push(url);}}});
  const pending=f.start();f.callbacks[0](png());await assert.rejects(pending,/URL budget/);assert.deepEqual(revoked,['blob:first']);
});
test('repeated capture and replacement keeps only one pair of URLs alive',async()=>{
  const f=fixture();for(let i=1;i<=20;i++){const pending=f.start(String(i));f.callbacks[i-1](png());await pending;assert.equal(f.created.length-f.revoked.length,2);}
  f.store.clear();assert.equal(f.created.length,f.revoked.length);
});
test('cancelled browser encoders remain bounded until their actual callbacks finish',async()=>{
  const f=fixture();const first=f.start().catch(e=>e.name),second=f.start('2').catch(e=>e.name);
  await assert.rejects(f.start('3'),/encoder.*busy/i);assert.equal(await first,'AbortError');assert.equal(await second,'AbortError');
  assert.equal(f.callbacks.length,2);f.callbacks[0](png());
  const retry=f.start('4');f.callbacks[2](png());assert.equal(JSON.parse(await retry).generation,'4');
  f.callbacks[1](png());f.store.clear();
});
