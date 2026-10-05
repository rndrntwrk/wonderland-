import test from 'node:test';
import assert from 'node:assert/strict';
import { SourceAudioPlayer, waveResource } from './source-audio.mjs';
import { BrowserAudio } from './browser-audio.mjs';
// Only browser platform interfaces are controlled; actual adapter/player policy
// and the connected source/gain graph execute in every lifecycle test.
class Context {
 state='suspended';currentTime=0;destination={};starts=[];listeners=[];
 async resume(){this.state='running';for(const fn of this.listeners)fn();}
 async suspend(){this.state='suspended';for(const fn of this.listeners)fn();}
 async close(){this.state='closed';}
 addEventListener(_name,fn){this.listeners.push(fn);}removeEventListener(_name,fn){this.listeners=this.listeners.filter(x=>x!==fn);}
 createBuffer(channels,length,sampleRate){const data=Array.from({length:channels},()=>new Float32Array(length));return {numberOfChannels:channels,length,sampleRate,duration:length/sampleRate,getChannelData:c=>data[c]};}
 node(){return {connect(other){this.next=other;},disconnect(){this.disconnected=true;}};}
 createGain(){return {...this.node(),gain:{value:1,setValueAtTime(value){this.value=value;}}};}
 createStereoPanner(){return {...this.node(),pan:{value:0,setValueAtTime(value){this.value=value;}}};}
 createBufferSource(){const context=this;return {...this.node(),start(){context.starts.push(this);},stop(){this.stopped=true;this.onended?.();}};}
}
function makeBackend(){const context=new Context();const backend=new BrowserAudio({contextFactory:()=>context,loadSample:async()=>({pcm:{sampleRate:4,channels:1,samples:new Int16Array([0,100,-100,0])}})});return {backend,context};}
const voice={generation:'9007199254740993',serial:'1'};
const key='a'.repeat(64);
const cue={Start:{voice,sample:key,group:'Music',gain:0.8,pan:0,looped:false,seek_frame:'0'}};
test('group volume and mute change actual voice gain while preserving original gain',async()=>{
 const {backend,context}=makeBackend();const player=new SourceAudioPlayer({backend});
 player.applyAll([cue]);await player.unlockFromGesture();await backend.settled();player.setVolume('Music',0.5);
 assert.equal(context.starts[0].next.gain.value,0.4);
 player.setMuted(true);assert.equal(context.starts[0].next.gain.value,0);
 player.setMuted(false);assert.equal(context.starts[0].next.gain.value,0.4);await player.dispose();
});
test('unlock lifecycle is explicit and cleanup cannot unlock a disposed player',async()=>{
 const {backend}=makeBackend();const player=new SourceAudioPlayer({backend});
 assert.equal(player.snapshot().state,'locked');await player.unlockFromGesture();assert.equal(player.snapshot().state,'running');
 await player.suspend();assert.equal(player.snapshot().state,'suspended');await player.dispose();assert.equal(player.snapshot().state,'disposed');await assert.rejects(player.unlockFromGesture(),/disposed/);
});
function wave(frames){let b=new ArrayBuffer(44+frames*4),v=new DataView(b);const text=(at,s)=>[...s].forEach((c,i)=>v.setUint8(at+i,c.charCodeAt(0)));text(0,'RIFF');v.setUint32(4,b.byteLength-8,true);text(8,'WAVE');text(12,'fmt ');v.setUint32(16,16,true);v.setUint16(20,1,true);v.setUint16(22,2,true);v.setUint32(24,44100,true);v.setUint32(28,176400,true);v.setUint16(32,4,true);v.setUint16(34,16,true);text(36,'data');v.setUint32(40,frames*4,true);return b;}
test('original RIFF fields allocate float32 budget rather than PCM16 bytes',()=>{
 const r=waveResource(wave(100));assert.equal(r.decodedBytes,800);assert.equal(r.sampleRate,44100);assert.equal(r.frames,100);assert.equal(r.channels,2);
});
test('long samples fail whole-buffer budget before browser decode and can stream',()=>{
 const r=waveResource(wave(100),{maxPcmBytes:64});assert.equal(r.streaming,true);assert.equal(r.decodedBytes,800);
 assert.throws(()=>waveResource(new ArrayBuffer(44)),/RIFF/);
});
class Media {volume=1;paused=true;listeners={};addEventListener(name,fn){this.listeners[name]=fn;}async play(){this.paused=false;}pause(){this.paused=true;}removeAttribute(){this.src='';}load(){} }
test('long original music streams without whole-buffer decode and stops/reclaims object URL',async()=>{
 const {backend,context}=makeBackend(),media=new Media(),released=[];
 const player=new SourceAudioPlayer({backend,mediaFactory:()=>media,urlApi:{createObjectURL:()=> 'blob:source',revokeObjectURL:url=>released.push(url)}});
 player.registerResource(key,{streaming:true,blob:{},sampleRate:44100});player.applyAll([cue]);assert.equal(media.paused,true);assert.equal(context.starts.length,0);
 await player.unlockFromGesture();assert.equal(media.paused,false);player.setVolume('Music',0.25);assert.equal(media.volume,0.2);player.setMuted(true);assert.equal(media.volume,0);
 player.applyAll([{Stop:{voice}}]);assert.equal(media.paused,true);assert.deepEqual(released,['blob:source']);assert.equal(player.snapshot().streamingVoices,0);await player.dispose();
});
test('stream identities cannot replay after stop and unsafe numbers do not round',()=>{
 const player=new SourceAudioPlayer({backend:makeBackend().backend,mediaFactory:()=>new Media(),urlApi:{createObjectURL:()=> 'blob:source',revokeObjectURL(){}}});
 player.registerResource(key,{streaming:true,blob:{}});player.applyAll([cue]);player.applyAll([{Stop:{voice}}]);assert.throws(()=>player.applyAll([cue]),/replay/);
 assert.throws(()=>player.applyAll([{Start:{...cue.Start,voice:{generation:2**60,serial:2}}}]),/identity/);
});
