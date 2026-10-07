// TEST ONLY. Loopback proxy for the real controlled FreeSO gateway plus a real
// native runtime subprocess. No production startup path imports this module.
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, extname, sep } from 'node:path';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { randomBytes } from 'node:crypto';
import { WebSocket, WebSocketServer } from 'ws';

export async function createFixture({dist, gateway, runtimeExecutable, port=18888}) {
  assert.equal(new URL(gateway).hostname,'127.0.0.1');
  const root=resolve(dist), origin=`http://127.0.0.1:${port}`;
  const child=spawn(runtimeExecutable,[],{env:{...process.env,WONDERLAND_NATIVE_PEER_TEST_ONLY:'1'},stdio:['pipe','pipe','inherit']});
  const requests=[];let poisoned=false;
  createInterface({input:child.stdout}).on('line',line=>{
    const slot=requests.shift();if(!slot)return;
    try{const result=JSON.parse(line);if(result.error)slot.reject(Error(result.error));else slot.resolve(result);}catch(error){slot.reject(error);}
  });
  child.on('exit',()=>{poisoned=true;for(const slot of requests.splice(0))slot.reject(Error('Native test peer exited'));});
  const execute=value=>new Promise((resolve,reject)=>{
    if(poisoned || requests.length>=8)return reject(Error('Native peer unavailable/backpressured'));
    requests.push({resolve,reject});child.stdin.write(JSON.stringify(value)+'\n');
  });
  const tickets=new Map(), clients=new Set(), relays=new Set();
  const stats={admissions:0,opens:0,checkpoints:0,actions:0,accepted:0,unknownDrops:0,ticks:0};
  let serial=Promise.resolve(), dropNext=false;
  const transact=fn=>{const result=serial.then(fn);serial=result.catch(()=>{});return result;};
  const packet=result=>Buffer.from(result.packet,'hex');
  const send=(ws,bytes)=>{if(ws.readyState===1){if(ws.bufferedAmount>2*1024*1024)ws.close(1013);else ws.send(bytes,{binary:true});}};
  const broadcast=result=>{if(!result)return;for(const ws of clients)if(ws.live)send(ws,packet(result));};
  async function current(authorization) {
    const response=await fetch(gateway+'/v1/session',{headers:{authorization},signal:AbortSignal.timeout(3000)});
    assert.ok(response.ok,'Authenticated gateway session required');
    const value=await response.json();
    assert.equal(value.state,'lot_ready');assert.equal(value.avatar_id,42);assert.equal(value.lot_location,16777472);
    assert.ok(Number.isSafeInteger(value.epoch)&&Number.isSafeInteger(value.lot_incarnation));
    return value;
  }
  const matches=(value,binding)=>String(value.epoch)===binding.source_epoch&&String(value.lot_incarnation)===binding.lot_incarnation&&value.avatar_id===binding.avatar_id&&value.lot_location===binding.lot_location;
  const body=async req=>{let data=Buffer.alloc(0);for await(const part of req){if(data.length+part.length>8192)throw Error('Request too large');data=Buffer.concat([data,part]);}return JSON.parse(data.toString());};
  const server=http.createServer(async(req,res)=>{
    try {
      const url=new URL(req.url,origin);
      if(url.pathname==='/gateway/v1/native-lot/admit') {
        assert.equal(req.method,'POST');assert.equal(req.headers.origin,origin);
        const binding=await body(req);const authorization=req.headers.authorization??'';
        const session=await current(authorization);assert.ok(matches(session,binding));
        for(const [key,value] of tickets)if(value.expires<Date.now())tickets.delete(key);
        assert.ok(tickets.size<32);
        const ticket=randomBytes(24).toString('hex');tickets.set(ticket,{authorization,binding,expires:Date.now()+15000});stats.admissions++;
        res.writeHead(200,{'content-type':'application/json','cache-control':'no-store'});res.end(JSON.stringify({ticket}));return;
      }
      if(url.pathname.startsWith('/gateway/')) {
        const upstream=http.request(gateway+url.pathname.slice('/gateway'.length),{method:req.method,headers:req.headers},reply=>{res.writeHead(reply.statusCode,reply.headers);reply.pipe(res);});
        upstream.on('error',()=>{res.writeHead(502);res.end();});req.pipe(upstream);return;
      }
      if(url.pathname==='/wonderland-config.json'){
        res.writeHead(200,{'content-type':'application/json','cache-control':'no-store'});
        res.end(JSON.stringify({version:1,mode:'connected',gateway_url:'/gateway',native_lots:true}));return;
      }
      const path=resolve(root,'.'+decodeURIComponent(url.pathname==='/'?'/index.html':url.pathname));assert.ok(path.startsWith(root+sep));
      const bytes=await readFile(path);assert.ok(bytes.length<=200*1024*1024);
      const type={'.wasm':'application/wasm','.js':'text/javascript','.mjs':'text/javascript','.css':'text/css','.json':'application/json','.png':'image/png','.webp':'image/webp','.svg':'image/svg+xml','.html':'text/html'}[extname(path)]??'application/octet-stream';
      res.writeHead(200,{'content-type':type,'cache-control':'no-store'});res.end(bytes);
    } catch {res.writeHead(400,{'content-type':'application/json'});res.end(JSON.stringify({error:'Controlled native request rejected'}));}
  });
  const native=new WebSocketServer({noServer:true,maxPayload:65536,perMessageDeflate:false});
  const legacy=new WebSocketServer({noServer:true,maxPayload:16*1024*1024,perMessageDeflate:false});
  server.on('upgrade',(req,socket,head)=>{
    if(req.headers.origin!==origin){socket.destroy();return;}
    if(req.url==='/gateway/v1/native-lot/stream')native.handleUpgrade(req,socket,head,ws=>native.emit('connection',ws));
    else if(req.url==='/gateway/v1/ws')legacy.handleUpgrade(req,socket,head,ws=>legacy.emit('connection',ws));
    else socket.destroy();
  });
  legacy.on('connection',ws=>{
    const upstream=new WebSocket(gateway.replace('http:','ws:')+'/v1/ws',{headers:{origin}});const pending=[];let bytes=0;
    relays.add(ws);relays.add(upstream);
    ws.on('message',(data,binary)=>{if(upstream.readyState===1)upstream.send(data,{binary});else{bytes+=data.length;if(bytes>1024*1024)ws.close(1009);else pending.push([data,binary]);}});
    upstream.on('open',()=>{for(const [data,binary] of pending.splice(0))upstream.send(data,{binary});});
    upstream.on('message',(data,binary)=>{if(ws.readyState===1)ws.send(data,{binary});});
    ws.on('close',()=>{relays.delete(ws);upstream.close();});upstream.on('close',()=>{relays.delete(upstream);ws.close();});
    ws.on('error',()=>upstream.close());upstream.on('error',()=>ws.close());
  });
  native.on('connection',ws=>{
    stats.opens++;clients.add(ws);let auth=null,queued=0;
    const timeout=setTimeout(()=>ws.close(1008),15000);
    ws.on('message',(data,binary)=>{
      if(++queued>4){ws.close(1013);return;}
      transact(async()=>{
        try{
          if(!auth){
            assert.equal(binary,false);const value=JSON.parse(data.toString());assert.equal(value.type,'native_auth');
            const grant=tickets.get(value.ticket);tickets.delete(value.ticket);assert.ok(grant&&grant.expires>=Date.now());
            const session=await current(grant.authorization);assert.ok(matches(session,grant.binding));auth=grant;
            if(value.resume!==true){const result=await execute({op:'bootstrap',...grant.binding});send(ws,packet(result));}
            return;
          }
          assert.equal(binary,true);const session=await current(auth.authorization);assert.ok(matches(session,auth.binding));
          const magic=data.subarray(0,4).toString();
          if(magic==='WLR1'){
            const result=await execute({op:'checkpoint',request:data.toString('hex')});send(ws,packet(result));ws.live=true;stats.checkpoints++;clearTimeout(timeout);
          } else if(magic==='WLC1') {
            assert.ok(ws.live);stats.actions++;
            const result=await execute({op:'action',request:data.toString('hex')});broadcast(result.transition);if(result.accepted)stats.accepted++;
            if(dropNext){dropNext=false;stats.unknownDrops++;ws.close(1012);return;}
            send(ws,Buffer.from(result.receipt,'hex'));
          } else throw Error('Unexpected native client protocol');
        }catch(error){console.error('Native fixture rejected:',error.message);ws.close(1008);}
        finally{queued--;}
      }).catch(()=>ws.close(1011));
    });
    ws.on('close',()=>{clearTimeout(timeout);clients.delete(ws);});ws.on('error',()=>ws.close());
  });
  await new Promise(resolve=>server.listen(port,'127.0.0.1',resolve));
  const timer=setInterval(()=>{if([...clients].some(ws=>ws.live))transact(async()=>{broadcast(await execute({op:'tick'}));stats.ticks++;}).catch(()=>{});},100);
  return {
    origin,stats,
    reset:()=>transact(async()=>broadcast(await execute({op:'reset_needs'}))),
    dropNextReceipt:()=>{dropNext=true;},
    disconnect:()=>{for(const ws of clients)ws.close(1012);},
    state:()=>transact(()=>execute({op:'state'})),
    active:()=>clients.size,
    close:async()=>{clearInterval(timer);for(const ws of [...clients,...relays])ws.terminate();native.close();legacy.close();child.kill();await new Promise(resolve=>server.close(resolve));},
  };
}
