import test from "node:test";
import assert from "node:assert/strict";
import {privateEodRequest,authenticatedEod} from "./eod.mjs";
const message={schema_version:1,source_session_id:"2",sequence:"1",event:"Timer_Start",binary:false,payload_hex:""};
test("private route rejects player-provided owner, ticket and malformed payload",()=>{
  for(const extra of [{control:{}},{actor:"1"},{ticket:"x"}]) assert.throws(()=>privateEodRequest({type:"native_eod_ui",message,...extra}));
  for(const bad of [{sequence:"0"},{source_session_id:"02"},{payload_hex:"FF"},{binary:1},{event:"a\n"}]) assert.throws(()=>privateEodRequest({type:"native_eod_ui",message:{...message,...bad}}));
});
test("private input injects authenticated control and publishes only after commit",async()=>{
  const owner={principal:"19"},order=[],sent=[];
  await authenticatedEod({type:"native_eod_ui",message},{control:owner},{
    revalidate:async()=>order.push("auth"),prepare:async(kind,extra)=>{order.push("prepare");assert.equal(kind,"prepare_eod_ui");assert.equal(extra.control,owner);return{}},
    commit:async()=>{order.push("commit");return{kind:"committed"}},read:async(control)=>{order.push("read");assert.equal(control,owner);return{kind:"private_eod_outputs",messages:[{output_id:"4"}]}},send:(v)=>sent.push(v),
  });
  assert.deepEqual(order,["auth","prepare","commit","read"]);assert.equal(sent.length,1);
});
test("rejection and unauthenticated input cannot emit private output",async()=>{
  for(const fail of ["auth","commit"]) {
    let sent=0,read=0;
    await assert.rejects(authenticatedEod({type:"native_eod_ui",message},{control:{}},{revalidate:async()=>{if(fail==="auth")throw Error("revoked")},prepare:async()=>({}),commit:async()=>({kind:"rejected"}),read:async()=>{read++;},send:()=>sent++}));
    assert.equal(read,0);assert.equal(sent,0);
  }
});
test("read/redelivery never calls commit or manufactures acknowledgement",async()=>{
  let reads=0,sent=0;
  const output={kind:"private_eod_outputs",messages:[{output_id:"7",sha256:"a".repeat(64)}]};
  const capabilities={revalidate:async()=>{},prepare:()=>assert.fail(),commit:()=>assert.fail(),read:async()=>{reads++;return output},send:()=>sent++};
  await authenticatedEod({type:"native_eod_read"},{control:{}},capabilities);
  await authenticatedEod({type:"native_eod_read"},{control:{}},capabilities);
  assert.equal(reads,2);assert.equal(sent,2);
});
