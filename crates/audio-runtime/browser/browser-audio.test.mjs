import test from 'node:test';
import assert from 'node:assert/strict';
import { BrowserAudio } from './browser-audio.mjs';
// Only the platform AudioContext is controlled. Queue, decode, cancellation,
// restart, ownership and memory decisions exercise the production adapter.
function deferred(){let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no});return {promise,resolve,reject};}
class Context {
  state='suspended';currentTime=0;destination={};nodes=[];starts=[];closed=false;listeners=[];deny=false;buffersCreated=0;
  async resume(){if(this.deny)throw Error('autoplay denied');this.state='running';this.signal();}
  async suspend(){this.state='suspended';this.signal();}
  async close(){this.closed=true;this.state='closed';this.signal();}
  addEventListener(name,fn){this.listeners.push(fn);}
  removeEventListener(name,fn){this.listeners=this.listeners.filter(x=>x!==fn);}
  signal(){for(const fn of this.listeners)fn();}
  createBuffer(channels,length,sampleRate){this.buffersCreated++;const data=Array.from({length:channels},()=>new Float32Array(length));return {numberOfChannels:channels,length,sampleRate,duration:length/sampleRate,getChannelData:c=>data[c]};}
  node(){return {connect(){},disconnect(){this.disconnected=true;}};}
  createGain(){return {...this.node(),gain:{value:1,setValueAtTime(value){this.value=value;}}};}
  createStereoPanner(){return {...this.node(),pan:{value:0,setValueAtTime(value){this.value=value;}}};}
  createBufferSource(){const context=this;const node={...this.node(),loop:false,start(time,offset){this.offset=offset;context.starts.push(this);},stop(){this.stopped=true;this.onended?.();}};this.nodes.push(node);return node;}
  async decodeAudioData(bytes){if(bytes.byteLength===3)throw Error('bad codec');return this.createBuffer(1,4,4);}
}
const sample=n=>Array(32).fill(n);
const start=(serial,key=1,looped=false)=>({Start:{voice:{generation:1,serial},sample:sample(key),group:'Fx',gain:0.5,pan:0,looped,seek_frame:0}});
function make(overrides={}){const context=new Context();let created=0;const adapter=new BrowserAudio({contextFactory(){created++;return context;},loadSample:async()=>({pcm:{sampleRate:4,channels:1,samples:new Int16Array([0,16000,-16000,0])}}),...overrides});return {adapter,context,created:()=>created};}
test('gesture unlock creates and resumes actual context only after explicit unlock',async()=>{
  const {adapter,context,created}=make();adapter.apply(start(1));assert.equal(created(),0);assert.equal(adapter.state,'locked');await adapter.unlockFromGesture();await adapter.settled();assert.equal(created(),1);assert.equal(adapter.state,'running');assert.equal(context.starts.length,1);assert.equal(adapter.snapshot().activeVoices,1);await adapter.dispose();assert.equal(context.closed,true);
});
test('autoplay rejection is explicit and a later gesture retries without duplicating',async()=>{
  const {adapter,context}=make();context.deny=true;adapter.apply(start(1));await assert.rejects(adapter.unlockFromGesture(),/autoplay denied/);assert.equal(context.starts.length,0);context.deny=false;await adapter.unlockFromGesture();await adapter.settled();assert.equal(context.starts.length,1);await adapter.unlockFromGesture();assert.equal(context.starts.length,1);await adapter.dispose();
});
test('out of order asynchronous decode completion retains source start order',async()=>{
  const a=deferred(),b=deferred();const {adapter,context}=make({loadSample:async key=>key[0]===1?a.promise:b.promise});adapter.applyAll([start(1,1),start(2,2)]);await adapter.unlockFromGesture();b.resolve({pcm:{sampleRate:4,channels:1,samples:new Int16Array([2,2])}});await new Promise(setImmediate);assert.equal(context.starts.length,0);a.resolve({pcm:{sampleRate:4,channels:1,samples:new Int16Array([1,1])}});await adapter.settled();assert.equal(context.starts.length,2);assert.ok(context.starts[0].buffer.getChannelData(0)[0]<context.starts[1].buffer.getChannelData(0)[0]);await adapter.dispose();
});
test('stop and reset fence pending decode and stale voice replay',async()=>{
  const d=deferred();const {adapter,context}=make({loadSample:()=>d.promise});adapter.apply(start(1));await adapter.unlockFromGesture();adapter.apply({Stop:{voice:{generation:1,serial:1}}});adapter.reset();d.resolve({pcm:{sampleRate:4,channels:1,samples:new Int16Array([1,1])}});await adapter.settled();assert.equal(context.starts.length,0);assert.equal(adapter.snapshot().pcmBytes,0);assert.throws(()=>adapter.apply(start(1)),/stale|replay/);await adapter.dispose();
});
test('decode and voice budgets reject overflow without hiding codec failure',async()=>{
  const {adapter,context}=make({maxVoices:1,maxPcmBytes:8,loadSample:async()=>({encoded:new Uint8Array(3).buffer,format:'mp3',decodedBytes:8})});adapter.apply(start(1));assert.throws(()=>adapter.apply(start(2)),/voice/);await adapter.unlockFromGesture();await adapter.settled();assert.equal(context.starts.length,0);assert.equal(adapter.snapshot().activeVoices,0);assert.match(adapter.snapshot().lastError,/bad codec/);await adapter.dispose();
});
test('suspend pauses context and queued starts until gesture resume',async()=>{
  const {adapter,context}=make();await adapter.unlockFromGesture();adapter.apply(start(1));await adapter.settled();await adapter.suspend();adapter.apply(start(2));await adapter.settled();assert.equal(adapter.state,'suspended');assert.equal(context.starts.length,1);await adapter.resumeFromGesture();await adapter.settled();assert.equal(context.starts.length,2);await adapter.dispose();
});
test('interruption drops one shots and recreates only loops once from saved phase',async()=>{
  const {adapter,context}=make();adapter.applyAll([start(1,1,false),start(2,2,true)]);await adapter.unlockFromGesture();await adapter.settled();context.currentTime=0.5;context.state='interrupted';context.signal();assert.equal(adapter.state,'interrupted');assert.equal(adapter.snapshot().activeVoices,1);await adapter.resumeFromGesture();assert.equal(context.starts.length,3);assert.equal(context.starts[2].loop,true);assert.equal(context.starts[2].offset,0.5);await adapter.dispose();
});
test('explicit pause resume replaces one shot node at its logical offset and releases on end',async()=>{
  const {adapter,context}=make();adapter.apply(start(1));await adapter.unlockFromGesture();await adapter.settled();context.currentTime=0.25;adapter.apply({Pause:{voice:{generation:1,serial:1}}});assert.equal(context.starts[0].stopped,true);adapter.apply({Resume:{voice:{generation:1,serial:1}}});assert.equal(context.starts.length,2);assert.equal(context.starts[1].offset,0.25);context.starts[1].onended();assert.equal(adapter.snapshot().activeVoices,0);assert.equal(context.starts[1].disconnected,true);await adapter.dispose();
});
test('numeric and PCM bounds reject unsafe identities and wrong sample dimensions',async()=>{
  const {adapter}=make({loadSample:async()=>({pcm:{sampleRate:4,channels:3,samples:new Int16Array([1,1,1])}})});assert.throws(()=>adapter.apply({...start(1),Start:{...start(1).Start,voice:{generation:1,serial:2**60}}}),/identity/);adapter.apply(start(1));await adapter.unlockFromGesture();await adapter.settled();assert.equal(adapter.snapshot().activeVoices,0);assert.match(adapter.snapshot().lastError,/PCM/);await adapter.dispose();
});
test('locked start stop churn cannot accumulate retired queue entries',async()=>{
  const {adapter}=make({maxVoices:1});for(let serial=1;serial<=100;serial++){adapter.apply(start(serial));adapter.apply({Stop:{voice:{generation:1,serial}}});}assert.equal(adapter.snapshot().queuedEntries,0);await adapter.dispose();
});
test('concurrent PCM conversions charge provisional memory before creating buffers',async()=>{
  const {adapter,context}=make({maxPcmBytes:16});adapter.applyAll([start(1,1),start(2,2)]);await adapter.unlockFromGesture();await adapter.settled();assert.equal(context.starts.length,1);assert.equal(adapter.snapshot().activeVoices,1);assert.equal(adapter.snapshot().pcmBytes,16);assert.equal(adapter.snapshot().reservedPcmBytes,0);assert.equal(context.buffersCreated,1);await adapter.dispose();
});

