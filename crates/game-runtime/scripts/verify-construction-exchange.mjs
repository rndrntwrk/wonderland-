// Execute the real Rust client/session/replica exchange in ordinary WASM, then compare complete output.
import assert from 'node:assert/strict';
import {readFile,stat} from 'node:fs/promises';
import {createHash} from 'node:crypto';
const [nativePath,wasmPath,...extra]=process.argv.slice(2);
assert.ok(nativePath && wasmPath && !extra.length,'Usage: verify-construction-exchange.mjs native.json probe.wasm');
async function bounded(path,max){const s=await stat(path);assert.ok(s.isFile()&&s.size>0&&s.size<=max);const b=await readFile(path);assert.ok(b.length<=max);return b;}
const utf8=new TextDecoder('utf-8',{fatal:true});
const native=utf8.decode(await bounded(nativePath,1024*1024)).replace(/\r?\n$/,'');
const wasmBytes=await bounded(wasmPath,32*1024*1024);
const module=await WebAssembly.compile(wasmBytes);assert.deepEqual(WebAssembly.Module.imports(module),[]);
const {exports:e}=await WebAssembly.instantiate(module,{});
const pointer=e.construction_probe_ptr()>>>0,length=e.construction_probe_len()>>>0;
assert.ok(e.memory instanceof WebAssembly.Memory && !(e.memory.buffer instanceof SharedArrayBuffer));
assert.ok(length>0&&length<=1024*1024&&pointer<=e.memory.buffer.byteLength&&length<=e.memory.buffer.byteLength-pointer);
const wasm=utf8.decode(new Uint8Array(e.memory.buffer,pointer,length));
function validate(text){
 const d=JSON.parse(text);assert.equal(d.schema,1);assert.equal(d.fixture,'declared-source-construction');
 assert.equal(d.request_id,'9007199254740993');assert.equal(d.operation,'18446744073709551601');
 assert.equal(d.cost,7);assert.equal(d.floor_before,0);assert.equal(d.floor_pending,0);assert.equal(d.floor_after,1);
 assert.equal(d.durable_requests,1);assert.equal(d.duplicate_admissions,0);assert.equal(d.unknown_confirmation_blocked,true);
 assert.equal(d.final_stage,'Committed');assert.equal(d.replica_equal,true);assert.equal(d.quoted_state_unchanged,true);
 for(const name of ['before_hash','pending_hash','committed_hash'])assert.match(d[name],/^[0-9a-f]{64}$/);
 assert.notEqual(d.before_hash,d.pending_hash);assert.notEqual(d.pending_hash,d.committed_hash);
 for(const name of ['quote_call_hex','quote_reply_hex','confirm_call_hex','status_call_hex','status_reply_hex']){
  assert.match(d[name],/^(?:[0-9a-f]{2})+$/);const b=Buffer.from(d[name],'hex');
  assert.ok(b.length>=16);assert.equal(b.readBigUInt64LE(8),BigInt(b.length-16));
  const magic=name.includes('reply')?'574c4252010d0a1a':'574c4251010d0a1a';assert.equal(b.subarray(0,8).toString('hex'),magic);
 }
}
function compare(a,b){validate(a);validate(b);assert.equal(a,b,'Full native/WASM records differ');}
compare(native,wasm);
assert.equal(e.construction_probe_ptr()>>>0,pointer);assert.equal(e.construction_probe_len()>>>0,length);
const mutations=[d=>{d.floor_pending=1;},d=>{d.cost=0;},d=>{d.durable_requests=2;},d=>{d.unknown_confirmation_blocked=false;},d=>{d.request_id='9007199254740992';},d=>{d.final_stage='Pending';},d=>{d.status_call_hex='00'+d.status_call_hex.slice(2);}];
for(const change of mutations){const bad=JSON.parse(native);change(bad);const text=JSON.stringify(bad);assert.throws(()=>compare(text,text));}
assert.throws(()=>compare(native,wasm+' '));
console.log(JSON.stringify({result:'pass',complete_record_bytes:length,native_wasm_equal:true,wasm_imports:0,shared_memory:false,
 negative_controls:mutations.length+1,output_sha256:createHash('sha256').update(wasm).digest('hex'),wasm_sha256:createHash('sha256').update(wasmBytes).digest('hex')},null,2));
