// Test-only SQLite driver. No production entrypoint imports this file.
import { DurableObject } from 'cloudflare:workers';
import { LotChatStore, chatAudience } from '../src/lot/chat';
export class TestChat extends DurableObject {
  private chat:LotChatStore;
  constructor(ctx:DurableObjectState,env:unknown){super(ctx,env);this.chat=new LotChatStore(ctx.storage);}
  run(v:any){return this.ctx.storage.transactionSync(()=>{
    if(v.action==='read')return this.chat.read(v.avatar);
    if(v.action==='audience'){const r=this.chat.read(v.avatar);return {...r,messages:chatAudience(r.messages,v.message)};}
    const r=this.chat.append(v.avatar,v.name,v.message,123);
    if(v.action==='rollback')throw Error('interrupted before durable acknowledgement');
    return r;
  });}
}
export default {async fetch(request:Request,env:any){
  const v=await request.json() as any;
  try{return Response.json(await env.CHAT.getByName(v.room).run(v));}
  catch{return new Response(null,{status:409});}
}};
