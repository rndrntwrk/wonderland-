// TEST ONLY. Voice admission and asynchronous sample/node readiness are distinct.
// This observer never starts, resumes, retries or otherwise changes playback.
import {setTimeout as sleep} from 'node:timers/promises';

export function playbackReady(s) {
 return !!s && s.state==='running' && s.activeVoices===1 && s.errors===0 &&
  s.pendingStarts===0 && s.pendingDecodes===0 && Array.isArray(s.voices) &&
  s.voices.length===1 && typeof s.voices[0]?.id==='string' &&
  /^[1-9][0-9]*:[1-9][0-9]*$/.test(s.voices[0].id) &&
  s.voices[0].status==='playing' && s.voices[0].node===true &&
  s.voices[0].gain===true && s.voices[0].pan===true;
}

export async function waitForPlayback(read, {timeout=12000,interval=20,
 now=()=>performance.now(),pause=sleep}={}) {
 if(typeof read!=='function'||!Number.isFinite(timeout)||timeout<=0||timeout>12000||
   !Number.isFinite(interval)||interval<=0||interval>1000)
  throw new Error('Invalid playback observation budget');
 const started=now();let last=null,polls=0;
 while(true) {
  last=await read();polls++;
  const elapsed=now()-started;
  if(last?.errors>0||last?.activeVoices>1)
   throw new Error('Audio failed before measurement: '+JSON.stringify(last));
  if(elapsed<=timeout && playbackReady(last))return {snapshot:last,polls,elapsedMs:elapsed};
  if(elapsed>=timeout)throw new Error('No actual playing native voice before deadline: '+JSON.stringify(last));
  await pause(Math.min(interval,timeout-elapsed));
 }
}

export async function observePlayback(page) {
 return page.evaluate(async()=>{
  const {acceptedAudioHost}=await import('/audio/source-audio.mjs');
  const host=acceptedAudioHost(),backend=host.backend;
  const s=host.snapshot();
  return {state:s.state,activeVoices:s.activeVoices,pendingStarts:s.pendingStarts,
   pendingDecodes:s.pendingDecodes,errors:s.errors,lastError:s.lastError,
   voices:[...backend._voices.values()].slice(0,2).map(v=>({id:v.id,status:v.status,
    node:!!v.node,gain:!!v.gainNode,pan:!!v.panNode}))};
 });
}
