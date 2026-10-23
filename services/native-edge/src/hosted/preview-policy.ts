export const MAX_PREVIEW_MS = 60 * 60 * 1000;
export interface PreviewSettings {
  id: string; origin: string; starts: number; expires: number;
  bootstrapSha256: string; lot: string; realm: string;
}
export interface PreviewEnv {
  NATIVE_PREVIEW_APPROVAL?: string; NATIVE_PREVIEW_ID?: string;
  NATIVE_PREVIEW_ORIGIN?: string; NATIVE_PREVIEW_START_MS?: string;
  NATIVE_PREVIEW_EXPIRES_MS?: string; NATIVE_BOOTSTRAP_SHA256?: string;
  LOT_NAME?: string; ACCOUNT_REALM?: string;
}
export function previewSettings(env: PreviewEnv): PreviewSettings {
  if (env.NATIVE_PREVIEW_APPROVAL !== "approved-one-hour-v1") throw Error("Preview inactive");
  const id=env.NATIVE_PREVIEW_ID,origin=env.NATIVE_PREVIEW_ORIGIN;
  if (!id || !/^[a-z0-9-]{16,64}$/.test(id) || !origin) throw Error("Invalid preview identity");
  const url=new URL(origin);
  if(url.protocol!=="https:"||url.origin!==origin)throw Error("Invalid preview origin");
  const parse=(v:string|undefined)=>{
    if(!v||!/^\d{13}$/.test(v)||!Number.isSafeInteger(Number(v)))throw Error("Invalid preview time");
    return Number(v);
  };
  const starts=parse(env.NATIVE_PREVIEW_START_MS),expires=parse(env.NATIVE_PREVIEW_EXPIRES_MS);
  if(expires<=starts||expires-starts>MAX_PREVIEW_MS)throw Error("Preview exceeds one hour");
  if(!env.NATIVE_BOOTSTRAP_SHA256||!/^[0-9a-f]{64}$/.test(env.NATIVE_BOOTSTRAP_SHA256)
    ||!env.LOT_NAME||!/^[1-9]\d{0,19}$/.test(env.LOT_NAME)
    ||!env.ACCOUNT_REALM||!/^[a-z0-9-]{1,64}$/.test(env.ACCOUNT_REALM))throw Error("Invalid preview binding");
  return {id,origin,starts,expires,bootstrapSha256:env.NATIVE_BOOTSTRAP_SHA256,lot:env.LOT_NAME,realm:env.ACCOUNT_REALM};
}
export function assertPreviewActive(settings:PreviewSettings,now=Date.now()) {
  if(now<settings.starts||now>=settings.expires)throw Error("Preview outside approved window");
}
const postPaths=new Set(["/native-account/login","/native-account/roster","/native-account/logout",
  "/native-account/rotate","/native-account/create","/native-account/enter","/native-account/reconnect","/native-account/leave",
  "/native-content/manifest"]);
export function isNativeRequest(request:Request,settings:PreviewSettings):boolean {
  const url=new URL(request.url);
  if(url.origin!==settings.origin||url.search||url.hash||url.pathname.includes("%"))return false;
  if(url.pathname==="/native-lot/stream")return request.method==="GET"
    &&request.headers.get("upgrade")?.toLowerCase()==="websocket"&&request.headers.get("origin")===settings.origin;
  return request.method==="POST"&&request.headers.get("origin")===settings.origin
    &&(postPaths.has(url.pathname)||/^\/native-content\/file\/[0-9a-f]{64}$/.test(url.pathname));
}
export function privatePrefix(settings:PreviewSettings){return `native-preview/${settings.id}/`;}