test('small decoded samples are bounded by both memory and cache entry count',async()=>{
  const {adapter}=make({maxVoices:1,maxPcmBytes:4096});await adapter.unlockFromGesture();for(let serial=1;serial<=20;serial++){adapter.apply(start(serial,serial));await adapter.settled();adapter.apply({Release:{voice:{generation:1,serial}}});}assert.ok(adapter.snapshot().cachedSamples<=2);await adapter.dispose();
});
test('a closed context is replaced only on gesture and resumes retained loops once',async()=>{
  const first=new Context(),second=new Context();let created=0;
  const {adapter}=make({contextFactory:()=>++created===1?first:second});
  adapter.applyAll([start(1,1,false),start(2,2,true)]);await adapter.unlockFromGesture();await adapter.settled();first.currentTime=0.25;await first.close();
  assert.equal(created,1);assert.equal(adapter.state,'interrupted');assert.equal(adapter.snapshot().activeVoices,1);await adapter.resumeFromGesture();await adapter.settled();assert.equal(created,2);assert.equal(second.starts.length,1);assert.equal(second.starts[0].loop,true);assert.equal(second.starts[0].offset,0.25);assert.deepEqual(adapter.takeFinished(),[{generation:'1',serial:'1'}]);await adapter.dispose();
});
test('finished voice delivery reserves capacity until the presentation host drains it',async()=>{
  const {adapter,context}=make({maxVoices:1});adapter.apply(start(1));await adapter.unlockFromGesture();await adapter.settled();context.starts[0].onended();
  assert.equal(adapter.snapshot().activeVoices,0);assert.equal(adapter.snapshot().completedVoices,1);assert.throws(()=>adapter.apply(start(2)),/voice budget/);
  assert.deepEqual(adapter.takeFinished(),[{generation:'1',serial:'1'}]);assert.deepEqual(adapter.takeFinished(),[]);adapter.apply(start(2));await adapter.settled();assert.equal(context.starts.length,2);await adapter.dispose();
});
test('decoder failures deliver a presentation completion and never resurrect a voice',async()=>{
  const {adapter,context}=make({loadSample:async()=>({encoded:new Uint8Array(3).buffer,format:'mp3',decodedBytes:16})});
  adapter.apply(start(1));await adapter.unlockFromGesture();await adapter.settled();assert.equal(context.starts.length,0);assert.deepEqual(adapter.takeFinished(),[{generation:'1',serial:'1'}]);assert.match(adapter.snapshot().lastError,/bad codec/);await adapter.dispose();
});
test('pending gesture resume cannot change a disposed adapter back to running',async()=>{
  const {adapter,context}=make();const gate=deferred();context.resume=()=>gate.promise;const unlocking=adapter.unlockFromGesture();await adapter.dispose();gate.resolve();await assert.rejects(unlocking,/cancelled/);assert.equal(adapter.state,'disposed');assert.equal(context.closed,true);
});

