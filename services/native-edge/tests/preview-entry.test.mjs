import assert from 'node:assert/strict';
import test from 'node:test';
import {readFile} from 'node:fs/promises';
import {build} from 'esbuild';
import {createHash} from 'node:crypto';

// Offline adapter tests, not Container execution. Preserve the actual auth/lot
// modules and crypto WASM; replace only unavailable Cloudflare base classes.
const bundle=await build({entryPoints:[new URL('../src/hosted/preview-entry.ts',import.meta.url).pathname],
 bundle:true,format:'esm',platform:'node',write:false,plugins:[{name:'test-bindings',setup(b){
  b.onResolve({filter:/^cloudflare:workers$/},()=>({path:'base',namespace:'fake-cf'}));
  b.onLoad({filter:/.*/,namespace:'fake-cf'},()=>({contents:'export class DurableObject{constructor(ctx,env){this.ctx=ctx;this.env=env}};export class WorkerEntrypoint extends DurableObject{}'}));
  b.onResolve({filter:/account_crypto_bg\.wasm$/},args=>({path:new URL('../generated/account_crypto_bg.wasm',import.meta.url).pathname,namespace:'real-wasm'}));
  b.onLoad({filter:/.*/,namespace:'real-wasm'},async args=>({contents:'export default new WebAssembly.Module(Buffer.from('+JSON.stringify((await readFile(args.path)).toString('base64'))+',"base64"));'}));
 }}]});
