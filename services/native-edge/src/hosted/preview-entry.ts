// Dedicated preview entry. No paid start is possible with omitted/inactive vars.
// Uses the official direct DO Container API; no SDK or public supervisor RPC.
import {DurableObject,WorkerEntrypoint} from "cloudflare:workers";
import {AccountAuthority} from "../account/authority";
import {LotCoordinator} from "../lot/coordinator";
import {accountHttp} from "../http/accounts";
import {dispatchSupervisorRpc} from "./supervisor-rpc";
import {createSupervisorOutboundHandler} from "./supervisor-outbound";
import {previewSettings,assertPreviewActive,isNativeRequest,privatePrefix,type PreviewEnv,type PreviewSettings} from "./preview-policy.ts";
import {retainPreviewWindow,stopExpiredPreview} from "./preview-lifecycle.ts";
export {AccountAuthority,LotCoordinator};
interface Env extends PreviewEnv {
  NATIVE_PREVIEW:DurableObjectNamespace<NativePreview>;
  ACCOUNTS:DurableObjectNamespace<AccountAuthority>;
  COORD:DurableObjectNamespace<LotCoordinator>;
  GAME_CONTENT:R2Bucket; CHECKPOINTS:R2Bucket; ASSETS:Fetcher;
  NATIVE_ACCOUNT_HTTP:string; NATIVE_ACCOUNT_ORIGIN:string; NATIVE_ACCOUNT_REALM:string;
}
const unavailable=()=>new Response("Native preview unavailable",{status:503,headers:{"cache-control":"no-store"}});
function boundSettings(env:Env){
  const s=previewSettings(env);
  if(env.NATIVE_ACCOUNT_HTTP!=="staged-v1"||env.NATIVE_ACCOUNT_ORIGIN!==s.origin
    ||env.NATIVE_ACCOUNT_REALM!==s.realm)throw Error("Account HTTP binding mismatch");
  return s;
}
async function sha(bytes:ArrayBuffer){return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)),b=>b.toString(16).padStart(2,"0")).join("");}
async function bootstrap(env:Env,s:PreviewSettings){
  const object=await env.GAME_CONTENT.get(privatePrefix(s)+"bootstrap.json");
  if(!object||object.size>2*1024*1024)throw Error("Missing pinned bootstrap");
  const bytes=await object.arrayBuffer();
  if(await sha(bytes)!==s.bootstrapSha256)throw Error("Bootstrap hash mismatch");
  const plan=JSON.parse(new TextDecoder("utf-8",{fatal:true,ignoreBOM:false}).decode(bytes));
  if(plan.schema_version!==1||!Array.isArray(plan.files)||plan.files.length>64
    ||plan.manifest?.lot?.lot_id!==s.lot||plan.public_origin!==s.origin)throw Error("Bootstrap scope mismatch");
  let total=0;const names=new Set<string>();
  for(const f of plan.files){
    if(typeof f.name!=="string"||!/^[a-zA-Z0-9_./-]{1,192}$/.test(f.name)
      ||f.name.startsWith("/")||f.name.split("/").some((x:string)=>!x||x==="."||x==="..")||names.has(f.name)
      ||!/^[0-9a-f]{64}$/.test(f.sha256)||!Number.isSafeInteger(f.size)||f.size<1||f.size>128*1024*1024)
      throw Error("Invalid bootstrap resource");
    names.add(f.name);total+=f.size;
  }
  if(total>160*1024*1024)throw Error("Bootstrap byte limit");
  return {plan,bytes};
}
// A loopback binding created ONLY by NativePreview, with trusted instance props.
// The default public fetch below never dispatches into this entrypoint.
export class NativePreviewOutbound extends WorkerEntrypoint<Env,{previewId:string}> {
  async fetch(request:Request){
    const s=boundSettings(this.env);assertPreviewActive(s);
    if(this.ctx.props.previewId!==s.id)throw Error("Wrong private preview instance");
    const url=new URL(request.url);
    if(url.origin!=="http://owned-private.invalid"||url.search||url.hash)return new Response(null,{status:404});
    if(request.method==="GET"&&!request.headers.has("origin")){
      const {plan,bytes}=await bootstrap(this.env,s);
      if(url.pathname==="/bootstrap")return new Response(bytes,{headers:{"content-type":"application/json","cache-control":"no-store"}});
      const hash=/^\/blob\/([0-9a-f]{64})$/.exec(url.pathname)?.[1];
      const file=hash&&plan.files.find((f:{sha256:string})=>f.sha256===hash);
      if(!file)return new Response(null,{status:404});
      const object=await this.env.GAME_CONTENT.get(privatePrefix(s)+"blobs/"+hash);
      if(!object||object.size!==file.size)throw Error("Private blob size mismatch");
      return new Response(object.body,{headers:{"content-type":"application/octet-stream","content-length":String(file.size),"cache-control":"no-store"}});
    }
    return createSupervisorOutboundHandler({publicOrigin:s.origin,
      dispatchRpc:body=>dispatchSupervisorRpc(body,async(target,method,args)=>{
        const stub=target==="account"?this.env.ACCOUNTS.get(this.env.ACCOUNTS.idFromName(s.realm)):
          this.env.COORD.get(this.env.COORD.idFromName(s.lot));
        return (stub as unknown as Record<string,(...args:unknown[])=>Promise<unknown>>)[method](...args);
      }),accountHttp:r=>accountHttp(r,this.env)})(request);
  }
}
export class NativePreview extends DurableObject<Env> {
  private settings:PreviewSettings;
  private starting:Promise<void>|null=null;
  constructor(ctx:DurableObjectState,env:Env){
    super(ctx,env);this.settings=boundSettings(env);
    const expected=env.NATIVE_PREVIEW.idFromName(this.settings.id);
    if(!ctx.id.equals(expected))throw Error("Wrong preview Durable Object");
    ctx.blockConcurrencyWhile(async()=>{await retainPreviewWindow(this.settings,this.hooks());});
  }
  private hooks(){return {
    load:()=>this.ctx.storage.get<PreviewSettings>("approved-window-v1"),
    save:(s:PreviewSettings)=>this.ctx.storage.put("approved-window-v1",s),
    alarm:(when:number)=>this.ctx.storage.setAlarm(when),
    stop:async()=>{if(this.ctx.container?.running)await this.ctx.container.destroy();},
  };}
  async alarm(){await stopExpiredPreview(this.settings,this.hooks());}
  private async ensureRunning(){
    if(this.starting)return this.starting;
    this.starting=(async()=>{
      const s=this.settings;await retainPreviewWindow(s,this.hooks());
      const container=this.ctx.container;if(!container)throw Error("Missing Container binding");
      const loopbacks=this.ctx.exports as Cloudflare.Exports & {
        NativePreviewOutbound:LoopbackServiceStub<NativePreviewOutbound>;
      };
      const binding=loopbacks.NativePreviewOutbound({props:{previewId:s.id}});
      // Intercepts expire with the process: reinstall on every start/DO lifetime.
      await container.interceptOutboundHttp("owned-private.invalid",binding);
      if(!container.running){
        await bootstrap(this.env,s);assertPreviewActive(s);
        const status=await this.env.COORD.get(this.env.COORD.idFromName(s.lot)).inspectControlledStartupV1();
        if(status.schema_version!==1||!["initialize","recover"].includes(status.mode))throw Error("Invalid durable startup decision");
        assertPreviewActive(s);
        container.start({image:container.images.native,instance:"standard-2",enableInternet:false,
          env:{WONDERLAND_PUBLIC_ORIGIN:s.origin,WONDERLAND_PREVIEW_EXPIRES_MS:String(s.expires),
            WONDERLAND_BOOTSTRAP_SHA256:s.bootstrapSha256,WONDERLAND_START_MODE:status.mode}});
      }
      await container.setInactivityTimeout(Math.max(1,s.expires-Date.now()));
      const port=container.getTcpPort(8080);
      // Readiness probes are safe GETs. Never replay the original account/action request.
      for(let i=0;i<40;i++){
        assertPreviewActive(s);
        try{if((await port.fetch("http://container/__native-preview/ready")).ok)return;}catch{}
        await new Promise(resolve=>setTimeout(resolve,250));
      }
      throw Error("Native startup is not ready");
    })().finally(()=>{this.starting=null;});
    return this.starting;
  }
  async fetch(request:Request){
    try{
      assertPreviewActive(this.settings);
      if(!isNativeRequest(request,this.settings))return new Response(null,{status:404});
      await this.ensureRunning();assertPreviewActive(this.settings);
      const headers=new Headers(request.headers);
      headers.delete("x-owned-private");headers.delete("x-native-preview");
      headers.delete("host");
      // This header was set at trusted Cloudflare ingress and preserved by the public Worker.
      if(!headers.get("cf-connecting-ip"))throw Error("Missing trusted ingress IP");
      const upstream=new Request("http://container"+new URL(request.url).pathname,request);
      return await this.ctx.container!.getTcpPort(8080).fetch(new Request(upstream,{headers}));
    }catch{if(Date.now()>=this.settings.expires)await this.hooks().stop();return unavailable();}
  }
}
export default {async fetch(request:Request,env:Env){
  try{
    const s=boundSettings(env);assertPreviewActive(s);const url=new URL(request.url);
    if(url.origin!==s.origin)return new Response(null,{status:404});
    if(url.pathname==="/wonderland-config.json")return Response.json({version:1,mode:"native_account",gateway_url:"/native-account",native_lots:true},{headers:{"cache-control":"no-store"}});
    if(["/native-account/","/native-lot/","/native-content/","/__native"].some(prefix=>url.pathname.startsWith(prefix))){
      if(!isNativeRequest(request,s))return new Response(null,{status:404});
      // Never accept alternate forwarded-IP or private capability headers.
      if(!request.headers.get("cf-connecting-ip"))return unavailable();
      return env.NATIVE_PREVIEW.get(env.NATIVE_PREVIEW.idFromName(s.id)).fetch(request);
    }
    if(request.method!=="GET"||url.search)return new Response(null,{status:404});
    return env.ASSETS.fetch(request);
  }catch{return unavailable();}
}};
