import {parseConfig,ProbeController,alignCanvasLayout} from './host-core.mjs';
import {EngineCanvasReadback} from './gpu-readback.mjs';

const host=document.querySelector('#canvas-host');
const statusNode=document.querySelector('#status');
let controller;
try{controller=new ProbeController(parseConfig(location.search));}
catch(error){document.querySelector('#errors').hidden=false;document.querySelector('#errors').textContent=String(error);throw error;}
let canvas=host.querySelector('canvas');
let gpuDevice=null,glContext=null,wasm=null,pointer=null,audioAdapter=null,audioGeneration=1,audioSerial=0,currentVoice=null;
const config=controller.config;
const started=performance.now();
controller.status.crossOriginIsolated=globalThis.crossOriginIsolated===true;
controller.status.devicePixelRatio=devicePixelRatio;
controller.status.startupMs=null;
controller.status.submittedFrameTimes=[];
controller.status.audio={state:'not-created',available:false};
controller.status.bootstrap=[];
const observedGpuDevices=new WeakSet(),observedGpuContexts=new WeakSet(),gpuDeviceIds=new WeakMap();
let nextGpuDeviceId=1;
function diagnostic(stage,detail={}){
  const event={stage,elapsedMs:Math.round(performance.now()-started),...detail};
  controller.status.bootstrap.push(event);
  if(controller.status.bootstrap.length>48)controller.status.bootstrap.shift();
  console.info('WONDERLAND_BOOTSTRAP '+JSON.stringify(event));
}
const gpuReadback=new EngineCanvasReadback({diagnostic,stamp:()=>({sceneHash:controller.engine.sceneHash,mode:controller.engine.mode,pass:controller.engine.pass,lastCommand:controller.engine.lastCommand})});
diagnostic('host-start',{variant:config.variant,secureContext:isSecureContext,hasNavigatorGpu:!!navigator.gpu,crossOriginIsolated:controller.status.crossOriginIsolated,dpr:devicePixelRatio});

