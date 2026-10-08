// Standalone Chromium/WebCrypto conformance for the production export helper.
// This is not the compiled Wonderland application or full browser acceptance.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createServer} from 'node:http';
import {readFile,writeFile,mkdir,mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {spawn} from 'node:child_process';
import {setTimeout as sleep} from 'node:timers/promises';
const helper=await readFile(new URL('../../public/facade-links.mjs',import.meta.url));
const profile=await mkdtemp(join(tmpdir(),'wonderland-facade-browser-'));
const output=resolve(process.env.WONDERLAND_FACADE_BROWSER_REPORT||'tests/output/facade-integrity/browser-report.json');
await mkdir(dirname(output),{recursive:true});
const report={schema:1,scope:'Standalone production facade helper with real Chromium/WebCrypto; not the compiled game',sourceSha256:createHash('sha256').update(helper).digest('hex'),checks:[],errors:[]};
const server=createServer((req,res)=>{
 if(req.url==='/facade-links.mjs'){res.writeHead(200,{'Content-Type':'text/javascript','Cache-Control':'no-store'});res.end(helper);}
 else if(req.url==='/'){res.writeHead(200,{'Content-Type':'text/html','Cache-Control':'no-store'});res.end('<!doctype html><meta charset="utf-8"><title>Facade integrity conformance</title>');}
 else {res.writeHead(req.url==='/favicon.ico'?204:404);res.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
let child,socket,next=0,log='';const pending=new Map();
let watchdog;
const deadline=Date.now()+30000;
async function until(fn){while(Date.now()<deadline){const value=await fn();if(value)return value;await sleep(50);}throw new Error('Chromium conformance deadline exceeded');}
function call(method,params={},sessionId){return new Promise((resolve,reject)=>{const id=++next;pending.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params,...(sessionId?{sessionId}:{})}));});}
try {
 child=spawn(process.env.WONDERLAND_CHROMIUM||'chromium',[
  '--headless',...(process.getuid?.()===0?['--no-sandbox']:[]),'--disable-dev-shm-usage',
  '--remote-debugging-port=0',`--user-data-dir=${profile}`,'about:blank',
 ],{stdio:['ignore','ignore','pipe']});
 child.on('error',error=>{report.errors.push(String(error));});
 child.stderr.on('data',data=>{log=(log+data).slice(-12000);});
 watchdog=setTimeout(()=>{for(const waiter of pending.values())waiter.reject(new Error('Browser execution deadline'));child.kill('SIGTERM');},30000);
 const active=await until(async()=>{try{return (await readFile(join(profile,'DevToolsActivePort'),'utf8')).trim().split('\n');}catch{return null;}});
 socket=new WebSocket(`ws://127.0.0.1:${active[0]}${active[1]}`);
 await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
 socket.addEventListener('message',event=>{
  const msg=JSON.parse(event.data);const waiter=pending.get(msg.id);if(!waiter)return;pending.delete(msg.id);
  if(msg.error)waiter.reject(new Error(JSON.stringify(msg.error)));else waiter.resolve(msg.result);
 });
 report.browser=await call('Browser.getVersion');
 const {targetId}=await call('Target.createTarget',{url:'about:blank'});
 const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});
 const url=`http://127.0.0.1:${server.address().port}/`;
 await call('Page.enable',{},sessionId);
 const nav=await call('Page.navigate',{url},sessionId);if(nav.errorText)throw new Error(nav.errorText);
 await until(async()=>{
  const value=await call('Runtime.evaluate',{expression:`location.href === ${JSON.stringify(url)} && document.readyState === 'complete'`,returnByValue:true},sessionId);
  return value.result.value;
 });
 const execution=await call('Runtime.evaluate',{
  expression:`(async()=>{
   const {makeFacadeLinks,releaseFacadeLinks}=await import('/facade-links.mjs');
   const checks=[];const ensure=(test,message)=>{if(!test)throw new Error(message);};
   const same=(a,b)=>a.length===b.length&&a.every((v,i)=>v===b[i]);
   const hash=async data=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',data)),b=>b.toString(16).padStart(2,'0')).join('');
   const key='a'.repeat(64),data=new Uint8Array([70,83,79,102,1,0,0,0,1,23,42]);
   const meta={kind:'presentation_facade',source_hash:key,bytes:data.length,sha256:await hash(data),not_a_game_save:true};
   ensure(isSecureContext,'WebCrypto needs the real secure loopback context');
   const urls=await makeFacadeLinks(data,JSON.stringify(meta),key);
   ensure(same(new Uint8Array(await(await fetch(urls[0])).arrayBuffer()),data),'valid bytes differ');
   ensure(await(await fetch(urls[1])).text()===JSON.stringify(meta),'receipt differs');
   checks.push('real WebCrypto digest and both downloaded Blob contents');
   releaseFacadeLinks(...urls);
   let revoked=false;try{await fetch(urls[0]);}catch{revoked=true;}ensure(revoked,'discard did not revoke file URL');
   checks.push('discard revokes the actual Blob URL');
   const changed=data.slice();changed[10]^=1;let rejected=false;
   try{await makeFacadeLinks(changed,JSON.stringify(meta),key);}catch(error){rejected=/checksum/.test(error.message);}
   ensure(rejected,'same-size corruption was accepted');checks.push('same-size corruption is rejected');
   const temporary=data.slice(),preimage=temporary.slice();const request=makeFacadeLinks(temporary,JSON.stringify(meta),key);temporary[9]^=1;
   const copied=await request;
   try{ensure(same(new Uint8Array(await(await fetch(copied[0])).arrayBuffer()),preimage),'borrowed source changed across await');}
   finally{releaseFacadeLinks(...copied);}
   checks.push('temporary caller memory is copied before the first await');
   const first=makeFacadeLinks(data,JSON.stringify(meta),key),second=makeFacadeLinks(data,JSON.stringify(meta),key);
   let bounded=false;try{await makeFacadeLinks(data,JSON.stringify(meta),key);}catch(error){bounded=/queue is full/.test(error.message);}
   const completed=await Promise.all([first,second]);for(const result of completed)releaseFacadeLinks(...result);
   ensure(bounded,'hash queue admitted a third retained copy');checks.push('real concurrent hashes preserve the two-copy budget');
   const again=await makeFacadeLinks(data,JSON.stringify(meta),key);releaseFacadeLinks(...again);checks.push('completed hashes release admission capacity');
   return {checks,secureContext:isSecureContext};
  })()`,awaitPromise:true,returnByValue:true,
 },sessionId);
 if(execution.exceptionDetails)throw new Error(execution.exceptionDetails.exception?.description||JSON.stringify(execution.exceptionDetails));
 assert.equal(execution.result.value.checks.length,6);Object.assign(report,execution.result.value);report.status='passed';
} catch(error){report.status='failed';report.errors.push(String(error.stack||error));process.exitCode=1;}
finally {
 clearTimeout(watchdog);
 for(const waiter of pending.values())waiter.reject(new Error('Browser owner disposed'));pending.clear();
 socket?.close();
 if(child&&child.exitCode===null){child.kill('SIGTERM');await Promise.race([new Promise(resolve=>child.once('exit',resolve)),sleep(2000)]);if(child.exitCode===null)child.kill('SIGKILL');}
 await new Promise(resolve=>server.close(resolve));
 await rm(profile,{recursive:true,force:true});
 report.browserLog=log;
 await writeFile(output,JSON.stringify(report,null,2)+'\n');
 console.log(JSON.stringify({status:report.status,checks:report.checks.length,errors:report.errors,report:output}));
}
