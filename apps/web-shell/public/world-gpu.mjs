// Disposable WebGL2 presentation of Rust-prepared source geometry. This module
// never advances simulation, authenticates a player, or invents an entity ID.
const owners=new WeakMap();
const MAX_BYTES=128*1024*1024;
function invalid(message){throw new Error('Source GPU frame: '+message);}
function integer(value,min,max){return Number.isSafeInteger(value)&&value>=min&&value<=max;}
function exactKeys(value,keys,name){
  if(!value||typeof value!=='object'||Array.isArray(value)||Object.keys(value).length!==keys.length||keys.some(key=>!Object.hasOwn(value,key)))invalid(name+' shape');
}
export function validateWorldGpuFrame(frame){
  if(!frame||![1,2].includes(frame.schema))invalid('unsupported schema');
  if(!integer(frame.width,1,4096)||!integer(frame.height,1,4096)||frame.width*frame.height>1048576)invalid('surface budget');
  if(typeof frame.generation!=='string'||!(/^[1-9][0-9]{0,19}$/).test(frame.generation)||BigInt(frame.generation)>18446744073709551615n)invalid('generation');
  if(!Array.isArray(frame.meshes)||!Array.isArray(frame.textures)||!Array.isArray(frame.draws)||frame.draws.length>262144||frame.meshes.length>262144||frame.textures.length>65535)invalid('collection budget');
  let bytes=frame.width*frame.height*8+frame.draws.length*(frame.schema===2?192:96),vertices=0,indices=0;
  function reserve(amount){bytes+=amount;if(!Number.isSafeInteger(bytes)||bytes>MAX_BYTES)invalid('upload budget');}
  for(const mesh of frame.meshes){
    if(!mesh||!Array.isArray(mesh.vertices)||mesh.vertices.length%9||!Array.isArray(mesh.indices)||mesh.indices.length%3)invalid('mesh shape');
    const count=mesh.vertices.length/9;vertices+=count;indices+=mesh.indices.length;
    if(vertices>2000000||indices>6000000)invalid('geometry budget');
    reserve(mesh.vertices.length*4+mesh.indices.length*4);
    if(mesh.vertices.some(value=>!Number.isFinite(value)||Math.abs(value)>3.4028234663852886e38))invalid('non-finite geometry');
    if(mesh.indices.some(index=>!integer(index,0,count-1)))invalid('mesh index');
  }
  for(const image of frame.textures){
    if(!image||!integer(image.width,1,4096)||!integer(image.height,1,4096)||!Array.isArray(image.pixels)||image.pixels.length!==image.width*image.height)invalid('texture shape');
    reserve(image.pixels.length*4);
    if(image.pixels.some(pixel=>!Array.isArray(pixel)||pixel.length!==4||pixel.some(value=>!integer(value,0,255))))invalid('texture bytes');
  }
  for(const draw of frame.draws){
    if(!draw||!integer(draw.mesh,0,frame.meshes.length-1)||(draw.texture!==null&&!integer(draw.texture,0,frame.textures.length-1)))invalid('draw reference');
    if(!Array.isArray(draw.matrix)||draw.matrix.length!==16||draw.matrix.some(value=>!Number.isFinite(value)||Math.abs(value)>3.4028234663852886e38))invalid('matrix');
    if(!integer(draw.pick_id,0,0xffffff)||typeof draw.depth_equal!=='boolean')invalid('draw state');
    if(frame.schema===1){if(Object.hasOwn(draw,'pipeline'))invalid('pipeline needs schema 2');}
    else{
      if(!Object.hasOwn(draw,'pipeline'))invalid('missing pipeline');
      if(draw.pipeline!==null){
        const p=draw.pipeline;
        exactKeys(p,['depth_compare','depth_write','forced_depth','stencil','blend'],'pipeline');
        if(!['less','less_equal','always'].includes(p.depth_compare)||typeof p.depth_write!=='boolean'
          ||(p.forced_depth!==null&&(!Number.isFinite(p.forced_depth)||p.forced_depth<0||p.forced_depth>1))
          ||!['source_over','non_premultiplied','no_color'].includes(p.blend)
          ||draw.depth_equal!==(p.depth_compare==='less_equal'))invalid('pipeline state');
        if(p.stencil!==null){
          exactKeys(p.stencil,['reference','clockwise','counterclockwise'],'stencil');
          if(!integer(p.stencil.reference,0,255))invalid('stencil reference');
          for(const face of [p.stencil.clockwise,p.stencil.counterclockwise]){
            exactKeys(face,['compare','pass','fail','depth_fail'],'stencil face');
            if(!['always','equal'].includes(face.compare)||[face.pass,face.fail,face.depth_fail].some(op=>!['keep','zero','replace'].includes(op)))invalid('stencil operation');
          }
        }
        if(p.blend==='no_color'&&(draw.texture!==null||draw.pick_id!==0||p.stencil===null))invalid('invisible mask texture/identity');
      }
    }
  }
  return frame;
}
const vertexSource=`#version 300 es
precision highp float;
layout(location=0) in vec3 aPosition;
layout(location=1) in vec2 aUv;
layout(location=2) in vec4 aColor;
uniform mat4 uMatrix;
uniform float uForcedDepth;
out vec2 vUv;
out vec4 vColor;
void main(){vec4 p=uMatrix*vec4(aPosition,1.0);if(uForcedDepth>=0.0)p.z=uForcedDepth*p.w;p.z=2.0*p.z-p.w;gl_Position=p;vUv=aUv;vColor=aColor;}`;
const fragmentSource=`#version 300 es
precision highp float;
in vec2 vUv;
in vec4 vColor;
uniform sampler2D uImage;
uniform bool uPick;
uniform vec3 uId;
out vec4 outputColor;
void main(){
  vec4 c=clamp(vColor,0.0,1.0)*texture(uImage,vUv);
  c=floor(c*255.0+0.5)/255.0;
  if(c.a<=2.0/255.0)discard;
  outputColor=uPick?vec4(uId,1.0):c;
}`;
// Every draw restores all relevant state; a portal's cleanup must not leak into
// later architecture, the ID pass, PNG capture, or the next admitted frame.
export function applySourceMaterialState(gl,program,draw,picking){
  const p=draw.pipeline??null;
  const noColor=p?.blend==='no_color';
  gl.frontFace(gl.CCW);
  gl.depthFunc(p?({less:gl.LESS,less_equal:gl.LEQUAL,always:gl.ALWAYS})[p.depth_compare]:(draw.depth_equal?gl.LEQUAL:gl.LESS));
  gl.depthMask(p?.depth_write??true);
  gl.uniform1f(gl.getUniformLocation(program,'uForcedDepth'),p?.forced_depth??-1);
  gl.colorMask(!noColor,!noColor,!noColor,!noColor);
  if(p?.stencil){
    gl.enable(gl.STENCIL_TEST);gl.stencilMask(0xff);
    const operations={keep:gl.KEEP,zero:gl.ZERO,replace:gl.REPLACE};
    // Source winding is measured before the CPU rasterizer canonicalizes it.
    // GL's positive-area CCW front matches the source counterclockwise face.
    for(const [which,face] of [[gl.FRONT,p.stencil.counterclockwise],[gl.BACK,p.stencil.clockwise]]){
      gl.stencilFuncSeparate(which,face.compare==='equal'?gl.EQUAL:gl.ALWAYS,p.stencil.reference,0xff);
      gl.stencilOpSeparate(which,operations[face.fail],operations[face.depth_fail],operations[face.pass]);
    }
  }else gl.disable(gl.STENCIL_TEST);
  if(picking||noColor)gl.disable(gl.BLEND);
  else{
    gl.enable(gl.BLEND);gl.blendEquation(gl.FUNC_ADD);
    gl.blendFuncSeparate(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA,p?.blend==='non_premultiplied'?gl.SRC_ALPHA:gl.ONE,gl.ONE_MINUS_SRC_ALPHA);
  }
}
function abort(message){return new DOMException(message,'AbortError');}
function required(value,name){if(!value)throw new Error('Cannot allocate source GPU '+name);return value;}
function compile(gl,type,source){
  const shader=required(gl.createShader(type),'shader');gl.shaderSource(shader,source);gl.compileShader(shader);
  if(!gl.getShaderParameter(shader,gl.COMPILE_STATUS)){const message=gl.getShaderInfoLog(shader);gl.deleteShader(shader);throw new Error('Source shader: '+message);}
  return shader;
}
function program(gl){
  const shaders=[];let result;
  try{
    shaders.push(compile(gl,gl.VERTEX_SHADER,vertexSource));
    shaders.push(compile(gl,gl.FRAGMENT_SHADER,fragmentSource));
    result=required(gl.createProgram(),'program');for(const shader of shaders)gl.attachShader(result,shader);gl.linkProgram(result);
    if(!gl.getProgramParameter(result,gl.LINK_STATUS))throw new Error('Source program: '+gl.getProgramInfoLog(result));
    return result;
  }catch(error){if(result)gl.deleteProgram(result);throw error;}
  finally{for(const shader of shaders)gl.deleteShader(shader);}
}
function release(gl,resources){
  if(!resources||resources.released)return;
  resources.released=true;
  for(const vao of resources.vaos)gl.deleteVertexArray(vao);
  for(const buffer of resources.buffers)gl.deleteBuffer(buffer);
  for(const texture of resources.textures)gl.deleteTexture(texture);
  for(const framebuffer of resources.framebuffers)gl.deleteFramebuffer(framebuffer);
  for(const buffer of resources.renderbuffers)gl.deleteRenderbuffer(buffer);
}
function emptyResources(){return {vaos:[],buffers:[],textures:[],framebuffers:[],renderbuffers:[]};}
class WorldGpuOwner{
  constructor(canvas){
    this.canvas=canvas;this.gl=required(canvas.getContext('webgl2',{alpha:false,antialias:false,depth:true,stencil:true,premultipliedAlpha:false}),'WebGL2 context');
    this.captureStore=new WorldPngCapture();
    this.resources=null;this.current=null;this.pending=null;this.disposed=false;this.lost=false;this.program=null;this.serial=0;
    this.onLost=event=>{event.preventDefault();this.lost=true;this.cancel('Graphics context lost');this.captureStore.clear('Graphics context lost. Capture again after recovery.');this.current=null;this.resources=null;this.program=null;canvas.setAttribute('data-gpu-state','lost');};
    this.onRestored=()=>{this.lost=false;canvas.setAttribute('data-gpu-state','restored-awaiting-frame');window.dispatchEvent(new Event('resize'));};
    this.onVisibility=()=>{if(document.hidden){this.cancel('Page suspended');this.captureStore.cancelPending('Page suspended. Capture again after returning.');}};
    canvas.addEventListener('webglcontextlost',this.onLost);canvas.addEventListener('webglcontextrestored',this.onRestored);
    document.addEventListener('visibilitychange',this.onVisibility);
  }
  check(){
    if(this.disposed||this.lost||this.gl.isContextLost())throw abort('Graphics owner is unavailable');
    const error=this.gl.getError();if(error!==this.gl.NO_ERROR)throw new Error('Source WebGL2 error '+error);
  }
  install(frame){
    this.check();const gl=this.gl,candidate=emptyResources(),meshes=[],images=[];
    gl.bindFramebuffer(gl.FRAMEBUFFER,null);
    if(frame.draws.some(draw=>draw.pipeline?.stencil)&&gl.getParameter(gl.STENCIL_BITS)<8)throw new Error('Source material masks require an 8-bit stencil surface');
    try{
      if(!this.program)this.program=program(gl);
      for(const mesh of frame.meshes){
        const vao=required(gl.createVertexArray(),'VAO');candidate.vaos.push(vao);gl.bindVertexArray(vao);
        const vertex=required(gl.createBuffer(),'vertex buffer');candidate.buffers.push(vertex);gl.bindBuffer(gl.ARRAY_BUFFER,vertex);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(mesh.vertices),gl.STATIC_DRAW);
        const index=required(gl.createBuffer(),'index buffer');candidate.buffers.push(index);gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,index);gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,new Uint32Array(mesh.indices),gl.STATIC_DRAW);
        for(const [location,size,offset] of [[0,3,0],[1,2,12],[2,4,20]]){gl.enableVertexAttribArray(location);gl.vertexAttribPointer(location,size,gl.FLOAT,false,36,offset);}
        meshes.push({vao,count:mesh.indices.length});
      }
      const texture=image=>{
        if(image.width>gl.getParameter(gl.MAX_TEXTURE_SIZE)||image.height>gl.getParameter(gl.MAX_TEXTURE_SIZE))throw new Error('Source texture exceeds device limits');
        const handle=required(gl.createTexture(),'texture');candidate.textures.push(handle);gl.bindTexture(gl.TEXTURE_2D,handle);
        gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,false);gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,false);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,image.width,image.height,0,gl.RGBA,gl.UNSIGNED_BYTE,image.pixels?new Uint8Array(image.pixels.flat()):null);
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
        return handle;
      };
      for(const image of frame.textures)images.push(texture(image));
      const white=texture({width:1,height:1,pixels:[[255,255,255,255]]});
      const ids=texture({width:frame.width,height:frame.height,pixels:null});
      const framebuffer=required(gl.createFramebuffer(),'ID framebuffer');candidate.framebuffers.push(framebuffer);gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);
      gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,ids,0);
      const depth=required(gl.createRenderbuffer(),'ID depth');candidate.renderbuffers.push(depth);gl.bindRenderbuffer(gl.RENDERBUFFER,depth);
      gl.renderbufferStorage(gl.RENDERBUFFER,gl.DEPTH24_STENCIL8,frame.width,frame.height);gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_STENCIL_ATTACHMENT,gl.RENDERBUFFER,depth);
      if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)throw new Error('Source ID framebuffer is incomplete');
      this.check();this.cancel('Frame replaced');this.captureStore.clear('The displayed view changed. Capture again.');
      const previous=this.resources;this.resources=candidate;this.current={frame,meshes,images,white,framebuffer};
      this.canvas.width=frame.width;this.canvas.height=frame.height;
      try{this.draw(false);}catch(error){this.current=null;this.resources=null;release(gl,candidate);release(gl,previous);throw error;}
      release(gl,previous);this.canvas.setAttribute('data-renderer','source-webgl2');this.canvas.setAttribute('data-gpu-state','ready');this.canvas.setAttribute('data-frame-generation',frame.generation);
    }catch(error){if(this.resources!==candidate)release(gl,candidate);throw error;}
    finally{gl.bindVertexArray(null);gl.bindBuffer(gl.ARRAY_BUFFER,null);gl.bindRenderbuffer(gl.RENDERBUFFER,null);gl.bindFramebuffer(gl.FRAMEBUFFER,null);}
  }
  draw(picking){
    this.check();const gl=this.gl,scene=this.current;if(!scene)throw abort('No admitted GPU frame');
    gl.bindFramebuffer(gl.FRAMEBUFFER,picking?scene.framebuffer:null);gl.viewport(0,0,scene.frame.width,scene.frame.height);
    gl.disable(gl.DITHER);gl.disable(gl.CULL_FACE);gl.disable(gl.SCISSOR_TEST);gl.disable(gl.STENCIL_TEST);gl.colorMask(true,true,true,true);
    gl.enable(gl.DEPTH_TEST);gl.depthMask(true);gl.clearDepth(1);gl.clearStencil(0);gl.stencilMask(0xff);
    if(picking){gl.disable(gl.BLEND);gl.clearColor(0,0,0,1);}
    else{gl.enable(gl.BLEND);gl.blendEquation(gl.FUNC_ADD);gl.blendFuncSeparate(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);gl.clearColor(229/255,235/255,222/255,1);}
    gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT|gl.STENCIL_BUFFER_BIT);gl.useProgram(this.program);
    gl.uniform1i(gl.getUniformLocation(this.program,'uPick'),picking?1:0);gl.uniform1i(gl.getUniformLocation(this.program,'uImage'),0);gl.activeTexture(gl.TEXTURE0);
    for(const draw of scene.frame.draws){
      const mesh=scene.meshes[draw.mesh];gl.bindVertexArray(mesh.vao);gl.bindTexture(gl.TEXTURE_2D,draw.texture===null?scene.white:scene.images[draw.texture]);
      gl.uniformMatrix4fv(gl.getUniformLocation(this.program,'uMatrix'),false,new Float32Array(draw.matrix));
      gl.uniform3f(gl.getUniformLocation(this.program,'uId'),(draw.pick_id&255)/255,((draw.pick_id>>>8)&255)/255,((draw.pick_id>>>16)&255)/255);
      applySourceMaterialState(gl,this.program,draw,picking);gl.drawElements(gl.TRIANGLES,mesh.count,gl.UNSIGNED_INT,0);
    }
    this.check();gl.colorMask(true,true,true,true);gl.depthMask(true);gl.disable(gl.STENCIL_TEST);gl.bindVertexArray(null);gl.bindFramebuffer(gl.FRAMEBUFFER,null);
  }
  cancel(message){
    const pending=this.pending;if(!pending)return;this.pending=null;clearTimeout(pending.timer);
    if(pending.sync)this.gl.deleteSync(pending.sync);if(pending.buffer)this.gl.deleteBuffer(pending.buffer);
    pending.reject(abort(message));
  }
  pick(x,y){
    this.cancel('Newer pick requested');
    return new Promise((resolve,reject)=>{
      let buffer,sync;
      try{
        this.check();const scene=this.current,gl=this.gl;if(!scene)throw abort('No admitted GPU frame');
        if(!integer(x,0,scene.frame.width-1)||!integer(y,0,scene.frame.height-1))throw new Error('Pick is outside the backing store');
        this.draw(true);gl.bindFramebuffer(gl.FRAMEBUFFER,scene.framebuffer);
        buffer=required(gl.createBuffer(),'pixel transfer');gl.bindBuffer(gl.PIXEL_PACK_BUFFER,buffer);gl.bufferData(gl.PIXEL_PACK_BUFFER,4,gl.STREAM_READ);
        gl.readPixels(x,scene.frame.height-y-1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,0);sync=required(gl.fenceSync(gl.SYNC_GPU_COMMANDS_COMPLETE,0),'pixel fence');gl.flush();
        gl.bindBuffer(gl.PIXEL_PACK_BUFFER,null);gl.bindFramebuffer(gl.FRAMEBUFFER,null);this.check();
        const pending={buffer,sync,reject,serial:++this.serial,timer:null,started:performance.now(),generation:scene.frame.generation};this.pending=pending;
        const poll=()=>{
          if(this.pending!==pending)return;
          try{
            this.check();if(!this.current||this.current.frame.generation!==pending.generation)throw abort('Stale GPU selection');
            if(document.hidden)throw abort('Page suspended');
            const result=gl.clientWaitSync(sync,0,0);
            if(result===gl.WAIT_FAILED)throw new Error('Source pixel transfer failed');
            if(result===gl.TIMEOUT_EXPIRED){if(performance.now()-pending.started>3000)throw new Error('Source pixel transfer timed out');pending.timer=setTimeout(poll,8);return;}
            const pixels=new Uint8Array(4);gl.bindBuffer(gl.PIXEL_PACK_BUFFER,buffer);gl.getBufferSubData(gl.PIXEL_PACK_BUFFER,0,pixels);gl.bindBuffer(gl.PIXEL_PACK_BUFFER,null);this.check();
            this.pending=null;gl.deleteSync(sync);gl.deleteBuffer(buffer);
            resolve(JSON.stringify({generation:pending.generation,index:pixels[0]|pixels[1]<<8|pixels[2]<<16,x,y}));
          }catch(error){this.pending=null;gl.bindBuffer(gl.PIXEL_PACK_BUFFER,null);gl.deleteSync(sync);gl.deleteBuffer(buffer);reject(error);}
        };
        pending.timer=setTimeout(poll,0);
      }catch(error){
        const gl=this.gl;gl.bindBuffer(gl.PIXEL_PACK_BUFFER,null);gl.bindFramebuffer(gl.FRAMEBUFFER,null);if(sync)gl.deleteSync(sync);if(buffer)gl.deleteBuffer(buffer);reject(error);
      }
    });
  }
  capture(generation,metadata){
    try{
      this.check();const scene=this.current;
      if(!scene||document.hidden||scene.frame.generation!==generation)throw abort('The displayed view changed. Capture again.');
      // Picking uses a separate framebuffer. Redraw color without reinstalling
      // geometry, advancing generation, or cancelling an unrelated pending pick.
      this.draw(false);
      return this.captureStore.capture(this.canvas,generation,metadata,()=>
        !this.disposed&&!this.lost&&!this.gl.isContextLost()&&!document.hidden&&this.current===scene);
    }catch(error){return Promise.reject(error);}
  }
  dispose(){
    if(this.disposed)return;this.cancel('View disposed');this.captureStore.clear('View disposed');this.disposed=true;
    this.canvas.removeEventListener('webglcontextlost',this.onLost);this.canvas.removeEventListener('webglcontextrestored',this.onRestored);document.removeEventListener('visibilitychange',this.onVisibility);
    release(this.gl,this.resources);if(this.program)this.gl.deleteProgram(this.program);
    this.resources=null;this.current=null;this.program=null;this.canvas.setAttribute('data-gpu-state','disposed');
  }
}
export function nextWorldPaint(){return new Promise(resolve=>requestAnimationFrame(resolve));}
export function paintSourceWorld(canvas,encoded){
  if(typeof encoded!=='string'||encoded.length>MAX_BYTES)invalid('encoded payload budget');
  const frame=validateWorldGpuFrame(JSON.parse(encoded));let owner=owners.get(canvas);
  if(!owner){owner=new WorldGpuOwner(canvas);owners.set(canvas,owner);}owner.install(frame);
}
export function pickSourceWorld(canvas,x,y){const owner=owners.get(canvas);return owner?owner.pick(x,y):Promise.reject(abort('View disposed'));}
export function captureSourceWorld(canvas,generation,metadata){const owner=owners.get(canvas);return owner?owner.capture(generation,metadata):Promise.reject(abort('View disposed'));}
export function clearSourceWorldCapture(canvas){owners.get(canvas)?.captureStore.clear();}
export function disposeSourceWorld(canvas){const owner=owners.get(canvas);owner?.dispose();owners.delete(canvas);}
export function worldGpuStats(canvas){
  const owner=owners.get(canvas),r=owner?.resources;
  return {ready:!!owner?.current,lost:owner?.lost??false,pending:!!owner?.pending,generation:owner?.current?.frame.generation??null,
    capturePending:!!owner?.captureStore.pending,captureUrls:owner?.captureStore.ready?.urls.length??0,
    meshes:r?.vaos.length??0,buffers:r?.buffers.length??0,textures:r?.textures.length??0,framebuffers:r?.framebuffers.length??0,renderbuffers:r?.renderbuffers.length??0};
}

