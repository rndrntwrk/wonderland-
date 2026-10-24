// Test-only persisted pagination/migration inputs. Never a production entrypoint.
import base from "./account-http-driver";
import {AccountAuthority} from "../src/account/authority";
import type {AccountHttpEnv} from "../src/http/accounts";
export class AssetFixtureAccounts extends AccountAuthority {
  fixture(id:string,mode:string) {
    if(mode==='pages')this.ctx.storage.transactionSync(()=>{
      for(let n=0n;n<75n;n++)this.ctx.storage.sql.exec(
        "INSERT INTO avatar_outfits(id,avatar_id,asset_key,suit_type,source,purchase_price,sale_price) VALUES(?,?,?,?,?,?,?)",
        String(9007199254740993n+n),id,'2',0,'test-pagination','0','0');
    });
    if(mode==='legacy')this.ctx.storage.sql.exec("DELETE FROM avatar_ownership_initializations WHERE avatar_id=?",id);
    return {outfits:this.ctx.storage.sql.exec<{n:number}>("SELECT COUNT(*) AS n FROM avatar_outfits WHERE avatar_id=?",id).one().n,
      initializations:this.ctx.storage.sql.exec<{n:number}>("SELECT COUNT(*) AS n FROM avatar_ownership_initializations WHERE avatar_id=?",id).one().n};
  }
}
export default {async fetch(request:Request,env:AccountHttpEnv){
  if(new URL(request.url).pathname==='/test-assets-fixture'){
    const v=await request.json() as {id:string;mode:string};
    const stub=env.ACCOUNTS!.get(env.ACCOUNTS!.idFromName(env.NATIVE_ACCOUNT_REALM!)) as unknown as AssetFixtureAccounts;
    return Response.json(await stub.fixture(v.id,v.mode));
  }
  return base.fetch(request,env);
}};