function fail(error){const reason=error?.stack||error?.message||String(error);controller.fail(reason);diagnostic('error',{reason:String(reason).slice(0,4000)});refresh();}
window.addEventListener('error',event=>fail(event.error||event.message));
window.addEventListener('unhandledrejection',event=>fail(event.reason));
function issue(kind,args){
  const seq=controller.enqueue(kind,args);
  if(kind==='suspend'||kind==='simulateLoss')audioAdapter?.suspend();
  if(kind==='reloadFixture'){audioAdapter?.reset();audioGeneration++;currentVoice=null;}
  refresh();return seq;
}
function audioSnapshot(){return audioAdapter?.snapshot()||controller.status.audio;}
function snapshot(){
  const rect=canvas.getBoundingClientRect();
  return {...controller.snapshot(),viewport:{width:canvas.width,height:canvas.height,cssWidth:rect.width,cssHeight:rect.height,cssX:rect.x,cssY:rect.y,documentX:rect.x+scrollX,documentY:rect.y+scrollY},
    memoryBytes:wasm?.memory?.buffer?.byteLength??null,elapsedMs:performance.now()-started,audio:audioSnapshot()};
}
function alignForCapture(){
  alignCanvasLayout(host,canvas,{x:scrollX,y:scrollY});
  return snapshot().viewport;
}
const api={
  snapshot,alignForCapture,
  readGpuFrame:()=>gpuReadback.read(),
  canvasSnapshot:()=>canvas.toDataURL('image/png'),
  setMode:mode=>issue('setMode',{mode}),setTick:tick=>issue('setTick',{tick}),
  selectAt:(x,y)=>issue('selectAt',{x,y}),setPass:pass=>issue('setPass',{pass}),
  suspend:()=>issue('suspend'),resume:()=>issue('resume'),simulateLoss:()=>issue('simulateLoss'),
  reloadFixture:()=>issue('reloadFixture'),audioSnapshot,
  startAudio:async()=>{
    if(!audioAdapter)throw new Error('Audio adapter is unavailable');
    currentVoice={generation:audioGeneration,serial:++audioSerial};
    audioAdapter.apply({Start:{voice:currentVoice,sample:Array(32).fill(67),group:'Fx',gain:0.08,pan:0,looped:true,seek_frame:0}});
    await audioAdapter.settled();
    refresh();return audioSnapshot();
  },
  stopAudio:()=>{if(audioAdapter&&currentVoice)audioAdapter.apply({Stop:{voice:currentVoice}});currentVoice=null;refresh();return audioSnapshot();},
  resize:(width,height)=>{
    if(![width,height].every(v=>Number.isInteger(v)&&v>=1&&v<=4096))throw new Error('Viewport must be 1..4096 CSS pixels');
    host.style.width=`${width}px`;host.style.height=`${height}px`;host.style.maxWidth='100%';window.dispatchEvent(new Event('resize'));
  },
  loseContext:()=>{
    const extension=glContext?.getExtension('WEBGL_lose_context');
    if(!extension)throw new Error('An actual WebGL2 context with WEBGL_lose_context is required');extension.loseContext();
  },
  restoreContext:()=>{
    const extension=glContext?.getExtension('WEBGL_lose_context');
    if(!extension)throw new Error('No restorable WebGL context');extension.restoreContext();
  },
  loseDevice:()=>{if(!gpuDevice)throw new Error('No actual WebGPU device');gpuDevice.destroy();},
  recover:()=>location.reload(),
};
Object.defineProperty(window,'__wonderlandProbe',{value:Object.freeze(api),writable:false});
let firstFixturePublished=false;
window.__wonderlandHost={config,drain:()=>controller.drain(),publish:value=>{
  controller.publish(value);
  if(!firstFixturePublished&&value.sceneHash){firstFixturePublished=true;diagnostic('fixture-published',{sceneHash:value.sceneHash,mode:value.mode,avatars:value.avatars});}
  refresh();
},fail};

function attach(next){
  if(!(next instanceof HTMLCanvasElement))return;
  if(next!==canvas){canvas=next;canvas.id='engine-canvas';host.replaceChildren(canvas);wireCanvas(canvas);}
  else if(canvas.parentNode!==host){host.replaceChildren(canvas);}
  canvas.tabIndex=0;canvas.setAttribute('aria-label','Game view. Click to select; keys 1, 2 and 3 change view.');
}
function observeGl(gl,next){
  glContext=gl;attach(next);
  const debug=gl.getExtension('WEBGL_debug_renderer_info');
  const adapter={api:gl.getParameter(gl.VERSION),vendor:debug?gl.getParameter(debug.UNMASKED_VENDOR_WEBGL):gl.getParameter(gl.VENDOR),
    renderer:debug?gl.getParameter(debug.UNMASKED_RENDERER_WEBGL):gl.getParameter(gl.RENDERER)};
  controller.observeBackend('webgl2',adapter);
  diagnostic('webgl2-context-observed',adapter);
  for(const name of ['drawArrays','drawElements','drawArraysInstanced','drawElementsInstanced']){
    const original=gl[name];if(!original)continue;
    gl[name]=function(...args){const result=original.apply(this,args);controller.submission('webgl2');return result;};
  }
}
function observeGpuDevice(device,details={}){
  if(!device)throw new Error('The engine supplied no WebGPU device');
  gpuDevice=device;
  const prior=controller.status.actualBackend==='webgpu'?(controller.status.adapter||{}):{};
  controller.observeBackend('webgpu',{...prior,...details});
  if(observedGpuDevices.has(device))return;
  observedGpuDevices.add(device);
  const deviceId=nextGpuDeviceId++;gpuDeviceIds.set(device,deviceId);
  diagnostic('webgpu-device-observed',{deviceId,...details,features:Array.from(device.features||[])});
  const originalDestroy=device.destroy.bind(device);
  device.destroy=()=>{
    diagnostic('webgpu-device-destroy-called',{deviceId,active:device===gpuDevice,stack:new Error('GPUDevice.destroy caller').stack?.slice(0,3500)});
    return originalDestroy();
  };
  const originalSubmit=device.queue.submit.bind(device.queue);
  device.queue.submit=commands=>{const result=originalSubmit(commands);controller.submission('webgpu');gpuReadback.submitted(device,originalSubmit);return result;};
  device.addEventListener('uncapturederror',event=>fail(event.error));
  device.lost.then(info=>{
    diagnostic('webgpu-device-lost',{deviceId,reason:info.reason,message:info.message,active:device===gpuDevice});
    if(device!==gpuDevice)return;
    controller.actualLoss('webgpu',info.reason+': '+info.message);audioAdapter?.suspend();refresh();
  });
}
function observeGpuContext(context,next){
  attach(next);
  if(observedGpuContexts.has(context))return;
  observedGpuContexts.add(context);
  diagnostic('webgpu-canvas-context-created');
  const configure=context.configure.bind(context);
  const getCurrentTexture=context.getCurrentTexture.bind(context);
  context.getCurrentTexture=()=>{const texture=getCurrentTexture();gpuReadback.acquired(context,texture);return texture;};
  context.configure=configuration=>{
    const result=configure(configuration);
    gpuReadback.configured(context,next,configuration);
    observeGpuDevice(configuration.device,{observation:'engine canvas configure',format:configuration.format,alphaMode:configuration.alphaMode??null});
    diagnostic('webgpu-canvas-configured',{deviceId:gpuDeviceIds.get(configuration.device),format:configuration.format,viewFormats:Array.from(configuration.viewFormats||[]),usage:configuration.usage??null,colorSpace:configuration.colorSpace??null,width:next.width,height:next.height});
    return result;
  };
}

