import test from 'node:test';
import assert from 'node:assert/strict';
import {dispatchSupervisorRpc} from '../src/hosted/supervisor-rpc.ts';

const call=(target,method,args=[])=>({target,method,args});
function fixture(result={ok:true}) {
 const calls=[];
 return {calls,dispatch:async (...args)=>{calls.push(args);return result;}};
}
test('private account and lot calls retain target/method/argument identity',async()=>{
 for(const [target,method] of [['account','roster'],['lot','getBatch'],['lot','ownedLotChatV1']]){
  const f=fixture({marker:17});const response=await dispatchSupervisorRpc(call(target,method,['one',7]),f.dispatch);
  assert.equal(response.status,200);assert.deepEqual(await response.json(),{value:{marker:17}});
  assert.deepEqual(f.calls,[[target,method,['one',7]]]);
 }
});
test('unknown targets, method crossing and malformed calls fail before binding invocation',async()=>{
 const values=[null,[],{},call('public','register'),call('account','commitControlledBatchV2'),call('lot','register'),call('account','ownedLotChatV1'),call('lot','constructor'),call('lot','getBatch',{}),call('lot','getBatch',Array(9).fill(null))];
 for(const body of values){const f=fixture();const r=await dispatchSupervisorRpc(body,f.dispatch);assert.equal(r.status,400);assert.deepEqual(f.calls,[]);}
});
test('checkpoint conversion stays at the existing byte position and does not mutate caller data',async()=>{
 for(const [method,index] of [['publishControlledCheckpointV2',3],['finishControlledRecoveryV2',1]]){
  const args=Array(index+1).fill(null);args[index]='0001feff';const f=fixture();
  const r=await dispatchSupervisorRpc(call('lot',method,args),f.dispatch);
  assert.equal(r.status,200);assert.deepEqual([...f.calls[0][2][index]],[0,1,254,255]);assert.equal(args[index],'0001feff');
 }
});
test('invalid and oversized checkpoint bytes remain fail-closed',async()=>{
 for(const value of ['', 'AABB','abc','zz',new Uint8Array([1]),'00'.repeat(8*1024*1024+1)]){
  const f=fixture();const r=await dispatchSupervisorRpc(call('lot','finishControlledRecoveryV2',[{},value]),f.dispatch);
  assert.equal(r.status,400);assert.equal(f.calls.length,0);
 }
});
test('recovery response preserves metadata and converts only typed checkpoint bytes',async()=>{
 const f=fixture({checkpoint_bytes:new Uint8Array([0,1,254,255]),cursor:{tick:'19'},tail:[{id:'7'}]});
 const r=await dispatchSupervisorRpc(call('lot','controlledRecoveryBundleV2',['1']),f.dispatch);
 assert.deepEqual(await r.json(),{value:{cursor:{tick:'19'},tail:[{id:'7'}],checkpoint_hex:'0001feff'}});
 const bad=await dispatchSupervisorRpc(call('lot','controlledRecoveryBundleV2'),fixture({checkpoint_bytes:[1]}).dispatch);assert.equal(bad.status,400);
});
test('missing export retains its typed failure and never retries or fabricates a result',async()=>{
 let count=0;const r=await dispatchSupervisorRpc(call('account','getReservedAvatarExportV2'),async()=>{count++;throw Error('owned admission export unavailable');});
 assert.equal(r.status,400);assert.equal((await r.json()).code,'owned_export_unavailable');assert.equal(count,1);
});
