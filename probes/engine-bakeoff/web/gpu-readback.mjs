// Diagnostic only: read the engine's submitted canvas texture, not a replacement
// renderer. The presentation screenshots remain an independent required gate.
export class EngineCanvasReadback {
  constructor({diagnostic=()=>{},stamp=()=>({}),timeoutMs=15000}={}){
    this.diagnostic=diagnostic;this.stamp=stamp;this.timeoutMs=timeoutMs;
    this.session=null;this.texture=null;this.serial=0;this.pending=null;
  }
  configured(context,canvas,configuration){
    if(this.pending)this.finish(this.pending,new Error('Canvas reconfigured during diagnostic readback'));
    this.session={context,canvas,configuration};this.texture=null;
  }
  acquired(context,texture){
    if(context!==this.session?.context)return;
    this.texture=texture;this.serial++;
  }
  read(){
    if(this.pending)return Promise.reject(new Error('A diagnostic readback is already pending'));
    const session=this.session;
    if(!session)return Promise.reject(new Error('No configured engine WebGPU canvas'));
    if(!['rgba8unorm','bgra8unorm'].includes(session.configuration.format))return Promise.reject(new Error('Unsupported diagnostic canvas format'));
    const usage=session.configuration.usage??GPUTextureUsage.RENDER_ATTACHMENT;
    if(!(usage&GPUTextureUsage.COPY_SRC)){
      // Requested only after ordinary screenshots, preserving their original
      // surface configuration. The reconfiguration is explicit in the evidence.
      this.diagnostic('engine-readback-copy-src-enabled',{previousUsage:usage,usage:usage|GPUTextureUsage.COPY_SRC});
      session.context.configure({...session.configuration,usage:usage|GPUTextureUsage.COPY_SRC});
    }
    return new Promise((resolve,reject)=>{
      const pending={resolve,reject,afterSerial:this.serial,scheduled:false,buffer:null};
      pending.timer=setTimeout(()=>this.finish(pending,new Error('No engine texture readback completed within the diagnostic deadline')),this.timeoutMs);
      this.pending=pending;
    });
  }
  submitted(device,submit){
    const pending=this.pending;
    if(!pending||pending.scheduled||device!==this.session?.configuration.device||!this.texture||this.serial<=pending.afterSerial)return;
    pending.scheduled=true;
    // Capture after all synchronous engine submissions in this task and before
    // the canvas texture's automatic expiry task. No engine submit is replaced.
    queueMicrotask(()=>{if(this.pending===pending)this.copy(pending,device,submit);});
  }
  async copy(pending,device,submit){
    let buffer;
    try{
      const texture=this.texture,{canvas,configuration}=this.session;
      const width=texture.width,height=texture.height;
      if(!Number.isInteger(width)||!Number.isInteger(height)||width<1||height<1||width>4096||height>4096||width!==canvas.width||height!==canvas.height)throw new Error('Engine texture and canvas dimensions differ');
      const bytesPerRow=Math.ceil(width*4/256)*256;
      const metadata={width,height,format:configuration.format,textureSerial:this.serial,stamp:this.stamp(),usage:configuration.usage,
        evidence:'Actual engine canvas texture copied after its queue submission; independent of browser presentation'};
      let encodingError;
      device.pushErrorScope('validation');
      try{
        buffer=device.createBuffer({label:'Wonderland diagnostic readback',size:bytesPerRow*height,usage:GPUBufferUsage.COPY_DST|GPUBufferUsage.MAP_READ});pending.buffer=buffer;
        const encoder=device.createCommandEncoder({label:'Wonderland diagnostic canvas copy'});
        encoder.copyTextureToBuffer({texture},{buffer,bytesPerRow,rowsPerImage:height},{width,height,depthOrArrayLayers:1});
        // Use the unwrapped queue method to exclude the diagnostic copy from
        // engine counters and prevent recursively triggering another copy.
        submit([encoder.finish()]);
      }catch(error){encodingError=error;}
      // Pop before yielding so the scope cannot consume later engine errors.
      const validation=device.popErrorScope();
      const validationError=await validation;
      if(encodingError)throw encodingError;
      if(validationError)throw new Error('Diagnostic GPU validation failed: '+validationError.message);
      if(this.pending!==pending)return;
      await buffer.mapAsync(GPUMapMode.READ);
      if(this.pending!==pending)return;
      const mapped=new Uint8Array(buffer.getMappedRange()),pixels=new Uint8Array(width*height*4);
      for(let y=0;y<height;y++)pixels.set(mapped.subarray(y*bytesPerRow,y*bytesPerRow+width*4),y*width*4);
      if(configuration.format==='bgra8unorm')for(let i=0;i<pixels.length;i+=4){const red=pixels[i];pixels[i]=pixels[i+2];pixels[i+2]=red;}
      let binary='';for(let i=0;i<pixels.length;i+=0x8000)binary+=String.fromCharCode(...pixels.subarray(i,i+0x8000));
      this.finish(pending,null,{...metadata,base64:btoa(binary)});
    }catch(error){this.finish(pending,error);}
    finally{try{buffer?.unmap();}catch{}buffer?.destroy();}
  }
  finish(pending,error,result){
    if(this.pending!==pending)return;
    this.pending=null;clearTimeout(pending.timer);
    if(error){pending.buffer?.destroy();pending.reject(error);}else pending.resolve(result);
  }
}
