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

export async function createFixture({dist, gateway, runtimeExecutable, port=18888, actionDelayTicks=0, manualTicks=false, construction=false}) {
  assert.equal(new URL(gateway).hostname,'127.0.0.1');
  assert.ok(Number.isInteger(actionDelayTicks) && actionDelayTicks>=0 && actionDelayTicks<=128);
  assert.equal(typeof manualTicks,'boolean');
  assert.equal(typeof construction,'boolean');
  const root=resolve(dist), origin=`http://127.0.0.1:${port}`;
  const child=spawn(runtimeExecutable,[],{env:{...process.env,WONDERLAND_NATIVE_PEER_TEST_ONLY:'1',WONDERLAND_NATIVE_CONSTRUCTION_FIXTURE:construction?'1':'0'},stdio:['pipe','pipe','inherit']});
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
  const stats={admissions:0,opens:0,checkpoints:0,actions:0,accepted:0,unknownDrops:0,ticks:0,silentReceipts:0,bursts:0};
  let serial=Promise.resolve(), dropNext=false, loseNext=false, recoveryTail=0, dropConstruction=false, holdConstruction=false;
  const constructionGrants=new Map();
  Object.assign(stats,{constructionCalls:0,constructionBuilds:0,constructionLostReplies:0});
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
      if(construction && /^\/construction-fixture\/(plinth|block|floor)\.svg$/.test(url.pathname)) {
        const color=url.pathname.includes('plinth')?'#be412d':url.pathname.includes('block')?'#2d5abe':'#507da0';
        // Authored fixture thumbnail only; not original catalogue artwork.
        res.writeHead(200,{'content-type':'image/svg+xml','cache-control':'no-store'});
        res.end(`<svg xmlns="http://www.w3.org/2000/svg" width="96" height="64" viewBox="0 0 96 64"><path d="M18 42L48 56L78 42L48 28Z" fill="#dce8dc"/><path d="M26 20L48 10L70 20L70 43L48 54L26 43Z" fill="${color}"/><path d="M26 20L48 31L70 20M48 31V54" fill="none" stroke="#fff" opacity=".5"/></svg>`);return;
      }
      if(construction && url.pathname==='/gateway/v1/native-lot/construction/catalogue') {
        assert.equal(req.method,'POST');assert.equal(req.headers.origin,origin);
        const authorization=req.headers.authorization??'';
        const binding=await body(req),session=await current(authorization);
        assert.ok(matches(session,binding));
        for(const [key,g] of constructionGrants)if(g.expires<Date.now())constructionGrants.delete(key);
        // One current anti-CSRF token for this authenticated account/session.
        for(const [key,g] of constructionGrants)if(g.authorization===authorization)constructionGrants.delete(key);
        assert.ok(constructionGrants.size<32);
        const value=await transact(()=>execute({op:'construction_catalogue',...binding}));
        const token=randomBytes(24).toString('hex');
        constructionGrants.set(token,{authorization,binding,expires:Date.now()+300000});
        value.csrf_token=token;
        res.writeHead(200,{'content-type':'application/json','cache-control':'no-store'});
        res.end(JSON.stringify(value));return;
      }
      if(construction && url.pathname==='/gateway/v1/native-lot/construction') {
        assert.equal(req.method,'POST');assert.equal(req.headers.origin,origin);
        assert.equal(req.headers['content-type'],'application/vnd.wonderland.construction-call');
        const authorization=req.headers.authorization??'';
        const grant=constructionGrants.get(req.headers['x-wonderland-csrf']);
        assert.ok(grant && grant.authorization===authorization && grant.expires>=Date.now());
        const session=await current(authorization);assert.ok(matches(session,grant.binding));
        let length=0;const chunks=[];
        for await(const part of req){length+=part.length;assert.ok(length<=131200,'Construction request size');chunks.push(part);}
        assert.ok(length>=16);
        const value=await transact(async()=>{
          // Admission and completion are test-only trusted providers in Rust.
          // The endpoint cannot accept browser prices, receipts or grants.
          const result=await execute({op:'construction',...grant.binding,hold_completion:holdConstruction,request:Buffer.concat(chunks,length).toString('hex'),now_ms:Math.floor(performance.now())});
          stats.constructionCalls++;stats.constructionBuilds=result.builds;if(result.kind==='confirm')holdConstruction=false;
          if(dropConstruction && result.kind==='confirm'){
            dropConstruction=false;stats.constructionLostReplies++;
            // Commit exists only in the authority. Lose the HTTP reply and both
            // update frames; reconnect must obtain the checkpoint, not retry.
            for(const ws of clients){ws.live=false;ws.close(1012);}
            return null;
          }
          for(const transition of result.transitions)broadcast(transition);
          return result;
        });
        if(value===null){res.destroy();return;}
        res.writeHead(200,{'content-type':'application/vnd.wonderland.construction-reply','cache-control':'no-store'});
        res.end(Buffer.from(value.reply,'hex'));return;
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
            const count=recoveryTail;recoveryTail=0;
            const result=await execute({op:count?'checkpoint_tail':'checkpoint',request:data.toString('hex'),count});
            if(count){assert.equal(result.tail_count,count);stats.recoveryTailFrames=(stats.recoveryTailFrames??0)+count;stats.ticks+=count;}
            send(ws,packet(result));ws.live=true;stats.checkpoints++;clearTimeout(timeout);
          } else if(magic==='WLC1') {
            assert.ok(ws.live);stats.actions++;
            // Deterministically reproduce ordinary server ticks while a chosen
            // browser action is in flight. The original bytes remain unchanged.
            for(let i=0;i<actionDelayTicks;i++){broadcast(await execute({op:'tick'}));stats.ticks++;}
            const result=await execute({op:'action',request:data.toString('hex')});broadcast(result.transition);if(result.accepted)stats.accepted++;
            if(dropNext){dropNext=false;stats.unknownDrops++;ws.close(1012);return;}
            // Test-only loss of a receipt while the same socket remains open.
            // Accepted state/events and subsequent ticks are still delivered.
            if(loseNext){loseNext=false;stats.silentReceipts++;return;}
            send(ws,Buffer.from(result.receipt,'hex'));
          } else throw Error('Unexpected native client protocol');
        }catch(error){console.error('Native fixture rejected:',error.message);ws.close(1008);}
        finally{queued--;}
      }).catch(()=>ws.close(1011));
    });
    ws.on('close',()=>{clearTimeout(timeout);clients.delete(ws);});ws.on('error',()=>ws.close());
  });
  await new Promise(resolve=>server.listen(port,'127.0.0.1',resolve));
  const timer=manualTicks?null:setInterval(()=>{if([...clients].some(ws=>ws.live))transact(async()=>{broadcast(await execute({op:'tick'}));stats.ticks++;}).catch(()=>{});},100);
  return {
    origin,stats,
    reset:()=>transact(async()=>broadcast(await execute({op:'reset_needs'}))),
    setHidden:value=>transact(async()=>broadcast(await execute({op:'hidden',value}))),
    // Test harness control only, never an HTTP route or browser-side state edit.
    burst:count=>transact(async()=>{
      assert.ok(manualTicks && Number.isInteger(count) && count>=1 && count<=64);
      const result=await execute({op:'burst',count});assert.equal(result.count,count);
      broadcast(result);stats.ticks+=count;stats.bursts++;
      return {count,tick:result.tick,hash:result.hash,packetSha256:await import('node:crypto').then(m=>m.createHash('sha256').update(packet(result)).digest('hex'))};
    }),
    // Only the isolated manual-tick harness can ask its real Rust authority
    // for a nonempty recovery tail. Never an HTTP route or edited packet.
    recoverWithTail:count=>{
      assert.ok(manualTicks && clients.size===0 && recoveryTail===0);
      assert.ok(Number.isInteger(count) && count>=1 && count<=64);
      recoveryTail=count;
    },
    holdNextConstruction:()=>{assert.ok(construction);holdConstruction=true;},
    completeConstruction:()=>transact(async()=>{assert.ok(construction);const result=await execute({op:'construction_complete'});broadcast(result);return result;}),
    dropNextConstructionReply:()=>{assert.ok(construction);dropConstruction=true;},
    dropNextReceipt:()=>{dropNext=true;},
    loseNextReceipt:()=>{loseNext=true;},
    disconnect:()=>{for(const ws of clients)ws.close(1012);},
    state:()=>transact(()=>execute({op:'state'})),
    active:()=>clients.size,
    close:async()=>{if(timer!==null)clearInterval(timer);for(const ws of [...clients,...relays])ws.terminate();native.close();legacy.close();child.kill();await new Promise(resolve=>server.close(resolve));},
  };
}
