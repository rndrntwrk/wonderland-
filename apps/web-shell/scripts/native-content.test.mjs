import assert from 'node:assert/strict';
import test from 'node:test';
import {createHash} from 'node:crypto';
import {loadNativeContent} from '../public/native-content.mjs';
import {createNativeAccountProvider} from '../public/native-account.mjs';
const data=new TextEncoder().encode('isolated fixture');
const sha=createHash('sha256').update(data).digest('hex');
const manifest=()=>({schema_version:1,content_release:'a'.repeat(64),skeleton_name:'adult.skel',
 head_collections:['heads.col'],body_collections:['body.col'],total_input_bytes:data.length,
 files:[{input_name:'adult.skel',size:data.length,sha256:sha}]});
function setup(change=x=>x,resource=data){
 const calls=[];
 return {calls,read:async path=>{calls.push(path);return path.endsWith('/manifest')
  ?Response.json(change(manifest())):new Response(resource,{headers:{'content-type':'application/octet-stream'}});}};
}
test('validated source bytes become named memory-only files',async()=>{
 const f=setup(),pack=await loadNativeContent(f);
 assert.deepEqual(f.calls,['/native-content/manifest','/native-content/file/'+sha]);
 assert.equal(pack.files[0].name,'adult.skel');assert.deepEqual(new Uint8Array(await pack.files[0].arrayBuffer()),data);
 assert.equal(JSON.parse(pack.manifest).content_release,'a'.repeat(64));
});
test('distinct archive bytes retain the same original basename without being merged',async()=>{
 const other=new TextEncoder().encode('second original archive');
 const otherHash=createHash('sha256').update(other).digest('hex');
 const value=manifest();value.files[0].input_name='textures.dat';
 value.files.push({input_name:'textures.dat',size:other.length,sha256:otherHash});value.total_input_bytes+=other.length;
 const pack=await loadNativeContent({read:async path=>path.endsWith('/manifest')?Response.json(value):
  new Response(path.endsWith(otherHash)?other:data,{headers:{'content-type':'application/octet-stream'}})});
 assert.deepEqual(pack.files.map(file=>file.name),['textures.dat','textures.dat']);
 assert.notEqual(await pack.files[0].text(),await pack.files[1].text());
});
test('resource mismatch and oversized streams fail without returning a pack',async()=>{
 await assert.rejects(loadNativeContent(setup(x=>x,new Uint8Array(data.length))),/hash mismatch/);
 await assert.rejects(loadNativeContent(setup(x=>x,new Uint8Array(data.length+1))),/byte budget/);
});
test('invalid manifests fail before any resource request',async()=>{
 for(const change of [x=>({...x,files:[{...x.files[0],input_name:'../secret'}]}),x=>({...x,files:[...x.files,...x.files]}),
 x=>({...x,total_input_bytes:0}),x=>({...x,content_release:'unbound'}),x=>({...x,total_input_bytes:128*1024*1024+1})]){
  const f=setup(change);await assert.rejects(loadNativeContent(f));assert.equal(f.calls.length,1);
 }
});
test('revocation during a resource read discards late bytes',async()=>{
 const f=setup();let valid=true;const read=async path=>{const response=await f.read(path);if(path.includes('/file/'))valid=false;return response;};
 await assert.rejects(loadNativeContent({read,current:()=>valid}),/session changed/);
});
test('authentication failures never fall back to unauthenticated or public content',async()=>{
 let calls=0;await assert.rejects(loadNativeContent({read:async()=>{calls++;return new Response(null,{status:401});}}),/unavailable/);assert.equal(calls,1);
});
test('account provider signs resource requests with the current exact owned selection only',async()=>{
 const content=setup(),requests=[];
 const provider=createNativeAccountProvider('/native-account',()=>{}, {origin:'https://fixture.invalid',nativeLots:true,
  operationId:()=> 'owned_operation_0001',fetch:async(url,options)=>{
   if(url.includes('/native-content/')){requests.push({url,options});return content.read(new URL(url).pathname);}
   if(url.endsWith('/login'))return Response.json({version:1,state:'account_ready',admission_available:true,token:'b'.repeat(64)});
   if(url.endsWith('/roster'))return Response.json({version:1,state:'account_ready',admission_available:true,account_id:'1',avatars:[{avatar_id:'7',name:'Fixture'}],destinations:[{lot_id:'11',lot_location:1,name:'Fixture lot'}],next:null});
   return Response.json({version:1,state:'lot_ready',binding:{source_epoch:'3',lot_incarnation:'4',lot_location:1,avatar_id:7},ticket:'c'.repeat(48)});
  }});
 try{
  await assert.rejects(provider.contentFiles(),/No active/);await provider.login('fixture','local');
  await assert.rejects(provider.contentFiles(),/No active/);await provider.enter('7','11');
  const pack=await provider.contentFiles();assert.equal(pack.files.length,1);assert.equal(requests.length,2);
  for(const {url,options} of requests){assert.equal(new URL(url).origin,'https://fixture.invalid');
   assert.equal(options.headers.Authorization,'Bearer '+'b'.repeat(64));
   assert.deepEqual(JSON.parse(options.body),{avatar_id:'7',lot_id:'11',operation:'owned_operation_0001'});
   assert.equal(options.cache,'no-store');assert.equal(options.redirect,'error');}
 }finally{provider.dispose();}
 await assert.rejects(provider.contentFiles(),/No active/);
});
