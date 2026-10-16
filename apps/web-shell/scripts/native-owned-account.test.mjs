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

test('owned leave polls the same admission until durable release and never sends save data',async()=>{
 let attempts=0;const s=setup(action=>action==='leave'?Response.json(++attempts<2?{version:1,state:'detach_pending',status:'leaving',avatar_id:'7'}:{version:1,state:'detached',status:'released',avatar_id:'7'}):Response.json(grant()));
 try{await s.provider.login('owner','secret');await s.provider.enter('7','11');await s.provider.leave();assert.equal(s.latest.state,'account_ready');const calls=s.calls.filter(x=>x.action==='leave');assert.equal(calls.length,2);assert.deepEqual(calls[0].body,calls[1].body);assert.deepEqual(Object.keys(calls[0].body).sort(),['avatar_id','lot_id','operation']);await assert.rejects(s.provider.socketGrant());}finally{s.provider.dispose();}
});
test('source blocked departure stays reserved and does not masquerade as successful leave',async()=>{
 const s=setup(action=>action==='leave'?Response.json({version:1,state:'detach_pending',status:'blocked',avatar_id:'7',blocker:'unsupported_source'}):Response.json(grant()));
 try{await s.provider.login('owner','secret');await s.provider.enter('7','11');await s.provider.leave();assert.equal(s.latest.state,'detach_pending');assert.equal(s.calls.filter(x=>x.action==='leave').length,1);await assert.rejects(s.provider.socketGrant());}finally{s.provider.dispose();}
});

test('committed admission awaiting placement retains selection and never yields a ticket or second admission',async()=>{
 let ready=false;const waiting=()=>({version:1,state:'admitted_awaiting_placement',binding:{...binding},blocker:'source_placement_unavailable'});
 const s=setup(action=>Response.json(ready?grant():waiting()));
 try{await s.provider.login('owner','secret');await s.provider.enter('7','11');assert.equal(s.latest.state,'admitted_awaiting_placement');assert.deepEqual(s.latest.grant.binding,binding);assert.equal(s.latest.grant.ticket,undefined);await assert.rejects(s.provider.socketGrant(),/awaiting source placement/);await s.provider.enter('7','11');assert.equal(s.calls.filter(c=>c.action==='enter').length,1);ready=true;await s.provider.retryPlacement();assert.equal(s.latest.state,'lot_ready');assert.equal((await s.provider.socketGrant()).ticket,'b'.repeat(48));assert.equal(s.calls.filter(c=>c.action==='enter').length,1);}finally{s.provider.dispose();}
});
test('presence placement loss hides the lot and requires exact reconnect proof',async()=>{
 const s=setup(()=>Response.json({version:1,state:'admitted_awaiting_placement',binding:{...binding},blocker:'source_placement_unavailable'}));
 try{await s.provider.login('owner','secret');await s.provider.enter('7','11');s.provider.placementUnavailable();assert.equal(s.latest.state,'admitted_awaiting_placement');await s.provider.retryPlacement();assert.equal(s.latest.state,'admitted_awaiting_placement');assert.deepEqual(s.latest.grant.binding,binding);assert.equal(s.latest.grant.ticket,undefined);}finally{s.provider.dispose();}
});
test('awaiting placement rejects ticket-bearing or cross-avatar replies',async()=>{
 for(const bad of ['ticket','avatar']){
 const s=setup(()=>{const v={version:1,state:'admitted_awaiting_placement',binding:{...binding},blocker:'source_placement_unavailable'};if(bad==='ticket')v.ticket='b'.repeat(48);else v.binding.avatar_id=8;return Response.json(v)});
 try{await s.provider.login('owner','secret');await s.provider.enter('7','11');assert.equal(s.latest.state,'account_error');await assert.rejects(s.provider.socketGrant());}finally{s.provider.dispose();}
 }
});
test('awaiting placement may advance host incarnation without losing the newly verified ready state',async()=>{
 let first=true;const s=setup(()=>{if(first){first=false;return Response.json({version:1,state:'admitted_awaiting_placement',binding:{...binding},blocker:'source_placement_unavailable'})}const v=grant();v.binding.lot_incarnation='5';return Response.json(v)});
 try{await s.provider.login('owner','secret');await s.provider.enter('7','11');await s.provider.retryPlacement();assert.equal(s.latest.state,'lot_ready');assert.equal((await s.provider.socketGrant()).binding.lot_incarnation,'5');}finally{s.provider.dispose();}
});
test('placement suspension retains exact binding and recovery scope for same and newer host',async()=>{
 for(const incarnation of ['4','5']){
  const s=setup(action=>{const v=grant();if(action==='reconnect')v.binding.lot_incarnation=incarnation;return Response.json(v)});
  try{await s.provider.login('owner','secret');await s.provider.enter('7','11');const before=s.latest.grant;await s.provider.socketGrant();s.provider.placementUnavailable();assert.deepEqual(s.latest.grant,before);assert.equal(s.latest.state,'admitted_awaiting_placement');await s.provider.retryPlacement();assert.equal(s.latest.state,'lot_ready');assert.equal(s.latest.grant.recovery_scope,before.recovery_scope);assert.equal(s.calls.filter(c=>c.action==='enter').length,1);}finally{s.provider.dispose();}
 }
});
test('delayed reconnect grant cannot reopen transport after placement suspension',async()=>{
 let complete;const delayed=new Promise(resolve=>complete=resolve);const s=setup(action=>action==='reconnect'?delayed:Response.json(grant()));
 try{await s.provider.login('owner','secret');await s.provider.enter('7','11');await s.provider.socketGrant();const pending=s.provider.socketGrant();s.provider.placementUnavailable();const value=grant();value.binding.lot_incarnation='5';complete(Response.json(value));await assert.rejects(pending,/connection changed/);assert.equal(s.latest.state,'admitted_awaiting_placement');assert.equal(s.latest.grant.binding.lot_incarnation,'4');}finally{complete?.(Response.json(grant()));s.provider.dispose();}
});

