import test from 'node:test';
import assert from 'node:assert/strict';
import { SourceAudioPlayer, waveResource } from './source-audio.mjs';
import * as sourceAudioModule from './source-audio.mjs';
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

function deferred(){let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return {promise,resolve,reject};}
function streamPlayer({backend=makeBackend().backend,media=new Media()}={}){
 const released=[];const player=new SourceAudioPlayer({backend,mediaFactory:()=>media,urlApi:{createObjectURL:()=> 'blob:source',revokeObjectURL:url=>released.push(url)}});
 player.registerResource(key,{streaming:true,blob:{},sampleRate:44100});return {player,media,backend,released};
}
test('invalid stream admission leaves identity and capacity available for a corrected Start',async()=>{
 const {player}=streamPlayer();assert.throws(()=>player.applyAll([{Start:{...cue.Start,group:'Fx'}}]),/Music/);
 assert.equal(player.voices.size,0);player.applyAll([cue]);assert.equal(player.streams.size,1);await player.dispose();
});
test('a backend admission failure does not consume the source voice identity',async()=>{
 const backend=new BrowserAudio({maxVoices:1,loadSample:async()=>null});const player=new SourceAudioPlayer({backend});
 player.applyAll([cue]);const next={Start:{...cue.Start,voice:{...voice,serial:'2'}}};
 assert.throws(()=>player.applyAll([next]),/voice budget/);assert.equal(player.voices.size,1);
 player.applyAll([{Stop:{voice}}]);player.applyAll([next]);assert.equal(player.voices.size,1);await player.dispose();
});
test('a new source generation replaces a full previous generation',async()=>{
 const {player}=streamPlayer();for(let serial=1;serial<=128;serial++)player.applyAll([{Start:{...cue.Start,voice:{generation:'1',serial:String(serial)}}}]);
 player.applyAll([{Start:{...cue.Start,voice:{generation:'2',serial:'1'}}}]);assert.equal(player.voices.size,1);assert.equal(player.streams.size,1);await player.dispose();
});
test('mixed decoded and streaming generation batches cannot resurrect the prior generation',async()=>{
 const {backend,context}=makeBackend(),{player}=streamPlayer({backend});
 player.applyAll([{Start:{...cue.Start,sample:'b'.repeat(64),voice:{generation:'1',serial:'1'}}},{Start:{...cue.Start,voice:{generation:'2',serial:'1'}}}]);
 await player.unlockFromGesture();await backend.settled();assert.equal(context.starts.length,0);assert.equal(player.streams.size,1);await player.dispose();
});
test('Stop and Release retire a streamed completion before the host drains it',async()=>{
 for(const kind of ['Stop','Release']){const {player,media}=streamPlayer();player.applyAll([cue]);media.listeners.ended();player.applyAll([{[kind]:{voice}}]);assert.deepEqual(player.takeFinished(),[]);await player.dispose();}
});
test('source admission owns the sample key and validates stream seek before allocating',async()=>{
 const {player,released}=streamPlayer();assert.throws(()=>player.applyAll([{Start:{...cue.Start,seek_frame:'-1'}}]),/identity|seek/);assert.equal(player.voices.size,0);assert.deepEqual(released,[]);
 const bytes=Array(32).fill(170);player.applyAll([{Start:{...cue.Start,sample:bytes}}]);bytes.fill(187);
 const admitted=player.voices.get(`${voice.generation}:${voice.serial}`);assert.equal(admitted.sample,key);await player.dispose();
});
test('stream play is requested in the gesture before an asynchronous context resume settles',async()=>{
 const {backend,context}=makeBackend(),gate=deferred();let plays=0;const media=new Media();media.play=async()=>{plays++;media.paused=false;};
 context.resume=async()=>{await gate.promise;context.state='running';for(const fn of context.listeners)fn();};
 const {player}=streamPlayer({backend,media});player.applyAll([cue]);const activation=player.unlockFromGesture();assert.equal(plays,1);
 gate.resolve();await activation;assert.equal(plays,1);await player.dispose();
});
test('a rejected media play is an awaited activation failure that can be retried',async()=>{
 const media=new Media();let deny=true;media.play=async()=>{if(deny)throw Error('autoplay denied');media.paused=false;};const {player}=streamPlayer({media});player.applyAll([cue]);
 await assert.rejects(player.unlockFromGesture(),/autoplay denied/);assert.equal(player.snapshot().state,'interrupted');deny=false;await player.unlockFromGesture();assert.equal(player.snapshot().state,'running');assert.equal(player.snapshot().lastNotice,null);await player.dispose();
});
test('stale media play rejection after Pause does not interrupt a healthy player',async()=>{
 const gate=deferred(),media=new Media();media.play=()=>gate.promise;const {player}=streamPlayer({media});await player.unlockFromGesture();player.applyAll([cue]);
 player.applyAll([{Pause:{voice}}]);gate.reject(Error('play cancelled by pause'));await new Promise(setImmediate);assert.equal(player.snapshot().state,'running');assert.equal(player.snapshot().lastNotice,null);await player.dispose();
});
test('Pause sound cancels a pending gesture activation and cannot be undone by its callback',async()=>{
 const {backend,context}=makeBackend(),gate=deferred();context.resume=async()=>{await gate.promise;context.state='running';for(const fn of context.listeners)fn();};
 const {player}=streamPlayer({backend});player.applyAll([cue]);const activation=player.unlockFromGesture();await player.suspend();gate.resolve();await assert.rejects(activation,/cancelled|superseded/);
 assert.equal(player.snapshot().state,'suspended');await player.dispose();
});
test('dispose wins over a pending source suspension',async()=>{
 const {backend,context}=makeBackend(),gate=deferred();context.suspend=()=>gate.promise;const {player}=streamPlayer({backend});await player.unlockFromGesture();const suspension=player.suspend();await player.dispose();gate.resolve();await suspension;assert.equal(player.state,'disposed');
});
test('file selection is atomic and duplicate content uses one resource',async()=>{
 const {player}=streamPlayer();const good={name:'original.wav',size:wave(4).byteLength,arrayBuffer:async()=>wave(4)};
 await assert.rejects(player.loadFiles([good,{name:'broken.wav',size:44,arrayBuffer:async()=>new ArrayBuffer(44)}]),/RIFF/);assert.equal(player.resources.size,1);
 const loaded=await player.loadFiles([good,good]);assert.equal(loaded.length,1);assert.equal(player.resources.size,2);await player.dispose();
});
test('sound preferences restore all four group gains and mute without starting a context',async()=>{
 const storage=new Map();const api={getItem:k=>storage.get(k)??null,setItem:(k,v)=>storage.set(k,v)};
 const first=new sourceAudioModule.SourceAudioPreferences({storage:()=>api});first.setVolume('Music',0.25);first.setVolume('Fx',0.5);first.setVolume('Vox',0.75);first.setVolume('Ambience',0.1);first.setMuted(true);
 const restored=new sourceAudioModule.SourceAudioPreferences({storage:()=>api});const {backend,context}=makeBackend(),player=new SourceAudioPlayer({backend});restored.applyTo(player);
 assert.deepEqual(player.volumes,{Music:0.25,Fx:0.5,Vox:0.75,Ambience:0.1});assert.equal(player.muted,true);assert.equal(player.snapshot().state,'locked');assert.equal(context.starts.length,0);await player.dispose();
});
test('malformed and unavailable preference storage stays usable and exposes persistence failures',()=>{
 const broken=new sourceAudioModule.SourceAudioPreferences({storage:()=>({getItem:()=>'{broken'})});assert.equal(broken.muted,false);assert.equal(broken.volumes.Music,1);assert.match(broken.lastError,/saved|settings/i);
 const denied=new sourceAudioModule.SourceAudioPreferences({storage:()=>{throw Error('storage disabled');}});denied.setVolume('Fx',0.2);assert.equal(denied.volumes.Fx,0.2);assert.match(denied.lastError,/save|remember/i);
});
test('the dialog audition entry point requests media play before gesture resume settles',async()=>{
 const {backend,context}=makeBackend(),gate=deferred();let plays=0;const media=new Media();media.play=async()=>{plays++;media.paused=false;};
 context.resume=async()=>{await gate.promise;context.state='running';for(const fn of context.listeners)fn();};const {player}=streamPlayer({backend,media});
 const playback=player.auditionFromGesture(key);assert.equal(plays,1);gate.resolve();await playback;assert.equal(player.snapshot().state,'running');await player.dispose();
});
test('a new audition retains the current media failure in its retryable state',async()=>{
 const media=new Media();media.play=async()=>{throw Error('selected music blocked');};const {player}=streamPlayer({media});
 await assert.rejects(player.auditionFromGesture(key),/selected music blocked/);assert.equal(player.snapshot().state,'interrupted');assert.match(player.snapshot().lastNotice,/selected music blocked/);await player.dispose();
});
test('a newer gesture owns an already pending media request and observes its failure',async()=>{
 const media=new Media(),gate=deferred();media.play=()=>gate.promise;const {player}=streamPlayer({media});await player.unlockFromGesture();player.applyAll([cue]);
 const activation=player.unlockFromGesture();gate.reject(Error('pending media blocked'));await assert.rejects(activation,/pending media blocked/);assert.equal(player.snapshot().state,'interrupted');await player.dispose();
});
test('audition streams a WAVE when device-rate resampling would exceed decode memory',async()=>{
 const context=new Context();context.sampleRate=48000;let decodes=0;context.decodeAudioData=async()=>{decodes++;throw Error('must stream before allocating');};
 const backend=new BrowserAudio({contextFactory:()=>context,maxPcmBytes:100000,loadSample:async()=>{throw Error('must stream');}}),media=new Media();
 const {player}=streamPlayer({backend,media});player.registerResource(key,{streaming:false,blob:{},encoded:new ArrayBuffer(44),format:'wav',sampleRate:22050,frames:22050,channels:1,decodedBytes:88200});
 await player.auditionFromGesture(key);assert.equal(decodes,0);assert.equal(media.paused,false);assert.equal(player.snapshot().streamingVoices,1);await player.dispose();
});
test('repeated file selections share a cumulative resource budget and disposal cancels pending reads',async()=>{
 const player=new SourceAudioPlayer({backend:makeBackend().backend,maxSourceFiles:1});
 const original={name:'first.wav',size:wave(4).byteLength,arrayBuffer:async()=>wave(4)};await player.loadFiles([original]);
 const other={name:'second.wav',size:wave(5).byteLength,arrayBuffer:async()=>wave(5)};await assert.rejects(player.loadFiles([other]),/memory budget/);assert.equal(player.resources.size,1);
 const gate=deferred();const pending=player.loadFiles([{...other,arrayBuffer:()=>gate.promise}]);await player.dispose();gate.resolve(wave(5));await assert.rejects(pending,/disposed/);assert.equal(player.resources.size,0);
});
