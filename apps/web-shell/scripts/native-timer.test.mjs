import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createNativeTimer} from '../public/native-timer.mjs';
const sha=v=>createHash('sha256').update(v).digest('hex');
function output(id,event,binary,payload_hex,source_session_id='7'){const v={source_session_id,output_id:id,event,binary,payload_hex};
 const canonical=Object.fromEntries(Object.keys(v).sort().map(k=>[k,v[k]]));return {...v,plugin_id:0xaa65fe9e,sha256:sha(JSON.stringify(canonical))};}
const show=()=>output('2','Timer_Show',true,'00010107');
function response({messages=[],next='1',last=null,accepted=null,closed=false}={}){return {version:1,state:'native_eod',type:'native_eod_outputs',lot_id:'41',lot_incarnation:'1',viewer_id:'9',
 source_sessions:[{source_session_id:'7',plugin_id:0xaa65fe9e,closed,next_ui_sequence:next,last_ui_receipt:last}],messages,accepted_ui:accepted};}
test('only verified original private Show opens Timer; duplicate outputs do not create UI input',async()=>{
 let view,writes=0,acks=0;const c=createNativeTimer(async q=>{if(q.type==='native_eod_ack'){acks++;return response();}if(q.type==='native_eod_ui')writes++;return response({messages:[show()]});},v=>view=v);
 c.setLive(true);await c.refresh();assert.equal(view.visible,true);assert.equal(view.minutes,1);assert.equal(view.seconds,7);assert.equal(view.mode,1);
 await c.refresh();assert.equal(writes,0);assert.equal(acks,2);c.dispose();
});
test('receipt cursor supplies next sequence; lost reply retries exact control without another intent',async()=>{
 let view,pending,attempts=0;const sent=[];let initial=true;
 const c=createNativeTimer(async q=>{if(q.type==='native_eod_ack')return response();
 if(q.type==='native_eod_read'){if(initial){initial=false;return response({messages:[show()],next:'4',last:{sequence:'3',sha256:'a'.repeat(64)}});}return response();}
 sent.push(structuredClone(q));pending=q.message;attempts++;if(attempts===1)throw Error('lost committed reply');
 return response({next:'5',last:{sequence:'4',sha256:sha(JSON.stringify(pending))},accepted:pending});},v=>view=v);
 c.setLive(true);await c.refresh();await c.send('Timer_IsRunning_Change',true,'01');assert.equal(view.unknown,true);
 await c.retry();assert.equal(attempts,2);assert.deepEqual(sent[0],sent[1]);assert.equal(sent[0].message.sequence,'4');assert.equal(view.running,true);assert.equal(view.unknown,false);c.dispose();
});
test('bad private digest and truncated Show are never acknowledged or rendered',async()=>{
 for(const value of [{...show(),sha256:'0'.repeat(64)},output('2','Timer_Show',true,'000101')]){
 let view,acks=0;const c=createNativeTimer(async q=>{if(q.type==='native_eod_ack')acks++;return response({messages:[value]});},v=>view=v);
 c.setLive(true);await c.refresh();assert.equal(view.visible,false);assert.equal(acks,0);c.dispose();}
});
test('private Update generates original Set feedback once per immutable output',async()=>{
 const update=output('3','Timer_Update',false,Buffer.from('1:8').toString('hex'));let sent=0,view;
 const c=createNativeTimer(async q=>{if(q.type==='native_eod_ack')return response();
 if(q.type==='native_eod_ui'){sent++;assert.equal(q.message.event,'Timer_Set');assert.equal(q.message.payload_hex,'0108');return response({next:'2',last:{sequence:'1',sha256:sha(JSON.stringify(q.message))},accepted:q.message});}
 return response({messages:[show(),update],next:sent?'2':'1',last:sent?{sequence:'1',sha256:'a'.repeat(64)}:null});},v=>view=v);
 c.setLive(true);assert.equal(await c.refresh(),true);assert.equal(sent,1);assert.equal(view.unknown,false);assert.equal(view.ready,true);await c.refresh();assert.equal(sent,1);assert.equal(view.seconds,8);c.dispose();
});
test('source leave closes panel; empty peer projection never opens one',async()=>{
 let view,leave=false;const c=createNativeTimer(async q=>q.type==='native_eod_ack'?response():response({closed:leave,messages:leave?[output('3','eod_leave',false,'')]:[show()]}),v=>view=v);
 c.setLive(true);await c.refresh();leave=true;await c.refresh();assert.equal(view.visible,false);c.dispose();
 const peer=createNativeTimer(async()=>({...response(),source_sessions:[]}),v=>view=v);peer.setLive(true);await peer.refresh();assert.equal(view.visible,false);peer.dispose();
});
test('source receipt reconciles an unknown control without resending it',async()=>{
 let view,saved,writes=0;
 const c=createNativeTimer(async q=>{
  if(q.type==='native_eod_ack')return response();
  if(q.type==='native_eod_ui'){writes++;saved=q.message;throw Error('lost after commit');}
  return saved?response({next:'2',last:{sequence:'1',sha256:sha(JSON.stringify(saved))}}):response({messages:[show()]});
 },v=>view=v);
 c.setLive(true);await c.refresh();await c.send('Timer_Set',true,'0203');assert.equal(view.unknown,true);
 await c.refresh();assert.equal(writes,1);assert.equal(view.unknown,false);assert.equal(view.minutes,2);assert.equal(view.seconds,3);c.dispose();
});
test('out-of-scope and malformed controls cannot cause private acknowledgements or writes',async()=>{
 let view,writes=0,acks=0,value=response({messages:[show()]});
 const c=createNativeTimer(async q=>{if(q.type==='native_eod_ui')writes++;if(q.type==='native_eod_ack'){acks++;return response();}return value;},v=>view=v);
 c.setLive(true);await c.refresh();
 for(const args of [['Timer_Set',true,'6400'],['Timer_Set',true,'003c'],['Timer_Close',true,''],['Unknown',false,'']])assert.equal(await c.send(...args),false);
 value={...response({messages:[{...show(),output_id:'3'}]}),lot_id:'42'};await c.refresh();
 assert.equal(view.ready,false);assert.equal(acks,1);assert.equal(writes,0);c.dispose();
});
test('disposed private reads cannot reopen a panel or acknowledge a late source output',async()=>{
 let resolve,acks=0,shown=0;const c=createNativeTimer(q=>{if(q.type==='native_eod_ack')acks++;return new Promise(r=>resolve=r);},v=>{if(v.visible)shown++;});
 c.setLive(true);const work=c.refresh();c.dispose();resolve(response({messages:[show()]}));await work;
 assert.equal(acks,0);assert.equal(shown,0);
});
test('retry of an older closed session cannot change a newer Timer window',async()=>{
 let view,pending,reads=0,writes=0;
 const newer=()=>({...response({messages:reads===2?[output('4','Timer_Show',true,'00000203','8')]:[]}),
   source_sessions:[{source_session_id:'8',plugin_id:0xaa65fe9e,closed:false,next_ui_sequence:'1',last_ui_receipt:null}]});
 const c=createNativeTimer(async q=>{
   if(q.type==='native_eod_ack')return response();
   if(q.type==='native_eod_read'){reads++;return reads===1?response({messages:[show()]}):newer();}
   pending=q.message;if(++writes===1)throw Error('lost original receipt');return {...newer(),accepted_ui:pending};
 },v=>view=v);
 c.setLive(true);await c.refresh();await c.send('Timer_IsRunning_Change',true,'01');await c.refresh();
 assert.equal(view.source_session_id,'8');assert.equal(view.running,false);
 await c.retry();assert.equal(view.unknown,false);assert.equal(view.source_session_id,'8');assert.equal(view.running,false);assert.equal(view.minutes,2);c.dispose();
});
