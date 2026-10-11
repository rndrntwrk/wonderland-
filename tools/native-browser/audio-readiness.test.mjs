import test from 'node:test';
import assert from 'node:assert/strict';
import {playbackReady} from './audio-readiness.mjs';
import {BrowserAudio} from '../../crates/audio-runtime/browser/browser-audio.mjs';

const ready=()=>({state:'running',activeVoices:1,pendingStarts:0,pendingDecodes:0,errors:0,
 voices:[{id:'9007199254740993:1',status:'playing',node:true,gain:true,pan:true}]});

test('a retained voice is not proof of a playing AudioBufferSourceNode',()=>{
 for(const status of ['queued','loading','ready','paused']) {
  const s=ready();s.voices[0].status=status;
  assert.equal(playbackReady(s),false,`must reject ${status}`);
 }
});
test('playback observation needs a running context and a complete real node graph',()=>{
 for(const state of ['suspended','locked','interrupted','disposed']) {
  assert.equal(playbackReady({...ready(),state}),false);
 }
 for(const part of ['node','gain','pan']) {
  const s=ready();s.voices[0][part]=false;assert.equal(playbackReady(s),false,part);
 }
});
test('pending work, duplicate voices and device errors cannot satisfy readiness',()=>{
 for(const changed of [{pendingDecodes:1},{pendingStarts:1},{errors:1},{voices:[]},
  {voices:[...ready().voices,...ready().voices]},{activeVoices:0}])
  assert.equal(playbackReady({...ready(),...changed}),false);
});
test('one actual playing voice keeps its exact decimal identity',()=>{
 assert.equal(playbackReady(ready()),true);
 assert.equal(ready().voices[0].id,'9007199254740993:1');
});
test('shipping BrowserAudio exposes activeVoices=1 before sample loading finishes',async()=>{
 let resolve;const pending=new Promise(r=>resolve=r);
 const context={state:'running',addEventListener(){},removeEventListener(){},async resume(){},async close(){this.state='closed';}};
 const audio=new BrowserAudio({contextFactory:()=>context,loadSample:()=>pending});
 await audio.unlockFromGesture();
 audio.apply({Start:{voice:{generation:1,serial:1},sample:Array(32).fill(1),group:'Fx',gain:.5,pan:0,looped:true,seek_frame:0}});
 await new Promise(setImmediate);
 try {
  const snapshot={...audio.snapshot(),voices:[...audio._voices.values()].map(v=>
   ({id:v.id,status:v.status,node:!!v.node,gain:!!v.gainNode,pan:!!v.panNode}))};
  assert.equal(snapshot.activeVoices,1,'exact old gate succeeds');
  assert.equal(snapshot.voices[0].status,'loading','real provider has not returned');
  assert.equal(playbackReady(snapshot),false,'measurement must not run on that old gate');
 }finally{await audio.dispose();resolve(null);await audio.settled();}
});

test('bounded readiness waits for delayed nodes rather than adding a fixed delay',async()=>{
 const {waitForPlayback}=await import('./audio-readiness.mjs');let clock=0,reads=0;
 const result=await waitForPlayback(()=>{reads++;const s=ready();if(clock<450){s.voices[0].status='loading';s.pendingDecodes=1;}return s;},
  {timeout:1000,interval:50,now:()=>clock,pause:async ms=>{clock+=ms;}});
 assert.equal(result.elapsedMs,450);assert.equal(reads,10);assert.equal(result.snapshot.voices[0].id,'9007199254740993:1');
});
test('permanent non-playing voice fails closed at the observation deadline',async()=>{
 const {waitForPlayback}=await import('./audio-readiness.mjs');let clock=0;
 const s=ready();s.voices[0].status='loading';
 await assert.rejects(waitForPlayback(()=>s,{timeout:500,interval:75,now:()=>clock,pause:async ms=>{clock+=ms;}}),/No actual playing native voice before deadline/);
 assert.equal(clock,500);
});
test('ready output is not retried to hide errors, duplicates or an expired budget',async()=>{
 const {waitForPlayback}=await import('./audio-readiness.mjs');let reads=0;
 for(const bad of [{...ready(),errors:1},{...ready(),activeVoices:2}]) {
  await assert.rejects(waitForPlayback(()=>{reads++;return bad;}),/Audio failed/);
 }
 assert.equal(reads,2);
 let clock=0;
 await assert.rejects(waitForPlayback(()=>{clock=501;return ready();},{timeout:500,now:()=>clock}),/before deadline/);
});
test('already playing needs no arbitrary pause and invalid budgets do not read',async()=>{
 const {waitForPlayback}=await import('./audio-readiness.mjs');let pauses=0,reads=0;
 const read=()=>{reads++;return ready();};
 assert.equal((await waitForPlayback(read,{pause:async()=>{pauses++;}})).polls,1);
 for(const options of [{timeout:0},{timeout:12001},{timeout:NaN},{interval:0},{interval:1001}])
  await assert.rejects(waitForPlayback(read,options),/Invalid playback/);
 assert.equal(pauses,0);assert.equal(reads,1);
});
test('actual audio measurement and permanent CI are wired to readiness',async()=>{
 const {readFile}=await import('node:fs/promises');
 const source=await readFile(new URL('./verify-audio.mjs',import.meta.url),'utf8');
 assert.match(source,/import \{waitForPlayback,observePlayback\} from '\.\/audio-readiness\.mjs'/);
 assert.match(source,/async function measure\(\)\{\s*const ready=await waitForPlayback\(\(\)=>observePlayback\(page\)\)/);
 assert.match(source,/ready\.snapshot\.voices\[0\]\.id/);
 assert.match(source,/finally\{pan\.disconnect\(analyser\);analyser\.disconnect\(\);\}/);
 const workflow=await readFile(new URL('../../.github/workflows/native-browser.yml',import.meta.url),'utf8');
 assert.match(workflow,/node --test[^\n]*tools\/native-browser\/\*\.test\.mjs/);
});
