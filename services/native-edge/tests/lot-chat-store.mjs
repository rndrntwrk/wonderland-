import assert from 'node:assert/strict';
import {Miniflare} from 'miniflare';
import {build} from 'esbuild';
import {mkdtemp} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
const root=await mkdtemp(join(tmpdir(),'wonderland-lot-chat-'));
const bundled=await build({entryPoints:['tests/lot-chat-driver.ts'],bundle:true,format:'esm',platform:'neutral',external:['cloudflare:workers'],write:false});
let mf;
async function start(){mf=new Miniflare({cf:false,modulesRoot:root,modules:[{type:'ESModule',path:join(root,'worker.mjs'),contents:bundled.outputFiles[0].text}],
 compatibilityDate:'2026-07-01',durableObjects:{CHAT:{className:'TestChat',useSQLite:true}},durableObjectsPersist:join(root,'do')});await mf.ready;}
async function raw(action,avatar='1',message,room='first'){
 const r=await mf.dispatchFetch('http://fixture.invalid/',{method:'POST',body:JSON.stringify({action,avatar,message,room,name:avatar==='1'?'First Sim':'Second Sim'})});
 return {status:r.status,value:r.ok?await r.json():null};
}
async function call(...args){const r=await raw(...args);assert.equal(r.status,200);return r.value;}
const checks=[];
try{
 await start();assert.deepEqual((await call('read')).messages,[]);
 const first=await call('append','1',{sequence:'1',text:'Hello'});
 assert.deepEqual(first,{id:'1',sender_uid:'1',sender_name:'First Sim',sender_sequence:'1',channel_id:0,private:false,text:'Hello',created_at_ms:123});
 assert.deepEqual(await call('append','1',{sequence:'1',text:'Hello'}),first);
 assert.equal((await raw('append','1',{sequence:'1',text:'different'})).status,409);
 checks.push('persisted exact retry and changed-byte conflict');
 for(let i=1;i<=102;i++)await call('append','2',{sequence:String(i),text:'Public message '+i});
 const history=await call('read','1');assert.equal(history.messages.length,100);assert.equal(history.messages[0].id,'4');assert.equal(history.messages.at(-1).id,'103');
 assert.deepEqual(history.viewer_receipt,first);assert.deepEqual(await call('append','1',{sequence:'1',text:'Hello'}),first);
 assert.equal((await call('read')).messages.length,100);
 checks.push('100-message history and independent sender receipt survive history pruning');
 assert.deepEqual((await call('read','1',undefined,'other')).messages,[]);
 assert.equal((await raw('rollback','1',{sequence:'2',text:'Interrupted'})).status,409);
 const recovered=await call('append','1',{sequence:'2',text:'After rollback'});assert.equal(recovered.id,'104');
 checks.push('separate rooms and atomic rollback do not leak or allocate phantom messages');
 await mf.dispose();await start();assert.deepEqual((await call('read')).messages.at(-1),recovered);
 assert.deepEqual(await call('append','1',{sequence:'2',text:'After rollback'}),recovered);
 assert.equal((await raw('append','1',{sequence:'1',text:'Hello'})).status,409);
 checks.push('cold SQLite restart preserves history and exact duplicate identity; older sequences stay rejected');
 const visible=await call('audience','1',['2']);assert.deepEqual(visible.messages,[recovered]);
 assert.equal((await raw('audience','1',null)).status,409);
 checks.push('bound source ignore list filters other speakers; missing metadata fails closed');
 console.log(JSON.stringify({passed:true,checks}));
}finally{await mf?.dispose();}