test('repeated placement retry preserves the in-flight reconnect and accepts its exact grant',async()=>{
 let complete;const delayed=new Promise(resolve=>complete=resolve);
 const waiting=()=>({version:1,state:'admitted_awaiting_placement',binding:{...binding},blocker:'source_placement_unavailable'});
 const s=setup(action=>action==='reconnect'?delayed:Response.json(waiting()));
 try{
  await s.provider.login('owner','secret');await s.provider.enter('7','11');
  const first=s.provider.retryPlacement();
  const reconnect=s.calls.find(c=>c.action==='reconnect');
  assert.ok(reconnect);
  await s.provider.retryPlacement();
  assert.equal(reconnect.signal.aborted,false,'a repeated click must not cancel the admitted Sim reconnect');
  assert.equal(s.calls.filter(c=>c.action==='reconnect').length,1);
  complete(Response.json(grant()));await first;
  assert.equal(s.latest.state,'lot_ready');
  assert.deepEqual((await s.provider.socketGrant()).binding,binding);
  assert.equal(s.calls.filter(c=>c.action==='enter').length,1);
 }finally{complete?.(Response.json(grant()));s.provider.dispose();}
});

test('failed placement reconnect releases its retry guard without requesting a new actor',async()=>{
 let attempts=0;
 const s=setup(action=>{
  if(action==='enter')return Response.json({version:1,state:'admitted_awaiting_placement',binding:{...binding},blocker:'source_placement_unavailable'});
  if(++attempts===1)throw Error('connection lost');
  return Response.json(grant());
 });
 try{
  await s.provider.login('owner','secret');await s.provider.enter('7','11');
  await s.provider.retryPlacement();assert.equal(s.latest.state,'admitted_awaiting_placement');
  await s.provider.retryPlacement();assert.equal(s.latest.state,'lot_ready');
  assert.equal(s.calls.filter(c=>c.action==='reconnect').length,2);
  assert.equal(s.calls.filter(c=>c.action==='enter').length,1);
  assert.deepEqual((await s.provider.socketGrant()).binding,binding);
 }finally{s.provider.dispose();}
});

test('stale placement retry cannot cancel a newer retry after account or placement scope changes',async()=>{
 for(const change of ['account','placement']){
  const completions=[];
  const s=setup(action=>action==='enter'?Response.json({version:1,state:'admitted_awaiting_placement',binding:{...binding},blocker:'source_placement_unavailable'}):new Promise(resolve=>completions.push(resolve)));
  try{
   await s.provider.login('owner','secret');await s.provider.enter('7','11');
   const old=s.provider.retryPlacement();
   if(change==='account'){
    await s.provider.logout();await s.provider.login('other','secret');await s.provider.enter('7','11');
   }else s.provider.placementUnavailable();
   const scope=s.latest.grant.recovery_scope;
   const fresh=s.provider.retryPlacement();
   const reconnect=s.calls.filter(c=>c.action==='reconnect').at(-1);
   completions[0](Response.json(grant()));await old;
   assert.equal(reconnect.signal.aborted,false,`${change}: obsolete retry must not abort newer reconnect`);
   assert.equal(s.latest.grant.recovery_scope,scope);
   completions[1](Response.json(grant()));await fresh;
   assert.equal(s.latest.state,'lot_ready');
   assert.deepEqual((await s.provider.socketGrant()).binding,binding);
  }finally{for(const complete of completions)complete(Response.json(grant()));s.provider.dispose();}
 }
});
