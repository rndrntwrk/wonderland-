import test from 'node:test';
import assert from 'node:assert/strict';
import {deflateSync} from 'node:zlib';
import {canvasPixels} from './canvas-evidence.mjs';

function image(redX=0, hud=0) {
 const rgba=Buffer.from([hud,80,90,255, hud,80,90,255, 50,60,70,255, 50,60,70,255]);
 rgba.set([255,0,0,255],redX*4);
 const chunk=(name,data)=>{const b=Buffer.alloc(data.length+12);b.writeUInt32BE(data.length);b.write(name,4);data.copy(b,8);return b;};
 const header=Buffer.alloc(13);header.writeUInt32BE(2);header.writeUInt32BE(2,4);header[8]=8;header[9]=6;
 return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',deflateSync(Buffer.concat([Buffer.from([0]),rgba.subarray(0,8),Buffer.from([0]),rgba.subarray(8)]))),chunk('IEND',Buffer.alloc(0))]);
}
function observedCanvas(frame,composited) {
 return {
  async screenshot(){return composited;},
  async evaluate(){return {generation:'7',width:2,height:2,png:'data:image/png;base64,'+frame.toString('base64')};}
 };
}
test('full canvas identity excludes a changing DOM overlay',async()=>{
 const frame=image();
 const first=await canvasPixels(observedCanvas(frame,image(0,80)));
 const second=await canvasPixels(observedCanvas(frame,image(0,120)));
 assert.equal(first,second,'Identical accepted canvas pixels must not drift because the HUD changed');
});
test('changed actual canvas cannot pass with an unchanged compositor overlay',async()=>{
 const overlay=image(0,100);
 assert.notEqual(await canvasPixels(observedCanvas(image(0),overlay)),
  await canvasPixels(observedCanvas(image(1),overlay)),'Actual avatar changes must not be hidden by unchanged chrome');
});

import {installCanvasPaintObserver} from './canvas-observation.mjs';
import {redAvatarSignature} from './canvas-evidence.mjs';
import {png as decodePng} from '../../apps/web-shell/tests/gpu/png.mjs';