// Keep this implementation in the wasm-bindgen snippet: no unbundled imports.
// One bounded, disposable source-view capture. Browser encoding and object URLs
// live here; source state, camera choices and frame identity remain Rust-owned.
const MAX_PIXELS = 1_048_576;
const MAX_PNG_BYTES = 8 * 1024 * 1024;
const MAX_METADATA_CHARS = 65_536;

function request(canvas, generation, encoded) {
  const {width, height} = canvas;
  if (![width, height].every(n => Number.isSafeInteger(n) && n > 0 && n <= 4096)
      || width * height > MAX_PIXELS) throw new Error('PNG capture exceeds the surface budget.');
  if (typeof generation !== 'string' || !/^[1-9][0-9]{0,19}$/.test(generation)
      || BigInt(generation) > 18446744073709551615n) throw new Error('Invalid capture generation.');
  if (typeof encoded !== 'string' || encoded.length > MAX_METADATA_CHARS) throw new Error('Capture metadata exceeds its budget.');
  let source;
  try { source = JSON.parse(encoded); } catch { throw new Error('Invalid capture metadata.'); }
  if (!source || typeof source !== 'object' || Array.isArray(source)) throw new Error('Invalid capture metadata.');
  return {width, height, generation, source};
}

function validatePng(bytes, width, height) {
  const signature = [137, 80, 78, 71, 13, 10, 26, 10];
  if (bytes.byteLength < 45 || signature.some((value, index) => bytes[index] !== value)) throw new Error('PNG encoder returned an invalid image.');
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint32(8) !== 13 || view.getUint32(12) !== 0x49484452
      || view.getUint32(16) !== width || view.getUint32(20) !== height) throw new Error('PNG encoder returned different dimensions.');
}

