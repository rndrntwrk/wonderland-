import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createNativeAccountProvider} from '../public/native-account.mjs';
const avatars=[{avatar_id:'7',name:'First Traveller'}];
const destinations=[{lot_id:'11',lot_location:16777472,name:'Shared Garden'}];
const binding={source_epoch:'3',lot_incarnation:'4',lot_location:16777472,avatar_id:7};
const grant=()=>({version:1,state:'lot_ready',binding:{...binding},ticket:'b'.repeat(48)});
function setup(handler=()=>Response.json(grant())) {
 let latest; const calls=[];
 const provider=createNativeAccountProvider('/native-account',v=>latest=v,{origin:'http://127.0.0.1:18888',nativeLots:true,operationId:()=> 'owned_operation_0001',fetch:async(url,options)=>{
  const action=url.split('/').at(-1);calls.push({action,body:JSON.parse(options.body),signal:options.signal});
  if(action==='login')return Response.json({version:1,state:'account_ready',admission_available:true,token:'a'.repeat(64)});
  if(action==='roster')return Response.json({version:1,state:'account_ready',admission_available:true,account_id:'1',avatars,destinations,next:null});
  if(action==='logout')return Response.json({signed_out:true});
  return handler(action,options);
 }});return {provider,calls,get latest(){return latest;}};
}
test('owned entry submits only selection and exposes no bearer or actor grant before committed response',async()=>{
 let resolve;const gate=new Promise(r=>resolve=r);const s=setup(()=>gate);
 try{await s.provider.login('owner','secret');assert.deepEqual(s.latest.destinations,destinations);const enter=s.provider.enter('7','11');assert.equal(s.latest.state,'lot_entering');assert.equal(s.latest.grant,undefined);assert.deepEqual(s.calls.at(-1).body,{avatar_id:'7',lot_id:'11',operation:'owned_operation_0001'});resolve(Response.json(grant()));await enter;assert.equal(s.latest.state,'lot_ready');assert.deepEqual(s.latest.grant.binding,binding);assert.equal(JSON.stringify(s.latest).includes('a'.repeat(64)),false);assert.equal('ticket' in s.latest.grant,false);const ticket=await s.provider.socketGrant();assert.equal(ticket.ticket,'b'.repeat(48));assert.deepEqual(ticket.binding,binding);}finally{resolve?.(Response.json(grant()));s.provider.dispose();}
});
test('cross-avatar admission is refused and unknown enter reuses the same operation',async()=>{
 let attempts=0;const s=setup(()=>{attempts++;if(attempts===1)throw Error('lost reply');const value=grant();if(attempts===2)value.binding.avatar_id=8;return Response.json(value);});
 try{await s.provider.login('owner','secret');await s.provider.enter('7','11');assert.equal(s.latest.state,'account_error');await s.provider.enter('7','11');assert.equal(s.latest.state,'account_error');await s.provider.enter('7','11');assert.equal(s.latest.state,'lot_ready');assert.equal(new Set(s.calls.filter(c=>c.action==='enter').map(c=>c.body.operation)).size,1);}finally{s.provider.dispose();}
});
test('sign-out during admission rejects late success and never returns a socket ticket',async()=>{
 let resolve;const gate=new Promise(r=>resolve=r);const s=setup(()=>gate);
 try{await s.provider.login('owner','secret');const enter=s.provider.enter('7','11');await s.provider.logout();resolve(Response.json(grant()));await enter;assert.equal(s.latest.state,'signed_out');await assert.rejects(s.provider.socketGrant());}finally{resolve?.(Response.json(grant()));s.provider.dispose();}
});
test('unconfirmed leave remains explicitly pending and cannot admit another actor',async()=>{
 const s=setup(action=>action==='leave'?new Response(null,{status:503}):Response.json(grant()));
 try{await s.provider.login('owner','secret');await s.provider.enter('7','11');await s.provider.leave();assert.equal(s.latest.state,'detach_pending');assert.equal(s.latest.grant,undefined);const before=s.calls.length;await s.provider.enter('7','11');assert.equal(s.calls.length,before);await assert.rejects(s.provider.socketGrant());}finally{s.provider.dispose();}
});

test('authenticated recovery refreshes only a monotonic incarnation and retains a one-use fresh bootstrap ticket',async()=>{
 const s=setup(action=>{const value=grant();if(action==='reconnect')value.binding.lot_incarnation='5';return Response.json(value);});
 try{await s.provider.login('owner','secret');await s.provider.enter('7','11');await s.provider.socketGrant();await assert.rejects(s.provider.socketGrant(),/recovered/);assert.equal(s.latest.state,'lot_ready');assert.equal(s.latest.grant.binding.lot_incarnation,'5');const ticket=await s.provider.socketGrant();assert.equal(ticket.binding.lot_incarnation,'5');assert.equal(s.calls.filter(c=>c.action==='reconnect').length,1);}finally{s.provider.dispose();}
});
test('recovery cannot change the control generation or roll incarnation backward',async()=>{
 for(const changed of [{lot_incarnation:'3'},{source_epoch:'4'}]){
  const s=setup(action=>{const value=grant();if(action==='reconnect')Object.assign(value.binding,changed);return Response.json(value);});
  try{await s.provider.login('owner','secret');await s.provider.enter('7','11');await s.provider.socketGrant();await assert.rejects(s.provider.socketGrant());assert.deepEqual(s.latest.grant.binding,binding);}finally{s.provider.dispose();}
 }
});

test('private recovery scope survives only the same authenticated entry',async()=>{
 const s=setup(action=>{const value=grant();if(action==='reconnect')value.binding.lot_incarnation='5';return Response.json(value);});
 try {
  await s.provider.login('owner','secret');await s.provider.enter('7','11');
  const original=s.latest.grant.recovery_scope;assert.match(original,/^[1-9]\d*$/);
  await s.provider.socketGrant();await assert.rejects(s.provider.socketGrant(),/recovered/);
  assert.equal(s.latest.grant.recovery_scope,original);
  await s.provider.logout();assert.equal(s.latest.grant,undefined);
  await s.provider.login('other owner','different');await s.provider.enter('7','11');
  assert.notEqual(s.latest.grant.recovery_scope,original);
 } finally{s.provider.dispose();}
});
test('late recovered grant cannot restore a signed-out recovery scope',async()=>{
 let resolve;const gate=new Promise(r=>resolve=r);
 const s=setup(action=>action==='reconnect'?gate:Response.json(grant()));
 try{
  await s.provider.login('owner','secret');await s.provider.enter('7','11');await s.provider.socketGrant();
  const reconnect=s.provider.socketGrant();const denied=assert.rejects(reconnect);
  await s.provider.logout();const value=grant();value.binding.lot_incarnation='5';resolve(Response.json(value));
  await denied;assert.equal(s.latest.state,'signed_out');assert.equal(s.latest.grant,undefined);
 }finally{resolve?.(Response.json(grant()));s.provider.dispose();}
});