test('voice resume during context suspension restarts after gesture recovery',async()=>{
  const {adapter,context}=make();adapter.apply(start(1));await adapter.unlockFromGesture();await adapter.settled();
  context.currentTime=0.25;adapter.apply({Pause:{voice:{generation:1,serial:1}}});await adapter.suspend();adapter.apply({Resume:{voice:{generation:1,serial:1}}});
  assert.equal(context.starts.length,1);await adapter.resumeFromGesture();await adapter.settled();assert.equal(context.starts.length,2);assert.equal(context.starts[1].offset,0.25);await adapter.dispose();
});
test('replacement voice cannot inherit an aborted inflight request for the same key',async()=>{
  let calls=0;const {adapter,context}=make({maxPendingDecodes:1,loadSample:async(key,{signal})=>{
    calls++;if(calls===1)return new Promise((resolve,reject)=>signal.addEventListener('abort',()=>reject(Error('old request aborted')),{once:true}));
    return {pcm:{sampleRate:4,channels:1,samples:new Int16Array([1,2,3,4])}};
  }});
  adapter.apply(start(1));await adapter.unlockFromGesture();await new Promise(setImmediate);
  adapter.apply({Stop:{voice:{generation:1,serial:1}}});adapter.apply(start(2));assert.equal(adapter.snapshot().pendingDecodes,1);
  await adapter.settled();assert.equal(calls,2);assert.equal(context.starts.length,1);assert.deepEqual(adapter.takeFinished(),[]);assert.equal(adapter.snapshot().lastError,null);await adapter.dispose();
});
test('admission snapshots the asset key before the caller reuses its byte array',async()=>{
  const requested=[];const {adapter,context}=make({loadSample:async key=>{requested.push(key[0]);return {pcm:{sampleRate:4,channels:1,samples:new Int16Array([key[0]])}};}});
  const intent=start(1);adapter.apply(intent);intent.Start.sample.fill(2);await adapter.unlockFromGesture();await adapter.settled();
  assert.deepEqual(requested,[1]);assert.equal(context.starts[0].buffer.getChannelData(0)[0],1/32768);await adapter.dispose();
});

