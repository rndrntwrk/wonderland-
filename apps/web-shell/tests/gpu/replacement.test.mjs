import test from 'node:test';
import assert from 'node:assert/strict';
import {paintSourceWorld,pickSourceWorld,disposeSourceWorld,worldGpuStats} from '../../public/world-gpu.mjs';

const matrix=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
const frame=(generation='1',id=7,width=64)=>({schema:2,width,height:48,generation,
 meshes:[{vertices:[0,0,0,0,0,1,0,0,1, 1,0,0,1,0,1,0,0,1, 0,1,0,0,1,1,0,0,1],indices:[0,1,2]}],
 textures:[],draws:[{mesh:0,texture:null,matrix,pick_id:id,depth_equal:false,pipeline:null}]});
// Allocation/draw faults exercise the shipping owner. This is a unit-level
// device adapter, not graphics evidence; actual pixels are checked in Chromium.
function device(){
 const allocated=new Set(),bound=new Map(),attributes=new Map(),events=new Map();
 let serial=0,id=0,target=null,visible=null,allocationFault=false,drawFault=false,lost=false;
 const gl={NO_ERROR:0,FRAMEBUFFER_COMPLETE:1,ALREADY_SIGNALED:2,MAX_TEXTURE_SIZE:3,STENCIL_BITS:4,
  ARRAY_BUFFER:5,ELEMENT_ARRAY_BUFFER:6,PIXEL_PACK_BUFFER:7,FRAMEBUFFER:8,
  getError:()=>0,isContextLost:()=>lost,getParameter:name=>name===4?8:4096,
  getShaderParameter:()=>true,getProgramParameter:()=>true,getUniformLocation:(_,name)=>name,
  checkFramebufferStatus:()=>1,clientWaitSync:()=>2,
  bindBuffer:(kind,value)=>bound.set(kind,value),bindFramebuffer:(_,value)=>{target=value;},
  uniform3f:(_,r,g,b)=>{id=Math.round(r*255)|(Math.round(g*255)<<8)|(Math.round(b*255)<<16);},
  drawElements:()=>{if(drawFault){drawFault=false;throw Error('injected draw refusal');}if(target===null)visible=id;},
  clear:()=>{if(target===null)visible=null;},
  readPixels:()=>{bound.get(7).pixel=id;},
  getBufferSubData:(_,__,bytes)=>{const value=bound.get(7).pixel;bytes.set([value&255,(value>>>8)&255,(value>>>16)&255,255]);},
 };
 for(const type of ['VertexArray','Buffer','Texture','Framebuffer','Renderbuffer','Shader','Program','Sync']){
  gl['create'+type]=()=>{if(type==='Buffer'&&allocationFault){allocationFault=false;return null;}const value={type,serial:++serial};allocated.add(value);return value;};
  gl['delete'+type]=value=>{if(value)assert.ok(allocated.delete(value),'resource deleted twice: '+type);};
 }
 gl.fenceSync=gl.createSync;
 const context=new Proxy(gl,{get:(object,key)=>key in object?object[key]:(()=>{})});
 let width=1,height=1;
 const canvas={getContext:()=>context,setAttribute:(k,v)=>attributes.set(k,v),getAttribute:k=>attributes.get(k),
  addEventListener:(name,fn)=>events.set(name,fn),removeEventListener:name=>events.delete(name),
  get width(){return width;},set width(v){width=v;visible=null;},get height(){return height;},set height(v){height=v;visible=null;}};
 globalThis.document={hidden:false,addEventListener(){},removeEventListener(){}};
 globalThis.window={dispatchEvent(){}};
 return {canvas,allocated,get visible(){return visible;},failAllocate(){allocationFault=true;},failDraw(){drawFault=true;},
  lose(){lost=true;events.get('webglcontextlost')({preventDefault(){}});}};
}
test('allocation refusal retains actual owner, generation and an already pending ID transfer',async()=>{
 const d=device();paintSourceWorld(d.canvas,JSON.stringify(frame()));
 const before=worldGpuStats(d.canvas),resources=d.allocated.size;
 const pending=pickSourceWorld(d.canvas,12,13);
 d.failAllocate();assert.throws(()=>paintSourceWorld(d.canvas,JSON.stringify(frame('2',9))),/vertex buffer/);
 assert.equal(d.visible,7);assert.equal(worldGpuStats(d.canvas).generation,before.generation);
 assert.equal(JSON.parse(await pending).index,7);
 assert.equal(d.allocated.size,resources);disposeSourceWorld(d.canvas);assert.equal(d.allocated.size,0);
});
test('failed first candidate draw restores old pixels, dimensions, tickets and resources',async()=>{
 const d=device();paintSourceWorld(d.canvas,JSON.stringify(frame()));
 const resources=d.allocated.size,pending=pickSourceWorld(d.canvas,12,13);
 d.failDraw();assert.throws(()=>paintSourceWorld(d.canvas,JSON.stringify(frame('2',9,96))),/draw refusal/);
 assert.equal(d.visible,7,'failed draw must repaint the retained scene, not leave a blank canvas');
 assert.equal(d.canvas.width,64);assert.equal(d.canvas.height,48);
 assert.equal(d.canvas.getAttribute('data-frame-generation'),'1');
 assert.equal(worldGpuStats(d.canvas).generation,'1');
 assert.equal(JSON.parse(await pending).generation,'1');
 assert.equal(d.allocated.size,resources);
 disposeSourceWorld(d.canvas);assert.equal(d.allocated.size,0);
});
test('only a successful draw retires the old frame and interrupts old picks',async()=>{
 const d=device();paintSourceWorld(d.canvas,JSON.stringify(frame()));
 const pending=assert.rejects(pickSourceWorld(d.canvas,12,13),/replaced/i);
 paintSourceWorld(d.canvas,JSON.stringify(frame('2',9,96)));await pending;
 assert.equal(d.visible,9);assert.equal(d.canvas.width,96);
 assert.equal(JSON.parse(await pickSourceWorld(d.canvas,12,13)).generation,'2');
 disposeSourceWorld(d.canvas);assert.equal(d.allocated.size,0);
});
test('repeated refused candidates do not accumulate device resources',()=>{
 const d=device();paintSourceWorld(d.canvas,JSON.stringify(frame()));const count=d.allocated.size;
 for(let i=0;i<20;i++){d.failDraw();assert.throws(()=>paintSourceWorld(d.canvas,JSON.stringify(frame(String(i+2),9))),/draw refusal/);assert.equal(d.allocated.size,count);assert.equal(d.visible,7);}
 disposeSourceWorld(d.canvas);assert.equal(d.allocated.size,0);
});
test('genuine context loss still invalidates the displayed world',async()=>{
 const d=device();paintSourceWorld(d.canvas,JSON.stringify(frame()));d.lose();
 assert.equal(worldGpuStats(d.canvas).ready,false);
 await assert.rejects(pickSourceWorld(d.canvas,12,13),/unavailable/);
 disposeSourceWorld(d.canvas);
});

test('browser adapter uses the CPU/device transaction and preserves recoverable owners',async()=>{
 const {readFile}=await import('node:fs/promises');
 const source=await readFile(new URL('../../src/world_renderer.rs',import.meta.url),'utf8');
 assert.match(source,/renderer\s*\.update_gpu\(/);
 assert.match(source,/if !frame_available\(&canvas\)/);
 assert.match(source,/WorldReplacementControls/);
});
test('source import does not admit a document before the renderer reports success',async()=>{
 const {readFile}=await import('node:fs/promises');
 const source=await readFile(new URL('../../src/source_world_screen.rs',import.meta.url),'utf8');
 const upload=source.slice(source.indexOf('let upload ='),source.indexOf('    view! {',source.indexOf('let upload =')));
 assert.doesNotMatch(upload,/world\.try_set\(document\)/);
 assert.match(upload,/pending\.try_set\(Some\(\(document, filename\)\)\)/);
 assert.match(source,/complete: Callback::new/);
});