// Observe the engine's own context. No independent capability probe can set actualBackend.
const originalGetContext=HTMLCanvasElement.prototype.getContext;
HTMLCanvasElement.prototype.getContext=function(kind,...args){
  const context=originalGetContext.call(this,kind,...args);
  if(context&&kind==='webgl2'&&context!==glContext){try{observeGl(context,this);}catch(error){fail(error);throw error;}}
  if(context&&kind==='webgpu')observeGpuContext(context,this);
  return context;
};
if(navigator.gpu){
  const originalRequestAdapter=navigator.gpu.requestAdapter.bind(navigator.gpu);
  navigator.gpu.requestAdapter=async options=>{
    diagnostic('webgpu-adapter-requested',{options:options??null});
    let adapter;
    try{adapter=await originalRequestAdapter(options);}
    catch(error){fail(error);throw error;}
    if(!adapter){fail('The engine WebGPU adapter request returned null');return adapter;}
    const info=adapter.info||{};
    const details={vendor:info.vendor??null,architecture:info.architecture??null,device:info.device??null,description:info.description??null,requestedOptions:options??null};
    diagnostic('webgpu-adapter-returned',details);
    const originalRequestDevice=adapter.requestDevice.bind(adapter);
    adapter.requestDevice=async descriptor=>{
      diagnostic('webgpu-device-requested',{requiredFeatures:Array.from(descriptor?.requiredFeatures||[]),requiredLimits:descriptor?.requiredLimits??null});
      try{
        const device=await originalRequestDevice(descriptor);
        observeGpuDevice(device,{...details,observation:'engine adapter requestDevice'});
        return device;
      }catch(error){fail(error);throw error;}
    };
    return adapter;
  };
}else if(controller.status.requestedBackend==='webgpu'){
  fail('navigator.gpu is unavailable in this browser context');
}
// Fyrox appends its canvas to body after creating the GL context. Reparent it into the controlled host.
new MutationObserver(records=>{
  for(const record of records)for(const node of record.addedNodes)if(node instanceof HTMLCanvasElement&&node.parentNode===document.body)attach(node);
}).observe(document.body,{childList:true});

