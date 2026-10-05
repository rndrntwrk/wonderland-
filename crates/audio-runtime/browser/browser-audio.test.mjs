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
