import test from 'node:test';
import assert from 'node:assert/strict';
import {createNativeLotChat} from '../public/native-chat.mjs';
const row=(id,text,sequence='1')=>({id,sender_uid:'7',sender_name:'First Sim',sender_sequence:sequence,channel_id:0,private:false,text,created_at_ms:123});
const reply=(rows=[],sequence='0',receipt=null)=>({version:1,state:'lot_chat',viewer_id:'7',lot_id:'41',lot_incarnation:'2',viewer_sequence:sequence,viewer_receipt:rows.filter(x=>x.sender_uid==='7').at(-1)??null,messages:rows,receipt});
test('unknown send retains exact sequence/text; explicit retry confirms one durable identity',async()=>{
 let latest,stored,fail=true;const writes=[];
 const c=createNativeLotChat(async(action,body)=>{
   if(action==='read')return reply();
   writes.push(structuredClone(body));stored??=row('1',body.text,body.sequence);
   if(fail){fail=false;throw Error('reply lost after commit');}return reply([stored],'1',stored);
 },v=>latest=v);
 c.setLive(true);await c.refresh();await c.send('Hello');
 assert.equal(latest.unknown,true);assert.equal(latest.messages.length,0);
 await c.send('Different');assert.equal(writes.length,1);
 await c.retry();assert.deepEqual(writes,[{sequence:'1',text:'Hello'},{sequence:'1',text:'Hello'}]);
 assert.equal(latest.unknown,false);assert.equal(latest.messages.length,1);assert.equal(latest.messages[0].id,'1');c.dispose();
});
test('authoritative history can reconcile a lost reply without another write',async()=>{
 let latest,stored,writes=0;
 const c=createNativeLotChat(async(action,body)=>{if(action==='read')return stored?reply([stored],'1'):reply();
 writes++;stored=row('1',body.text);throw Error('lost');},v=>latest=v);
 c.setLive(true);await c.refresh();await c.send('Saved');await c.refresh();
 assert.equal(latest.pending,false);assert.equal(latest.messages[0].text,'Saved');assert.equal(writes,1);c.dispose();
});
test('malformed or changed-room response hides history and never enables sending',async()=>{
 let latest,value=reply();const c=createNativeLotChat(async()=>value,v=>latest=v);
 c.setLive(true);await c.refresh();value={...reply([row('1','secret')],'1'),lot_id:'42'};await c.refresh();
 assert.equal(latest.ready,false);assert.deepEqual(latest.messages,[]);c.dispose();
});
test('disconnect/disposal rejects late completion and disables new writes',async()=>{
 let latest,resolve;const c=createNativeLotChat(()=>new Promise(r=>resolve=r),v=>latest=v);
 c.setLive(true);const p=c.refresh();c.setLive(false);resolve(reply([row('1','late')],'1'));await p;
 assert.equal(latest.ready,false);assert.deepEqual(latest.messages,[]);assert.equal(await c.send('No'),false);c.dispose();
});
test('input limits reject whitespace, controls and excessive UTF-16 text before transport',async()=>{
 let writes=0;const c=createNativeLotChat(async(action)=>{if(action==='send')writes++;return reply();},()=>{});
 c.setLive(true);await c.refresh();for(const text of ['', '  ','x'.repeat(201),'😀'.repeat(101),'\ud800','\0'])assert.equal(await c.send(text),false);
 assert.equal(writes,0);c.dispose();
});
