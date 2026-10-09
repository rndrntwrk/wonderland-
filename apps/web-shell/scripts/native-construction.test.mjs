import test from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {once} from 'node:events';
import {createConstructionTransport, MAX_REPLY_BYTES} from '../public/native-construction.mjs';

const endpoint='https://game.example/native/construction';
const payload=new Uint8Array(40).fill(7);
function reply(body=new Uint8Array(24).fill(3), headers={}) {
 const response=new Response(body,{headers:{'content-type':'application/vnd.wonderland.construction-reply',...headers}});
 Object.defineProperty(response,'url',{value:endpoint});return response;
}
function options(fetchImpl, more={}) {return {allowedOrigin:'https://game.example',csrfToken:'test-only-csrf',fetchImpl,timeoutMs:1000,...more};}

test('one binary POST, immutable request copy, credentials and CSRF stay off the URL',async()=>{
 let calls=0;
 const transport=createConstructionTransport(endpoint,options(async(url,init)=>{
  calls++;assert.equal(url,endpoint);assert.equal(init.method,'POST');assert.equal(init.credentials,'include');
  assert.equal(init.redirect,'error');assert.equal(init.cache,'no-store');assert.equal(init.headers['X-Wonderland-CSRF'],'test-only-csrf');
  await Promise.resolve();assert.equal(init.body[0],7);return reply();
 }));
 const mutable=payload.slice();const result=transport.send(mutable);mutable[0]=99;
 assert.deepEqual(await result,new Uint8Array(24).fill(3));assert.equal(calls,1);transport.dispose();
});
test('origin, userinfo, URL token fields and insecure production endpoints reject before fetch',()=>{
 for(const url of ['https://other.example/native','https://u:p@game.example/native','https://game.example/native?token=x','https://game.example/native#x','http://game.example/native','javascript:alert(1)']) {
  assert.throws(()=>createConstructionTransport(url,options(()=>assert.fail('must not fetch'))));
 }
 assert.throws(()=>createConstructionTransport(endpoint,options(()=>{}, {csrfToken:''})));
});
test('HTTP errors or wrong media type are unknown delivery, not implicit retries',async()=>{
 let calls=0;
 const transport=createConstructionTransport(endpoint,options(async()=>{calls++;return new Response('failure',{status:500});}));
 await assert.rejects(transport.send(payload),error=>error.code==='DELIVERY_UNKNOWN');
 await assert.rejects(transport.send(payload));assert.equal(calls,1);
 const wrong=createConstructionTransport(endpoint,options(async()=>reply('<html>',{'content-type':'text/html'})));
 await assert.rejects(wrong.send(payload),error=>error.code==='DELIVERY_UNKNOWN');
});
test('response advertised beyond cap is cancelled before reading',async()=>{
 let cancelled=false;
 const stream=new ReadableStream({cancel(){cancelled=true;}});
 const transport=createConstructionTransport(endpoint,options(async()=>reply(stream,{'content-length':String(MAX_REPLY_BYTES+1)})));
 await assert.rejects(transport.send(payload));assert.equal(cancelled,true);
});
test('streamed reply cannot bypass byte cap with a missing content-length',async()=>{
 let cancelled=false;
 const stream=new ReadableStream({start(c){c.enqueue(new Uint8Array(MAX_REPLY_BYTES));c.enqueue(new Uint8Array(1));},cancel(){cancelled=true;}});
 const transport=createConstructionTransport(endpoint,options(async()=>reply(stream)));
 await assert.rejects(transport.send(payload));assert.equal(cancelled,true);
});
test('dispose fences an uncooperative late fetch reply',async()=>{
 let resolve;
 const transport=createConstructionTransport(endpoint,options(()=>new Promise(r=>{resolve=r;})));
 const pending=transport.send(payload);transport.dispose();resolve(reply());
 await assert.rejects(pending,error=>error.code==='DELIVERY_UNKNOWN');
 await assert.rejects(transport.send(payload));
});
test('at most one call is in flight; a read or write cannot overtake it',async()=>{
 let resolve;let calls=0;
 const transport=createConstructionTransport(endpoint,options(()=>{calls++;return new Promise(r=>{resolve=r;});}));
 const pending=transport.send(payload);await assert.rejects(transport.send(payload));
 resolve(reply());await pending;assert.equal(calls,1);transport.dispose();
});
test('already-aborted caller sends nothing',async()=>{
 const abort=new AbortController();abort.abort();let calls=0;
 const transport=createConstructionTransport(endpoint,options(async()=>{calls++;return reply();}));
 await assert.rejects(transport.send(payload,{signal:abort.signal}));assert.equal(calls,0);transport.dispose();
});
test('wrong response URL and redirects cannot supply accepted bytes',async()=>{
 const response=reply();Object.defineProperty(response,'redirected',{value:true});
 const transport=createConstructionTransport(endpoint,options(async()=>response));
 await assert.rejects(transport.send(payload));
});
test('actual loopback fetch preserves complete bytes and CSRF header without retry',async()=>{
 let received=0;let captured;
 const server=createServer(async(req,res)=>{
  received++;const chunks=[];for await(const chunk of req)chunks.push(chunk);
  captured=Buffer.concat(chunks);assert.equal(req.headers['x-wonderland-csrf'],'fixture-token');
  res.writeHead(200,{'content-type':'application/vnd.wonderland.construction-reply'});res.end(new Uint8Array(32).fill(9));
 });
 server.listen(0,'127.0.0.1');await once(server,'listening');
 try {
  const origin=`http://127.0.0.1:${server.address().port}`;
  const transport=createConstructionTransport(`${origin}/native/construction`,{allowedOrigin:origin,csrfToken:'fixture-token',timeoutMs:3000});
  assert.deepEqual(await transport.send(payload),new Uint8Array(32).fill(9));assert.equal(received,1);assert.deepEqual(captured,Buffer.from(payload));transport.dispose();
 } finally {server.closeAllConnections();await new Promise(r=>server.close(r));}
});
test('real timeout after server receipt remains unknown and sends only once',async()=>{
 let count=0;
 const server=createServer(async(req)=>{for await(const _ of req){}count++;});
 server.listen(0,'127.0.0.1');await once(server,'listening');
 try {
  const origin=`http://127.0.0.1:${server.address().port}`;
  const transport=createConstructionTransport(`${origin}/native/construction`,{allowedOrigin:origin,csrfToken:'fixture-token',timeoutMs:250});
  await assert.rejects(transport.send(payload),error=>error.code==='DELIVERY_UNKNOWN');
  assert.equal(count,1);await assert.rejects(transport.send(payload));assert.equal(count,1);transport.dispose();
 } finally {server.closeAllConnections();await new Promise(r=>server.close(r));}
});
test('bounded response work rejects thousands of tiny chunks before byte exhaustion',async()=>{
 let produced=0;
 const stream=new ReadableStream({pull(c){if(produced++<5000)c.enqueue(new Uint8Array([1]));else c.close();}});
 const transport=createConstructionTransport(endpoint,options(async()=>reply(stream)));
 await assert.rejects(transport.send(payload));assert.ok(produced<=4098,`produced ${produced}`);
});
test('uncooperative stream cleanup cannot hold an uncertain result indefinitely',async()=>{
 const stream=new ReadableStream({start(c){c.enqueue(new Uint8Array(MAX_REPLY_BYTES+1));},cancel(){return new Promise(()=>{});}});
 const transport=createConstructionTransport(endpoint,options(async()=>reply(stream)));
 const outcome=await Promise.race([transport.send(payload).then(()=> 'accepted', e=>e.code),new Promise(resolve=>setTimeout(()=>resolve('cleanup-hung'),120))]);
 assert.equal(outcome,'DELIVERY_UNKNOWN');transport.dispose();
});
test('a synchronous fetch adapter failure has no orphan abort rejection',async()=>{
 const transport=createConstructionTransport(endpoint,options(()=>{throw new Error('synchronous host failure');}));
 await assert.rejects(transport.send(payload),error=>error.code==='DELIVERY_UNKNOWN');
 await new Promise(resolve=>setImmediate(resolve));
 await assert.rejects(transport.send(payload),error=>error.code==='CLOSED');
});
test('timeout also fences an uncooperative body read after response headers',async()=>{
 const stream=new ReadableStream({pull(){return new Promise(()=>{});},cancel(){return new Promise(()=>{});}});
 const transport=createConstructionTransport(endpoint,options(async()=>reply(stream),{timeoutMs:30}));
 const outcome=await Promise.race([transport.send(payload).then(()=> 'accepted', e=>e.code),new Promise(resolve=>setTimeout(()=>resolve('read-hung'),200))]);
 assert.equal(outcome,'DELIVERY_UNKNOWN');
 await assert.rejects(transport.send(payload),error=>error.code==='CLOSED');
});