function wireCanvas(target){
  target.addEventListener('webglcontextlost',event=>{event.preventDefault();controller.actualLoss('webgl2',event.statusMessage||'webglcontextlost');pointer=null;audioAdapter?.suspend();refresh();});
  target.addEventListener('webglcontextrestored',()=>{controller.status.lifecycle='restored-awaiting-reload';refresh();});
  target.addEventListener('pointerdown',event=>{
    if(event.button!==0||controller.status.suspended)return;
    target.focus();pointer={id:event.pointerId,x:event.clientX,y:event.clientY};target.setPointerCapture(event.pointerId);
  });
  target.addEventListener('pointercancel',()=>{pointer=null;controller.status.pointerCancelCount=(controller.status.pointerCancelCount||0)+1;});
  target.addEventListener('pointerup',event=>{
    const down=pointer;pointer=null;
    if(!down||down.id!==event.pointerId||document.activeElement!==target||controller.status.suspended)return;
    const rect=target.getBoundingClientRect();const x=Math.floor((event.clientX-rect.left)/rect.width*640),y=Math.floor((event.clientY-rect.top)/rect.height*480);
    if(x>=0&&x<640&&y>=0&&y<480)issue('selectAt',{x,y});
    if(target.hasPointerCapture(event.pointerId))target.releasePointerCapture(event.pointerId);
  });
  target.addEventListener('keydown',event=>{
    if(document.activeElement!==target||event.repeat||controller.status.suspended)return;
    const mode={'1':'full2d','2':'hybrid2d','3':'full3d'}[event.key];
    if(mode){event.preventDefault();issue('setMode',{mode});}
  });
}
wireCanvas(canvas);
document.addEventListener('focusin',()=>{controller.status.focus=document.activeElement?.id||document.activeElement?.tagName||'none';refresh();});
document.addEventListener('visibilitychange',()=>{pointer=null;issue(document.hidden?'suspend':'resume');});
window.addEventListener('pagehide',()=>{pointer=null;issue('suspend');});
window.addEventListener('pageshow',event=>{if(event.persisted)issue('resume');});
new ResizeObserver(()=>{controller.status.resizeEvents=(controller.status.resizeEvents||0)+1;window.dispatchEvent(new Event('resize'));refresh();}).observe(host);

document.querySelector('#variant').value=config.variant;
document.querySelector('#mode').value=config.mode;
document.querySelector('#tick').value=config.tick;
document.querySelector('#variant').addEventListener('change',event=>{const url=new URL(location);url.searchParams.set('variant',event.target.value);location.href=url;});
document.querySelector('#mode').addEventListener('change',event=>api.setMode(event.target.value));
document.querySelector('#apply-tick').addEventListener('click',()=>{try{api.setTick(Number(document.querySelector('#tick').value));}catch(error){fail(error);}});
document.querySelector('#pass').addEventListener('click',()=>api.setPass(controller.engine.pass==='pick'?'color':'pick'));
document.querySelector('#suspend').addEventListener('click',api.suspend);
document.querySelector('#resume').addEventListener('click',async()=>{api.resume();if(audioAdapter)await audioAdapter.resumeFromGesture();refresh();});
document.querySelector('#unlock-audio').addEventListener('click',async()=>{
  try{if(!audioAdapter)throw new Error('Audio adapter is unavailable');await audioAdapter.unlockFromGesture();refresh();}catch(error){fail(error);}
});
document.querySelector('#start-audio').addEventListener('click',()=>api.startAudio().catch(fail));
document.querySelector('#stop-audio').addEventListener('click',api.stopAudio);

