import test from 'node:test';
import assert from 'node:assert/strict';
import {createSupervisorOutboundHandler} from '../src/hosted/supervisor-outbound.ts';
import {dispatchSupervisorRpc} from '../src/hosted/supervisor-rpc.ts';
import {createContainerEdgeTransport} from '../../../tools/native-browser/owned-demo/edge-transport.mjs';

const origin='https://wonderland-demo.gl4sspr1sm.workers.dev';
function setup(){
 const calls=[],http=[];
 const handler=createSupervisorOutboundHandler({publicOrigin:origin,maxPrivateRequestBytes:1024,
  dispatchRpc:body=>dispatchSupervisorRpc(body,async(...args)=>{calls.push(args);return {receipt:'exact'};}),
  accountHttp:async req=>{http.push({url:req.url,origin:req.headers.get('origin'),ip:req.headers.get('cf-connecting-ip'),private:req.headers.get('x-owned-private'),body:await req.json()});return Response.json({session:'fixture-response'});}});
 const transport=createContainerEdgeTransport({publicOrigin:origin,fetchOutbound:async req=>handler(req)});
 return {calls,http,handler,transport};
}
test('container private calls use the existing exact RPC body and receipt, once',async()=>{
 const f=setup(),body={target:'lot',method:'getBatch',args:['1','7']};
 const res=await f.transport.dispatchFetch('http://owned-private.invalid/rpc',{method:'POST',headers:{'content-type':'application/json','x-owned-private':'local-fixture-capability'},body:JSON.stringify(body)});
 assert.deepEqual(await res.json(),{value:{receipt:'exact'}});assert.deepEqual(f.calls,[['lot','getBatch',['1','7']]]);
});
test('private bridge never treats a browser Origin as supervisor authority',async()=>{
 const f=setup();const res=await f.handler(new Request('http://owned-private.invalid/rpc',{method:'POST',headers:{origin,'content-type':'application/json'},body:'{}'}));
 assert.equal(res.status,403);assert.deepEqual(f.calls,[]);
});
test('private outbound JSON is bounded before dispatch',async()=>{
 const f=setup();for(const body of ['{invalid',' '.repeat(1025)]){
  const res=await f.handler(new Request('http://owned-private.invalid/rpc',{method:'POST',headers:{'content-type':'application/json'},body}));assert.equal(res.status,400);
 }assert.equal(f.calls.length,0);
});
test('account HTTP retains public Origin and trusted ingress identity without the private capability',async()=>{
 const f=setup();const res=await f.transport.dispatchFetch(origin+'/native-account/login',{method:'POST',headers:{origin,'content-type':'application/json','cf-connecting-ip':'192.0.2.1','x-owned-private':'must-not-forward'},body:'{"username":"fixture"}'});
 assert.equal(res.status,200);assert.deepEqual(f.http,[{url:origin+'/native-account/login',origin,ip:'192.0.2.1',private:null,body:{username:'fixture'}}]);assert.deepEqual(f.calls,[]);
});
test('authenticated avatar creation preserves exact operation/body through the private bridge once',async()=>{
 const f=setup(),body={operation:'native_http_create_01',avatar:{name:'Owned Traveller',head_key:'9007199254740993',body_key:'18446744073709551615'}};
 const res=await f.transport.dispatchFetch(origin+'/native-account/create',{method:'POST',headers:{origin,'content-type':'application/json','cf-connecting-ip':'192.0.2.2','x-owned-private':'must-not-forward'},body:JSON.stringify(body)});
 assert.equal(res.status,200);assert.deepEqual(f.http,[{url:origin+'/native-account/create',origin,ip:'192.0.2.2',private:null,body}]);assert.deepEqual(f.calls,[]);
});
test('only explicit public account methods can cross the outbound bridge',async()=>{
 const f=setup();for(const path of ['/native-account/register','/rpc','/native-account/login?x=1','/native-account/../rpc']){
  await assert.rejects(f.transport.dispatchFetch(origin+path,{method:'POST',headers:{origin,'content-type':'application/json'},body:'{}'}));
 }assert.equal(f.http.length,0);assert.equal(f.calls.length,0);
});
test('cross-origin, credentials, non-JSON and non-POST forwarding fail closed',async()=>{
 const f=setup();for(const [url,init] of [
  ['https://other.invalid/native-account/login',{method:'POST',headers:{origin,'content-type':'application/json'},body:'{}'}],
  [origin+'/native-account/login',{method:'POST',headers:{origin:'https://evil.invalid','content-type':'application/json'},body:'{}'}],
  [origin+'/native-account/login',{method:'GET'}],
  [origin+'/native-account/login',{method:'POST',headers:{origin,'content-type':'text/plain'},body:'{}'}],
 ])await assert.rejects(f.transport.dispatchFetch(url,init));assert.equal(f.http.length,0);
});
test('outbound bridge refuses unregistered host/path rather than fetching it',async()=>{
 const f=setup();for(const url of ['https://owned-private.invalid/rpc','http://owned-private.invalid/rpc?x=1','http://other.invalid/rpc','http://owned-private.invalid/admin']){
  const res=await f.handler(new Request(url,{method:'POST',headers:{'content-type':'application/json'},body:'{}'}));assert.equal(res.status,404);
 }assert.equal(f.calls.length,0);
});
test('closing the transport prevents any later private mutation',async()=>{
 const f=setup();await f.transport.dispose();await assert.rejects(f.transport.dispatchFetch('http://owned-private.invalid/rpc',{method:'POST',headers:{'content-type':'application/json'},body:'{}'}));assert.equal(f.calls.length,0);
});
test('transport propagates an ambiguous network failure without retry',async()=>{
 let count=0;const failure=Error('reply lost');const transport=createContainerEdgeTransport({publicOrigin:origin,fetchOutbound:async()=>{count++;throw failure;}});
 await assert.rejects(transport.dispatchFetch('http://owned-private.invalid/rpc',{method:'POST',headers:{'content-type':'application/json'},body:'{}'}),e=>e===failure);assert.equal(count,1);
});
