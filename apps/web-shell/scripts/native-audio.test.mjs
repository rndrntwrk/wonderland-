import test from 'node:test';
import assert from 'node:assert/strict';
import {createNativeAudioSession} from '../public/native-audio.mjs';
function player(){return {resources:new Map(),clears:0,batches:[],resetVoices(){this.clears++;},registerResource(k,r){this.resources.set(k,r);},snapshot:()=>({state:'running'}),applyAll(v){this.batches.push(v);},takeFinished:()=>[{generation:'123',serial:'9007199254740993'}]};}
const key='01'.repeat(32);
test('owns copied PCM, exact identities, and clears only game resources on dispose',()=>{const p=player(),s=createNativeAudioSession(p),pcm=new Int16Array([1,-1]);s.register(key,8000,1,pcm);pcm[0]=9;assert.equal(p.resources.get(key).pcm.samples[0],1);assert.equal(s.playable(),true);assert.equal(JSON.parse(s.finished())[0].serial,'9007199254740993');s.dispose();assert.equal(p.resources.size,0);assert.equal(s.playable(),false);});
test('replacing a native session fences old deliveries and late disposal',()=>{const p=player(),a=createNativeAudioSession(p),b=createNativeAudioSession(p);const cleared=p.clears;assert.ok(BigInt(b.generation)>BigInt(a.generation));a.dispose();assert.equal(p.clears,cleared);assert.throws(()=>a.deliver('[]'));assert.throws(()=>a.register(key,8000,1,new Int16Array([1])));b.deliver('[]');assert.equal(p.batches.length,1);b.dispose();});
test('invalid shapes and cumulative PCM limits reject before copying',()=>{const p=player(),s=createNativeAudioSession(p);assert.throws(()=>s.register(key,8000,3,new Int16Array([1])));assert.throws(()=>s.register('../x',8000,1,new Int16Array([1])));assert.throws(()=>s.register(key,8000,1,new Int16Array(4*1024*1024+1)));assert.equal(p.resources.size,0);s.dispose();});
test('stop cancels game voices but retains samples and permits future cues',()=>{const p=player(),s=createNativeAudioSession(p);s.register(key,8000,1,new Int16Array([1]));let c=p.clears;s.stop();assert.equal(p.clears,c+1);assert.equal(p.resources.size,1);assert.equal(s.playable(),true);s.dispose();});
test('restores prior resources and preserves a later user replacement',()=>{
 const p=player(),old={encoded:new ArrayBuffer(1)};p.resources.set(key,old);
 const a=createNativeAudioSession(p);a.register(key,8000,1,new Int16Array([1]));a.dispose();assert.equal(p.resources.get(key),old);
 const b=createNativeAudioSession(p);b.register(key,8000,1,new Int16Array([2]));const later={encoded:new ArrayBuffer(3)};p.resources.set(key,later);b.dispose();assert.equal(p.resources.get(key),later);
});
test('aggregate residency is checked even for individually admissible samples',()=>{
 const p=player(),s=createNativeAudioSession(p),pcm=new Int16Array(4*1024*1024);
 s.register('02'.repeat(32),8000,1,pcm);s.register('03'.repeat(32),8000,1,pcm);
 assert.throws(()=>s.register('04'.repeat(32),8000,1,new Int16Array([1])));assert.equal(p.resources.size,2);s.dispose();
});
test('new playback identity exceeds the previous host high-water mark without Number rounding',()=>{
 const p=player();p.lastGeneration=9007199254740993n;const s=createNativeAudioSession(p);assert.ok(BigInt(s.generation)>p.lastGeneration);s.dispose();
});