function refresh(){
  const s=controller.snapshot();
  statusNode.textContent=s.errors.length?'Run failed':s.lifecycle==='lost'?'Graphics context lost — reload to rebuild resources':s.ready?'Fixture uploaded':s.readiness;
  document.querySelector('#requested').textContent=`${config.variant} (${s.requestedBackend})`;
  document.querySelector('#observed').textContent=s.actualBackend?`${s.actualBackend} · ${s.adapter?.renderer||s.adapter?.description||'device created'}`:'Awaiting engine context';
  document.querySelector('#hash').textContent=s.sceneHash||'Awaiting fixture';
  document.querySelector('#selection').textContent=s.selection?`Selection: ${s.selection.object_id} / generation ${s.selection.generation} (${s.selectionSource})`:'Selection: none';
  document.querySelector('#metrics').textContent=JSON.stringify({lifecycle:s.lifecycle,updateCount:s.updateCount,renderScheduleVisits:s.renderScheduleVisits,
    gpuSubmissions:s.gpuSubmissions,glDrawCalls:s.glDrawCalls,wasmMemoryShared:s.wasmMemoryShared,crossOriginIsolated:s.crossOriginIsolated,
    meshCount:s.meshCount,spriteCount:s.spriteCount,vertexCount:s.vertexCount,actualLossCount:s.actualLossCount,simulatedLossCount:s.simulatedLossCount,audio:audioSnapshot()},null,2);
  const errors=document.querySelector('#errors');errors.hidden=!s.errors.length;errors.textContent=s.errors.join('\n\n');
}
let lastSubmissions=0,lastPresentedAt=null;
function samplePresentation(now){
  const total=controller.status.gpuSubmissions+controller.status.glDrawCalls;
  if(total>lastSubmissions){
    if(lastPresentedAt!==null){controller.status.submittedFrameTimes.push(now-lastPresentedAt);if(controller.status.submittedFrameTimes.length>600)controller.status.submittedFrameTimes.shift();}
    lastPresentedAt=now;lastSubmissions=total;
  }
  requestAnimationFrame(samplePresentation);
}
requestAnimationFrame(samplePresentation);


for(const delay of [5000,15000,45000])setTimeout(()=>{
  const s=snapshot();
  if(!s.ready)diagnostic('startup-pending',{requestedBackend:s.requestedBackend,actualBackend:s.actualBackend,readiness:s.readiness,errors:s.errors,gpuSubmissions:s.gpuSubmissions,glDrawCalls:s.glDrawCalls,viewport:s.viewport});
},delay);

try{
  const {BrowserAudio}=await import('./audio/browser-audio.mjs');
  audioAdapter=new BrowserAudio({contextFactory:()=>new AudioContext(),maxVoices:8,maxPendingDecodes:4,maxPcmBytes:2*1024*1024,
    loadSample:async()=>{
      const samples=new Int16Array(12000);
      for(let i=0;i<samples.length;i++)samples[i]=Math.round(Math.sin(2*Math.PI*440*i/24000)*6000);
      return {pcm:{sampleRate:24000,channels:1,samples}};
    }});
}catch(error){controller.status.audio={available:false,error:String(error)};}
try{
  diagnostic('wasm-module-import-start',{path:`./pkg/${config.variant}/engine.js`});
  const module=await import(`./pkg/${config.variant}/engine.js`);
  diagnostic('wasm-module-imported',{exports:Object.keys(module)});
  diagnostic('wasm-initialization-start');
  wasm=await module.default();
  diagnostic('wasm-initialized',{memoryBytes:wasm?.memory?.buffer?.byteLength??null});
  if(!wasm?.memory)throw new Error('Engine WASM memory export is unavailable for the no-shared-memory gate');
  const shared=typeof SharedArrayBuffer!=='undefined'&&wasm.memory.buffer instanceof SharedArrayBuffer;
  controller.status.wasmMemoryShared=shared;
  if(shared)throw new Error('Shared WASM memory violates this probe baseline');
  controller.status.startupMs=performance.now()-started;
  if(typeof module.run!=='function')throw new Error('Packaged engine has no exported run()');
  diagnostic('engine-run-called');
  module.run();
  diagnostic('engine-run-returned');
}catch(error){fail(error);}
refresh();
