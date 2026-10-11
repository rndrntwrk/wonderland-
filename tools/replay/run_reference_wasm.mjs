#!/usr/bin/env node
// Execute the actual ordinary (non-WASI) WASM probe. No imports or JS simulation.
import assert from 'node:assert/strict';
import {readFile,writeFile,lstat} from 'node:fs/promises';
import {createHash} from 'node:crypto';

const [input,output,...extra]=process.argv.slice(2);
assert.ok(input&&output&&!extra.length,'Usage: run_reference_wasm.mjs probe.wasm new-trace.tsv');
const info=await lstat(input);
assert.ok(info.isFile()&&!info.isSymbolicLink()&&info.size>0&&info.size<=16*1024*1024,'Module input limit');
const bytes=await readFile(input);
assert.ok(bytes.length<=16*1024*1024,'Module grew past input limit');
const module=await WebAssembly.compile(bytes);
assert.deepEqual(WebAssembly.Module.imports(module),[],'Reference WASM must not import host behavior');
const decoder=new TextDecoder('utf-8',{fatal:true});
function digest(data){return createHash('sha256').update(data).digest('hex');}
async function execute(){
  const instance=await WebAssembly.instantiate(module,{});
  const {memory,reference_ptr:ptr,reference_len:len}=instance.exports;
  assert.ok(memory instanceof WebAssembly.Memory);
  assert.equal(typeof ptr,'function');assert.equal(typeof len,'function');
  const start=ptr()>>>0,size=len()>>>0;
  assert.ok(size>0&&size<=4*1024*1024,'WASM output size limit');
  assert.ok(!(memory.buffer instanceof SharedArrayBuffer),'Shared output memory not admitted');
  assert.ok(start<=memory.buffer.byteLength&&size<=memory.buffer.byteLength-start,'WASM output bounds');
  const output=Buffer.from(new Uint8Array(memory.buffer,start,size));
  decoder.decode(output);
  for(let index=0;index<3;index++){
    assert.equal(ptr()>>>0,start);assert.equal(len()>>>0,size);
    assert.deepEqual(Buffer.from(new Uint8Array(memory.buffer,start,size)),output,'Read advanced or rewrote the scenario');
  }
  return {output,memory_bytes:memory.buffer.byteLength};
}
const a=await execute(),b=await execute();
assert.deepEqual(a.output,b.output,'Independent WASM instances diverged');
await writeFile(output,a.output,{flag:'wx',mode:0o600});
console.log(JSON.stringify({schema:1,wasm_sha256:digest(bytes),output_sha256:digest(a.output),
  output_bytes:a.output.length,host_imports:0,shared_memory:false,independent_instances:2,
  repeated_reads_per_instance:3,memory_bytes:a.memory_bytes,full_original_engine:'not-tested'},null,2));
