import test from 'node:test';
import assert from 'node:assert/strict';
import {createNativeAccountProvider} from '../public/native-account.mjs';
const row=(id='9007199254740993')=>({id,avatar_id:'7',asset_key:'18446744073709551615',suit_type:0,source:'cas',purchase_price:'0',sale_price:'0'});
const page=()=>({version:1,state:'owned_assets',account_id:'1',avatar_id:'7',balance:'0',outfits:[row()],next:null});
function fixture(handler=()=>Response.json(page())){
 const calls=[];let latest;
 const provider=createNativeAccountProvider('/native-account',v=>latest=v,{origin:'https://fixture.invalid',nativeLots:true,fetch:async(url,init)=>{
  const action=url.split('/').at(-1),body=JSON.parse(init.body);calls.push({action,body});
  if(action==='login')return Response.json({version:1,state:'account_ready',admission_available:true,token:'a'.repeat(64)});
  if(action==='roster')return Response.json({version:1,state:'account_ready',admission_available:true,account_id:'1',avatars:[{avatar_id:'7',name:'First Traveller'}],destinations:[],next:null});
  if(action==='logout')return Response.json({signed_out:true});return handler(action,body,init);
 }});return {provider,calls,get latest(){return latest}};
}
test('saved assets read uses only the owned avatar/cursor and preserves exact durable decimal identities',async()=>{
 const f=fixture();try{await f.provider.login('owner','local');const v=await f.provider.ownedAssets('7','0');assert.deepEqual(v,page());
  assert.deepEqual(f.calls.at(-1),{action:'assets',body:{avatar_id:'7',after:'0'}});
  const before=f.calls.length;await assert.rejects(f.provider.ownedAssets('8','0'));assert.equal(f.calls.length,before);
 }finally{f.provider.dispose();}
});
test('saved ownership pages reject foreign identity, duplicates, invalid budget and regressed cursors',async()=>{
 for(const bad of [{...page(),avatar_id:'8'},{...page(),balance:'-1'},{...page(),balance:'4294967296'},{...page(),outfits:[row(),row()]},{...page(),outfits:[{...row(),avatar_id:'8'}]},{...page(),next:'0'},{...page(),outfits:Array.from({length:65},(_,i)=>row(String(i+1)))}]){
  const f=fixture(()=>Response.json(bad));try{await f.provider.login('owner','local');await assert.rejects(f.provider.ownedAssets('7','0'));}finally{f.provider.dispose();}
 }
});
test('sign-out aborts saved-assets reads and late replies cannot restore owned data',async()=>{
 let release,signal;const f=fixture((action,body,init)=>{assert.equal(action,'assets');signal=init.signal;return new Promise(r=>release=r);});
 try{await f.provider.login('owner','local');const work=f.provider.ownedAssets('7','0');const denied=assert.rejects(work);
  await f.provider.logout();assert.equal(signal.aborted,true);release(Response.json(page()));await denied;assert.equal(f.latest.state,'signed_out');
 }finally{f.provider.dispose();}
});
test('a full transport page carries an exact large cursor into the next owned page',async()=>{
 const f=fixture((action,body)=>Response.json(body.after==='0'?{...page(),outfits:Array.from({length:64},(_,i)=>row(String(9007199254740993n+BigInt(i)))),next:'9007199254741056'}:{...page(),outfits:[row('9007199254741057')]}));
 try{await f.provider.login('owner','local');const first=await f.provider.ownedAssets('7','0');assert.equal(first.next,'9007199254741056');
  const second=await f.provider.ownedAssets('7',first.next);assert.equal(second.outfits[0].id,'9007199254741057');assert.equal(second.next,null);
  assert.deepEqual(f.calls.at(-1).body,{avatar_id:'7',after:'9007199254741056'});
 }finally{f.provider.dispose();}
});