function environment(){
 let notify,observed;
 const scope={document:{},MutationObserver:class{
  constructor(callback){notify=callback;}
  observe(target,options){observed={target,options};}
 }};
 installCanvasPaintObserver(scope);
 const canvas={tagName:'CANVAS',width:2,height:2,isConnected:true,native:true,captures:0,
  attributes:{'data-gpu-state':'ready','data-frame-generation':'1'},
  closest(selector){assert.equal(selector,'.native-lot');return this.native?{}:null;},
  getAttribute(key){return this.attributes[key]??null;},
  toDataURL(mime){assert.equal(mime,'image/png');this.captures++;if(this.tainted)throw Error('Tainted');
   return this.url??('data:image/png;base64,'+image().toString('base64'));},
  getContext(){assert.fail('Observer cannot acquire a graphics context');}
 };
 const emit=(target=canvas,name='data-frame-generation')=>notify([{type:'attributes',target,attributeName:name}]);
 return {scope,canvas,emit,removed:()=>notify([{type:'childList'}]),observed};
}
test('observer captures only the actual post-draw marker and retains one latest image',()=>{
 const {scope,canvas,emit,observed}=environment();
 assert.equal(observed.target,scope.document);assert.equal(observed.options.subtree,true);
 assert.deepEqual(observed.options.attributeFilter,['data-frame-generation','data-gpu-state']);
 assert.equal(scope.__wonderlandCanvasEvidence.read(canvas),null);
 emit(canvas,'data-gpu-state');assert.equal(canvas.captures,0);
 emit();const first=scope.__wonderlandCanvasEvidence.read(canvas);
 assert.equal(first.observation,1);assert.equal(first.generation,'1');
 canvas.attributes['data-frame-generation']='2';
 assert.equal(scope.__wonderlandCanvasEvidence.read(canvas),null,'Old generation is not current');
 canvas.url='data:image/png;base64,'+image(1).toString('base64');
 emit();const second=scope.__wonderlandCanvasEvidence.read(canvas);
 assert.equal(second.observation,2);assert.equal(second.generation,'2');assert.notEqual(second.png,first.png);
 assert.equal(canvas.captures,2);
});
test('lost, disposed, resized, removed and non-native canvases cannot expose a retained frame',()=>{
 for(const invalidate of [
  ({canvas,emit})=>{canvas.attributes['data-gpu-state']='lost';emit(canvas,'data-gpu-state');},
  ({canvas,emit})=>{canvas.attributes['data-gpu-state']='disposed';emit(canvas,'data-gpu-state');},
  ({canvas})=>{canvas.width=3;},
  ({canvas,removed})=>{canvas.isConnected=false;removed();},
  ({canvas})=>{canvas.native=false;}
 ]){
  const env=environment();env.emit();assert.ok(env.scope.__wonderlandCanvasEvidence.read(env.canvas));
  invalidate(env);assert.equal(env.scope.__wonderlandCanvasEvidence.read(env.canvas),null);
 }
});
test('failed or excessive capture clears the prior image and restoration requires a new draw',()=>{
 for(const change of [
  canvas=>{canvas.tainted=true;},
  canvas=>{canvas.width=4097;},
  canvas=>{canvas.width=1025;canvas.height=1025;},
  canvas=>{canvas.attributes['data-frame-generation']='18446744073709551616';},
  canvas=>{canvas.attributes['data-frame-generation']='01';},
  canvas=>{canvas.url='data:,';},
  canvas=>{canvas.url='data:image/png;base64,'+'A'.repeat(8*1024*1024);}
 ]){
  const {scope,canvas,emit}=environment();emit();change(canvas);emit();
  assert.equal(scope.__wonderlandCanvasEvidence.read(canvas),null);
 }
 const {scope,canvas,emit}=environment();emit();
 canvas.attributes['data-gpu-state']='lost';emit(canvas,'data-gpu-state');
 canvas.attributes['data-gpu-state']='ready';emit(canvas,'data-gpu-state');
 assert.equal(scope.__wonderlandCanvasEvidence.read(canvas),null);
 emit();assert.ok(scope.__wonderlandCanvasEvidence.read(canvas));
});
test('a replacement native canvas evicts the prior owner without retaining a history',()=>{
 const {scope,canvas,emit}=environment();emit();
 const next={...canvas,attributes:{...canvas.attributes}};
 emit(next);assert.ok(scope.__wonderlandCanvasEvidence.read(next));
 assert.equal(scope.__wonderlandCanvasEvidence.read(canvas),null);
 assert.throws(()=>installCanvasPaintObserver(scope),/already installed/);
});
test('empty or mismatched framebuffer data fails the pose witness',async()=>{
 const empty=image();const decoded=decodePng(empty);
 for(let i=0;i<decoded.pixels.length;i+=4)decoded.pixels.set([0,0,0,255],i);
 assert.throws(()=>redAvatarSignature(decoded),/No synthetic avatar/);
 const bad=observedCanvas(image(),image());
 bad.evaluate=async()=>({generation:'1',width:3,height:2,png:'data:image/png;base64,'+image().toString('base64')});
 await assert.rejects(canvasPixels(bad),/2 !== 3/);
});
test('ordinary compositor motion excludes HUD changes but detects a moved visible avatar',()=>{
 const first=redAvatarSignature(decodePng(image(0,90)));
 assert.equal(first,redAvatarSignature(decodePng(image(0,120))));
 assert.notEqual(first,redAvatarSignature(decodePng(image(1,90))));
});

import {readFile} from 'node:fs/promises';
test('all four native image witnesses install the observation before navigation',async()=>{
 for(const file of ['verify-avatars.mjs','verify-audio.mjs','verify-pose-batches.mjs','verify-pose-retention.mjs']){
  const source=await readFile(new URL(file,import.meta.url),'utf8');
  assert.ok(source.indexOf('await observeNativeCanvas(page)')>=0,file);
  assert.ok(source.indexOf('await observeNativeCanvas(page)')<source.indexOf('await page.goto('),file);
 }
});
test('moving avatars require independent ordinary compositor motion as well as frame identity',async()=>{
 for(const file of ['verify-avatars.mjs','verify-pose-retention.mjs']){
  const source=await readFile(new URL(file,import.meta.url),'utf8');
  assert.match(source,/visibleAvatarPixels\(canvas\)/,file);
  assert.match(source,/visibleFrames\.size>=2/,file);
 }
});

test('unrelated non-native canvas notifications cannot evict a valid native observation',()=>{
 const {scope,canvas,emit}=environment();emit();
 const before=scope.__wonderlandCanvasEvidence.read(canvas);
 const unrelated={...canvas,native:false};
 emit(unrelated);
 assert.deepEqual(scope.__wonderlandCanvasEvidence.read(canvas),before);
 assert.equal(unrelated.captures,canvas.captures,'A non-native canvas must never be captured');
});