const module=await import('data:text/javascript;base64,'+Buffer.from(bundle.outputFiles[0].text).toString('base64'));
const sha=b=>createHash('sha256').update(b).digest('hex');
function fixture(){
 const now=Date.now(),origin='https://fixture.invalid';
 const plan=Buffer.from(JSON.stringify({schema_version:1,public_origin:origin,manifest:{lot:{lot_id:'41'}},files:[{name:'runtime/config.json',size:2,sha256:sha(Buffer.from('{}'))}]}));
 const calls=[],store=new Map();let initialized;
 const container={running:false,images:{native:'registry.fixture/image@sha256:'+'a'.repeat(64)},
  async interceptOutboundHttp(host,binding){calls.push(['intercept',host,binding]);},
  start(options){calls.push(['start',options]);this.running=true;},
  async setInactivityTimeout(ms){calls.push(['inactivity',ms]);},
  async destroy(){calls.push(['destroy']);this.running=false;},
  getTcpPort(port){assert.equal(port,8080);return {fetch:async request=>{
   const r=request instanceof Request?request:new Request(request);
   if(new URL(r.url).pathname==='/__native-preview/ready'){calls.push(['ready']);return new Response('ready');}
   calls.push(['forward',r.url,r.method,r.headers.get('cf-connecting-ip'),await r.text()]);return new Response('actual forwarded response');
  }};}};
 const namespace={idFromName:name=>({name,equals:other=>name===other.name}),get:()=>({fetch:async()=>new Response('native stub')})};
 const env={NATIVE_PREVIEW_APPROVAL:'approved-one-hour-v1',NATIVE_PREVIEW_ID:'fixture-preview-0001',NATIVE_PREVIEW_ORIGIN:origin,
  NATIVE_PREVIEW_START_MS:String(now-1000),NATIVE_PREVIEW_EXPIRES_MS:String(now+3599000),NATIVE_BOOTSTRAP_SHA256:sha(plan),
  LOT_NAME:'41',ACCOUNT_REALM:'fixture',NATIVE_ACCOUNT_HTTP:'staged-v1',NATIVE_ACCOUNT_ORIGIN:origin,NATIVE_ACCOUNT_REALM:'fixture',
  NATIVE_PREVIEW:namespace,COORD:{idFromName:x=>x,get:()=>({inspectControlledStartupV1:async()=>({schema_version:1,mode:'recover'})})},
  ACCOUNTS:{},CHECKPOINTS:{},ASSETS:{fetch:async()=>new Response('static module')},
  GAME_CONTENT:{get:async key=>{calls.push(['r2',key]);return {size:plan.length,arrayBuffer:async()=>plan.buffer.slice(plan.byteOffset,plan.byteOffset+plan.length)};}}};
 const ctx={id:namespace.idFromName(env.NATIVE_PREVIEW_ID),container,
  storage:{get:async key=>store.get(key),put:async(key,value)=>{store.set(key,structuredClone(value));calls.push(['persist']);},
   setAlarm:async when=>calls.push(['alarm',when])},
  exports:{NativePreviewOutbound:options=>({privateProps:options.props})},
  blockConcurrencyWhile(fn){initialized=fn();}};
 return {origin,env,ctx,calls,container,ready:()=>initialized};
}
test('public entry serves static modules and never exposes private/bootstrap/registration routes',async()=>{
 const f=fixture();assert.equal(await (await module.default.fetch(new Request(f.origin+'/native-account.mjs'),f.env)).text(),'static module');
 for(const path of ['/__native-preview/ready','/native-account/register','/native-content/file/bad'])
  assert.equal((await module.default.fetch(new Request(f.origin+path),f.env)).status,404);
 const inactive={...f.env,NATIVE_PREVIEW_APPROVAL:undefined};assert.equal((await module.default.fetch(new Request(f.origin+'/'),inactive)).status,503);
 assert.equal(f.calls.some(r=>r[0]==='start'),false);
});
test('durable approval and pinned bootstrap precede paid start; original POST is forwarded exactly once',async()=>{
 const f=fixture(),host=new module.NativePreview(f.ctx,f.env);await f.ready();
 const request=()=>new Request(f.origin+'/native-account/login',{method:'POST',headers:{origin:f.origin,'cf-connecting-ip':'192.0.2.1'},body:'exact body'});
 assert.equal(await (await host.fetch(request())).text(),'actual forwarded response');
 assert.equal((await host.fetch(request())).status,200);
 const starts=f.calls.filter(x=>x[0]==='start');assert.equal(starts.length,1);
 assert.equal(starts[0][1].enableInternet,false);assert.equal(starts[0][1].env.WONDERLAND_START_MODE,'recover');
 assert.ok(f.calls.findIndex(x=>x[0]==='alarm')<f.calls.findIndex(x=>x[0]==='start'));
 assert.ok(f.calls.findIndex(x=>x[0]==='r2')<f.calls.findIndex(x=>x[0]==='start'));
 assert.deepEqual(f.calls.filter(x=>x[0]==='forward').map(x=>x.slice(2)),[['POST','192.0.2.1','exact body'],['POST','192.0.2.1','exact body']]);
 assert.ok(f.calls.filter(x=>x[0]==='forward').every(x=>x[1]==='http://container/native-account/login'));
 assert.equal(f.calls.find(x=>x[0]==='intercept')[1],'owned-private.invalid');
});
test('missing pinned bootstrap prevents process start and original request forwarding',async()=>{
 const f=fixture();f.env.GAME_CONTENT.get=async()=>null;
 const host=new module.NativePreview(f.ctx,f.env);await f.ready();
 const reply=await host.fetch(new Request(f.origin+'/native-account/login',{method:'POST',headers:{origin:f.origin,'cf-connecting-ip':'192.0.2.1'}}));
 assert.equal(reply.status,503);assert.equal(f.calls.some(x=>['start','forward'].includes(x[0])),false);
});
test('private binding requires the trusted preview props and does not serve browser-origin bootstrap',async()=>{
 const f=fixture(),wrong=new module.NativePreviewOutbound({props:{previewId:'wrong'}},f.env);
 await assert.rejects(wrong.fetch(new Request('http://owned-private.invalid/bootstrap')),/Wrong private/);
 const privateEntry=new module.NativePreviewOutbound({props:{previewId:f.env.NATIVE_PREVIEW_ID}},f.env);
 const reply=await privateEntry.fetch(new Request('http://owned-private.invalid/bootstrap',{headers:{origin:f.origin}}));
 assert.equal(reply.status,403);
 assert.equal((await privateEntry.fetch(new Request('http://owned-private.invalid/blob/'+'b'.repeat(64)))).status,404);
});
test('actual startup classifier rejects inconsistent or non-pristine checkpoint loss',()=>{
 function lot({state=true,checkpoint=false,revision='0',control=false,batches=false,actors=false}={}){
  const instance=Object.create(module.LotCoordinator.prototype);
  instance.controlledConfiguration=()=>({});instance.controlState=()=>({revision});
  instance.ctx={storage:{transactionSync:fn=>fn(),sql:{exec:query=>({toArray:()=>{
   if(query.includes('FROM lot_state'))return state?[{}]:[];
   if(query.includes('FROM lot_control_state'))return control?[{}]:[];
   if(query.includes('FROM checkpoints'))return checkpoint?[{}]:[];
   if(query.includes('FROM batches'))return batches?[{}]:[];
   if(query.includes('FROM actor_controls'))return actors?[{}]:[];
   return [];
  }})}}};return instance;
 }
 assert.equal(lot({state:false}).inspectControlledStartupV1().mode,'initialize');
 assert.equal(lot({checkpoint:true}).inspectControlledStartupV1().mode,'recover');
 assert.equal(lot().inspectControlledStartupV1().mode,'initialize');
 for(const options of [{state:false,checkpoint:true},{state:false,control:true},{revision:'1'},{batches:true},{actors:true}])
  assert.throws(()=>lot(options).inspectControlledStartupV1());
});