export class WorldPngCapture {
  constructor({urlApi = URL, cryptoApi = globalThis.crypto, timeoutMs = 10_000} = {}) {
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 30_000) throw new Error('Invalid capture deadline.');
    this.urlApi = urlApi;
    this.cryptoApi = cryptoApi;
    this.timeoutMs = timeoutMs;
    this.pending = null;
    this.ready = null;
    this.encoders = 0;
  }

  cancelPending(message = 'Capture cancelled.') {
    this.pending?.fail(abort(message));
  }

  clear(message = 'Captured view released.') {
    this.cancelPending(message);
    if (this.ready) {
      for (const url of this.ready.urls) this.urlApi.revokeObjectURL(url);
      this.ready = null;
    }
  }

  capture(canvas, generation, encoded, isCurrent) {
    this.clear('A newer capture was requested.');
    return new Promise((resolve, reject) => {
      let info;
      try {
        info = request(canvas, generation, encoded);
        if (this.encoders >= 2) throw new Error('PNG encoder is busy finishing a cancelled capture. Retry shortly.');
        if (!isCurrent()) throw abort('The displayed view changed. Capture again.');
        if (!this.cryptoApi?.subtle) throw new Error('PNG capture requires a secure browser context.');
      } catch (error) { reject(error); return; }
      const urls = [];
      const ticket = {
        timer: null,
        fail: error => {
          if (this.pending !== ticket) return;
          this.pending = null;
          clearTimeout(ticket.timer);
          for (const url of urls) this.urlApi.revokeObjectURL(url);
          reject(error);
        },
      };
      this.pending = ticket;
      const current = () => {
        if (this.pending !== ticket) return false;
        if (!isCurrent()) { ticket.fail(abort('The displayed view changed. Capture again.')); return false; }
        return true;
      };
      ticket.timer = setTimeout(() => ticket.fail(new DOMException('PNG encoding timed out. Try capturing again.', 'TimeoutError')), this.timeoutMs);
      let encoding = false;
      try {
        // Browser encoders cannot be cancelled. Keep at most two outstanding
        // snapshots, including cancelled requests whose callbacks have not run.
        this.encoders += 1;
        encoding = true;
        // Called immediately after the owner redraws its color pass, in the same
        // JS task. We do not read a compositor-cleared default drawing buffer.
        canvas.toBlob(async blob => {
          try {
            if (!current()) return;
            if (!blob) throw new Error('PNG encoding failed. Try capturing again.');
            if (blob.type !== 'image/png' || blob.size > MAX_PNG_BYTES || blob.size < 45) throw new Error('PNG encoder exceeded its format or byte budget.');
            const bytes = new Uint8Array(await blob.arrayBuffer());
            if (!current()) return;
            validatePng(bytes, info.width, info.height);
            const digest = await this.cryptoApi.subtle.digest('SHA-256', bytes);
            if (!current()) return;
            const sha256 = Array.from(new Uint8Array(digest), n => n.toString(16).padStart(2, '0')).join('');
            const manifest = {
              schema: 1,
              kind: 'source-view-capture',
              generation: info.generation,
              image: {width: info.width, height: info.height, bytes: blob.size, sha256},
              source: info.source,
              scope: 'Local rendered source view only; not a saved game, facade or authoritative snapshot.',
            };
            const metadata = new Blob([JSON.stringify(manifest, null, 2) + '\n'], {type: 'application/json'});
            urls.push(this.urlApi.createObjectURL(blob));
            urls.push(this.urlApi.createObjectURL(metadata));
            const filename = `wonderland-source-view-${generation}.png`;
            const receipt = {generation, width: info.width, height: info.height, image_url: urls[0], metadata_url: urls[1], filename, metadata_filename: `wonderland-source-view-${generation}.json`};
            this.ready = {urls, receipt};
            this.pending = null;
            clearTimeout(ticket.timer);
            resolve(JSON.stringify(receipt));
          } catch (error) { ticket.fail(error); }
          finally { this.encoders -= 1; }
        }, 'image/png');
      } catch (error) {
        if (encoding) this.encoders -= 1;
        ticket.fail(error);
      }
    });
  }
}