test('a stale suspension completion cannot overwrite a newer successful gesture',async()=>{
  const {adapter,context}=make(),gate=deferred();await adapter.unlockFromGesture();context.suspend=()=>gate.promise;
  const suspension=adapter.suspend();await adapter.unlockFromGesture();gate.resolve();await suspension;
  assert.equal(context.state,'running');assert.equal(adapter.state,'running');await adapter.dispose();
});
test('a rejected older unlock cannot overwrite a newer successful gesture',async()=>{
  const {adapter,context}=make(),gate=deferred();let calls=0;const resume=context.resume.bind(context);context.resume=()=>++calls===1?gate.promise:resume();
  const old=adapter.unlockFromGesture();await adapter.unlockFromGesture();gate.reject(Error('older attempt rejected'));await assert.rejects(old);
  assert.equal(adapter.state,'running');assert.equal(adapter.snapshot().lastError,null);await adapter.dispose();
});
test('interrupted context remains gesture-gated through suspended and running events',async()=>{
  const {adapter,context}=make();adapter.apply(start(1,1,true));await adapter.unlockFromGesture();await adapter.settled();
  context.state='interrupted';context.signal();context.state='suspended';context.signal();context.state='running';context.signal();
  assert.equal(adapter.state,'interrupted');assert.equal(context.starts.length,1);await adapter.unlockFromGesture();assert.equal(context.starts.length,2);await adapter.dispose();
});
test('a cancelled unlock cannot resume queued sound after Pause sound',async()=>{
  const {adapter,context}=make(),gate=deferred();context.resume=async()=>{await gate.promise;context.state='running';context.signal();};adapter.apply(start(1));
  const activation=adapter.unlockFromGesture();await adapter.suspend();gate.resolve();await assert.rejects(activation,/cancelled|superseded/);await adapter.settled();
  assert.equal(adapter.state,'suspended');assert.equal(context.starts.length,0);await adapter.dispose();
});
test('encoded WAVE reserves resampled PCM and keeps seek positions in source frames',async()=>{
  const {adapter,context}=make({maxPcmBytes:192004,loadSample:async()=>({encoded:new ArrayBuffer(44),format:'wav',decodedBytes:88200,sampleRate:22050,channels:1,frames:22050})});
  context.sampleRate=48000;context.decodeAudioData=async()=>context.createBuffer(1,48000,48000);
  const intent=start(1);intent.Start.seek_frame='11025';adapter.apply(intent);await adapter.unlockFromGesture();await adapter.settled();
  assert.equal(context.starts.length,1);assert.equal(context.starts[0].offset,0.5);assert.equal(adapter.snapshot().pcmBytes,192000);await adapter.dispose();
});
test('resampled PCM must fit the memory budget before browser decoding starts',async()=>{
  let decodes=0;const {adapter,context}=make({maxPcmBytes:100000,loadSample:async()=>({encoded:new ArrayBuffer(44),format:'wav',decodedBytes:88200,sampleRate:22050,channels:1,frames:22050})});
  context.sampleRate=48000;context.decodeAudioData=async()=>{decodes++;return context.createBuffer(1,48000,48000);};
  adapter.apply(start(1));await adapter.unlockFromGesture();await adapter.settled();assert.equal(decodes,0);assert.match(adapter.snapshot().lastError,/budget/);await adapter.dispose();
});
