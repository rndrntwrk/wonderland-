import test from 'node:test';
import assert from 'node:assert/strict';
import {createNativeAccountProvider} from '../public/native-account.mjs';
const release='b'.repeat(64),token='a'.repeat(64);
const spec={name:'New Traveller',description:'Independent original choices',gender:'female',skin:'dark',head_key:'1',body_key:'2',release};
function fixture({lostReply=false,badPage=false,revoked=false}={}){
 let latest,rows=[],creates=0,held=null;const bodies=[];
 const provider=createNativeAccountProvider('/native-account',v=>latest=v,{origin:'https://fixture.invalid',nativeLots:true,
 operationId:()=> 'native_creator_operation_01',
 fetch:async(url,init)=>{
 const action=new URL(url).pathname.split('/').at(-1),body=JSON.parse(init.body);bodies.push({action,body,authorization:init.headers.Authorization});
 if(action==='login')return Response.json({version:1,state:'account_ready',admission_available:true,token});
 if(action==='roster')return Response.json({version:1,state:'account_ready',admission_available:true,account_id:'1',avatars:rows,destinations:[],next:null});
 if(action==='creation-options'&&revoked)return new Response(null,{status:401});
 if(action==='creation-options')return Response.json({version:1,state:'creation_options',release,available_slots:3,
 entries:body.after==='0'?[{key:'1',part:'head',gender:'female',skins:['dark']}]:[{key:'2',part:'body',gender:'female',skins:['dark']}],
 next:body.after==='0'?(badPage?'0':'1'):null});
 if(action==='create'){creates++;assert.deepEqual(body,{operation:'native_creator_operation_01',avatar:spec});
 rows=[{avatar_id:'7',name:spec.name}];if(lostReply&&creates===1)throw Error('reply lost after commit');
 return Response.json({version:1,state:'avatar_created',avatar:{avatar_id:'7',...spec}});}
 if(action==='logout')return Response.json({signed_out:true});
 throw Error('unexpected '+url);
 }});
 return {provider,bodies,get latest(){return latest},get creates(){return creates}};
}
test('empty-account creator collects paged eligible catalog without manufacturing a Sim',async()=>{
 const f=fixture();try{await f.provider.login('fixture','local');await f.provider.openCreator();
 assert.equal(f.latest.creator_open,true);assert.equal(f.latest.creation_options.entries.length,2);
 assert.equal(f.latest.creation_options.release,release);assert.deepEqual(f.latest.avatars,[]);assert.equal(f.creates,0);
 assert.ok(f.bodies.filter(x=>x.action==='creation-options').every(x=>x.authorization==='Bearer '+token));
 }finally{f.provider.dispose();}
});
test('create result returns to actual owned roster and selects the exact created identity',async()=>{
 const f=fixture();try{await f.provider.login('fixture','local');await f.provider.openCreator();await f.provider.createAvatar(spec);
 assert.equal(f.latest.creator_open,false);assert.equal(f.latest.created_avatar_id,'7');assert.deepEqual(f.latest.avatars,[{avatar_id:'7',name:spec.name}]);
 assert.equal(f.latest.creation_pending,false);assert.equal(f.creates,1);
 }finally{f.provider.dispose();}
});
test('ambiguous creation holds the same bytes/operation and requires explicit retry',async()=>{
 const f=fixture({lostReply:true});try{await f.provider.login('fixture','local');await f.provider.openCreator();await f.provider.createAvatar(spec);
 assert.equal(f.latest.creation_unknown,true);assert.equal(f.latest.creation_pending,true);assert.equal(f.creates,1);
 await f.provider.createAvatar({...spec,name:'Different Traveller'});assert.equal(f.creates,1);
 await f.provider.retryCreate();assert.equal(f.creates,2);assert.equal(f.latest.created_avatar_id,'7');assert.equal(f.latest.avatars.length,1);
 const sent=f.bodies.filter(x=>x.action==='create');assert.deepEqual(sent[0].body,sent[1].body);
 }finally{f.provider.dispose();}
});
test('unbound appearance and malformed catalog pagination fail closed',async()=>{
 const f=fixture({badPage:true});try{await f.provider.login('fixture','local');await f.provider.openCreator();
 assert.equal(f.latest.creator_open,false);assert.equal(f.creates,0);}finally{f.provider.dispose();}
 const g=fixture();try{await g.provider.login('fixture','local');await g.provider.openCreator();
 await g.provider.createAvatar({...spec,body_key:'999'});assert.equal(g.creates,0);
 }finally{g.provider.dispose();}
});
test('disposed or signed-out creator cannot submit a new mutation',async()=>{
 const f=fixture();await f.provider.login('fixture','local');await f.provider.openCreator();await f.provider.logout();
 await f.provider.createAvatar(spec);assert.equal(f.creates,0);f.provider.dispose();await f.provider.openCreator();assert.equal(f.creates,0);
});
test('revoked creator eligibility clears the account and private creator state',async()=>{
 const f=fixture({revoked:true});try{await f.provider.login('fixture','local');await f.provider.openCreator();
 assert.equal(f.latest.state,'signed_out');assert.equal(f.latest.creator_open,false);assert.deepEqual(f.latest.avatars,[]);
 await assert.rejects(f.provider.contentFiles(),/No active/);
 }finally{f.provider.dispose();}
});
