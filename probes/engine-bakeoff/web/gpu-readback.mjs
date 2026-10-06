// Diagnostic only: read the engine's submitted canvas texture, not a replacement
// renderer. The presentation screenshots remain an independent required gate.
export class EngineCanvasReadback {
  constructor({diagnostic=()=>{},stamp=()=>({}),timeoutMs=15000}={}){
    this.diagnostic=diagnostic;this.stamp=stamp;this.timeoutMs=timeoutMs;
    this.session=null;this.texture=null;this.serial=0;this.pending=null;
    this.submissions=0;this.requestSerial=0;this.last=null;
  }
  configured(context,canvas,configuration){
    this.cancel('Canvas reconfigured during diagnostic readback');
    this.session={context,canvas,configuration:{...configuration}};this.texture=null;
  }
  unconfigured(context){
    if(context!==this.session?.context)return;
    this.cancel('Engine canvas unconfigured');this.session=null;this.texture=null;
  }
  deviceLost(device,reason){
    if(device!==this.session?.configuration.device)return;
    this.cancel('Engine WebGPU device lost: '+reason);this.session=null;this.texture=null;
  }
  cancel(reason){
    if(this.pending)this.finish(this.pending,new Error(reason));
  }
  progress(pending,stage,detail={}){
    pending.stage=stage;
    const event={stage,elapsedMs:Math.round(performance.now()-pending.started),...detail};
    pending.progress.push(event);
    if(pending.progress.length>16)pending.progress.shift();
    this.diagnostic('engine-readback-'+stage,{request:pending.id,readbackStage:stage,readbackElapsedMs:event.elapsedMs,...detail});
  }
  describe(pending){
    return {request:pending.id,stage:pending.stage,elapsedMs:Math.round(performance.now()-pending.started),
      acquisitions:this.serial-pending.afterSerial,submissions:this.submissions-pending.afterSubmission,
      stamp:pending.stamp,progress:pending.progress.map(event=>({...event}))};
  }
  snapshot(){
    return {configured:!!this.session,textureSerial:this.serial,submissions:this.submissions,
      pending:this.pending?this.describe(this.pending):null,last:this.last};
  }
  acquired(context,texture){
    if(context!==this.session?.context)return;
    this.texture=texture;this.serial++;
    if(this.pending&&!this.pending.scheduled&&this.pending.stage==='awaiting-texture')this.progress(this.pending,'awaiting-submission',{textureSerial:this.serial});
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
      const pending={resolve,reject,id:++this.requestSerial,started:performance.now(),progress:[],stamp:structuredClone(this.stamp()),
        afterSerial:this.serial,afterSubmission:this.submissions,scheduled:false,buffer:null};
      pending.timer=setTimeout(()=>this.finish(pending,new Error(`Engine texture readback deadline at ${pending.stage}`)),this.timeoutMs);
      this.pending=pending;
      this.progress(pending,'awaiting-texture');
    });
  }
  submitted(device,submit){
    if(device===this.session?.configuration.device)this.submissions++;
    const pending=this.pending;
    if(!pending||pending.scheduled||device!==this.session?.configuration.device||!this.texture||this.serial<=pending.afterSerial)return;
    pending.scheduled=true;
    const frame={session:this.session,texture:this.texture,serial:this.serial};
    this.progress(pending,'copy-scheduled',{textureSerial:frame.serial});
    // Capture after all synchronous engine submissions in this task and before
    // the canvas texture's automatic expiry task. No engine submit is replaced.
    queueMicrotask(()=>{if(this.pending===pending)this.copy(pending,device,submit,frame);});
  }
  async copy(pending,device,submit,frame){
    let buffer;
    try{
      const {texture,session:{canvas,configuration}}=frame;
      if(frame.session!==this.session)throw new Error('Engine canvas changed before diagnostic copy');
      if(JSON.stringify(pending.stamp)!==JSON.stringify(this.stamp()))throw new Error('Fixture changed before diagnostic copy');
      const width=texture.width,height=texture.height;
      if(!Number.isInteger(width)||!Number.isInteger(height)||width<1||height<1||width>4096||height>4096||width!==canvas.width||height!==canvas.height)throw new Error('Engine texture and canvas dimensions differ');
      const bytesPerRow=Math.ceil(width*4/256)*256;
      const metadata={width,height,format:configuration.format,textureSerial:frame.serial,stamp:pending.stamp,usage:configuration.usage,
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
        this.progress(pending,'copy-submitted',{textureSerial:frame.serial,bytesPerRow,byteLength:bytesPerRow*height});
      }catch(error){encodingError=error;}
      // Pop before yielding so the scope cannot consume later engine errors.
      const validation=device.popErrorScope();
      this.progress(pending,'awaiting-validation');
      const validationError=await validation;
      if(this.pending!==pending)return;
      if(encodingError)throw encodingError;
      if(validationError)throw new Error('Diagnostic GPU validation failed: '+validationError.message);
      this.progress(pending,'awaiting-map');
      await buffer.mapAsync(GPUMapMode.READ);
      if(this.pending!==pending)return;
      this.progress(pending,'mapped');
      const mapped=new Uint8Array(buffer.getMappedRange()),pixels=new Uint8Array(width*height*4);
      for(let y=0;y<height;y++)pixels.set(mapped.subarray(y*bytesPerRow,y*bytesPerRow+width*4),y*width*4);
      if(configuration.format==='bgra8unorm')for(let i=0;i<pixels.length;i+=4){const red=pixels[i];pixels[i]=pixels[i+2];pixels[i+2]=red;}
      let binary='';for(let i=0;i<pixels.length;i+=0x8000)binary+=String.fromCharCode(...pixels.subarray(i,i+0x8000));
      this.finish(pending,null,{...metadata,base64:btoa(binary),readback:this.describe(pending)});
    }catch(error){this.finish(pending,error);}
    finally{this.release(pending);}
  }
  release(pending){
    const buffer=pending.buffer;pending.buffer=null;
    if(!buffer)return;
    try{buffer.unmap();}catch{}
    buffer.destroy();
  }
  finish(pending,error,result){
    if(this.pending!==pending)return;
    this.pending=null;clearTimeout(pending.timer);
    this.last={...this.describe(pending),outcome:error?'rejected':'completed',error:error?String(error):null};
    this.diagnostic('engine-readback-finished',{...this.last,stage:'engine-readback-finished',readbackStage:this.last.stage});
    this.release(pending);
    if(error){error.readback=this.last;pending.reject(error);}else pending.resolve(result);
  }
}
