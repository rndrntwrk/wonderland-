import {assertPreviewActive,type PreviewSettings} from "./preview-policy.ts";

// One durable window per preview ID. Neither traffic nor a process/DO restart
// renews it. These hooks are implemented by the actual Container DO below.
export interface PreviewHooks {
  load:()=>Promise<PreviewSettings|undefined>;
  save:(settings:PreviewSettings)=>Promise<void>;
  alarm:(when:number)=>Promise<void>;
  stop:()=>Promise<void>;
}
export async function retainPreviewWindow(settings:PreviewSettings,hooks:PreviewHooks,now=Date.now()) {
  const previous=await hooks.load();
  if(previous && JSON.stringify(previous)!==JSON.stringify(settings))throw Error("Preview window/bindings changed");
  if(!previous)await hooks.save(settings);
  if(now>=settings.expires){await hooks.stop();throw Error("Preview expired");}
  assertPreviewActive(settings,now);
  // Persist the stop alarm BEFORE any paid process starts. An alarm failure
  // never authorizes a start. Alarm delivery is not represented as a billing cap.
  await hooks.alarm(settings.expires);
}
export async function stopExpiredPreview(settings:PreviewSettings,hooks:PreviewHooks,now=Date.now()) {
  if(now<settings.expires){await hooks.alarm(settings.expires);return false;}
  await hooks.stop();return true;
}
