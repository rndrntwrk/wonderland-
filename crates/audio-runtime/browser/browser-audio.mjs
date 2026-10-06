/**
 * C-local Web Audio adapter (MPL-2.0).
 *
 * Accepts the Rust MixerIntent JSON shape. `loadSample(assetKey, {signal})`
 * resolves an authorized provider entry: either {pcm:{sampleRate,channels,
 * samples:Int16Array|Float32Array}}, or {encoded:ArrayBuffer,format:'wav'|'mp3',
 * decodedBytes:<declared float32 AudioBuffer bytes>}. Encoded resources with
 * source sampleRate/frames/channels reserve for the device's resampling rate
 * and keep seek positions in source frames. XA/UTK must first be
 * cooked by the bounded Rust decoder. No arbitrary URL fetch occurs here.
 *
 * All callbacks and clocks below belong exclusively to presentation. The
 * adapter accepts no simulation reference and returns no gameplay ack.
 */
const MAX_U64=(1n<<64n)-1n;
function identity(value) {
  if(typeof value==='number' && (!Number.isSafeInteger(value)||value<0))throw Error('invalid audio identity');
  if(!['string','number','bigint'].includes(typeof value))throw Error('invalid audio identity');
  if(typeof value==='string'&&!/^(0|[1-9][0-9]*)$/.test(value))throw Error('invalid audio identity');
  const result=BigInt(value);if(result<0n||result>MAX_U64)throw Error('invalid audio identity');return result;
}
function keyOf(value) {
  if(typeof value==='string'&&/^[0-9a-f]{64}$/.test(value))return value;
  if(!Array.isArray(value)||value.length!==32||value.some(x=>!Number.isInteger(x)||x<0||x>255))throw Error('invalid audio asset key');
  return value.map(x=>x.toString(16).padStart(2,'0')).join('');
}
function gainPan(gain,pan){if(!Number.isFinite(gain)||gain<0||gain>1||!Number.isFinite(pan)||pan<-1||pan>1)throw Error('invalid audio gain or pan');}
function positive(value,name){if(!Number.isSafeInteger(value)||value<1)throw Error(`invalid ${name} budget`);return value;}
export class BrowserAudio {
  constructor({contextFactory=()=>new (globalThis.AudioContext??globalThis.webkitAudioContext)(),loadSample,maxVoices=128,maxPendingDecodes=16,maxPcmBytes=64*1024*1024,maxEncodedBytes=4*1024*1024}={}) {
    if(typeof loadSample!=='function'||typeof contextFactory!=='function')throw Error('audio provider and context factory required');
    this._factory=contextFactory;this._load=loadSample;this._maxVoices=positive(maxVoices,'voice');this._maxDecodes=positive(maxPendingDecodes,'decode');this._maxPcm=positive(maxPcmBytes,'PCM');this._maxEncoded=positive(maxEncodedBytes,'encoded');
    this.state='locked';this._context=null;this._epoch=0;this._voices=new Map();this._queue=[];this._completed=[];this._cache=new Map();this._inflight=new Map();this._requests=new Set();this._pcmBytes=0;this._reserved=0;this._generation=null;this._serial=0n;this._lastError=null;this._errors=0;this._operation=0;this._unlocking=null;this._desiredState='locked';this._needsGesture=false;this._change=()=>this._stateChanged();
  }
  _live(){if(this.state==='disposed')throw Error('audio adapter disposed');}
  async unlockFromGesture() {
    this._live();
    if(globalThis.navigator?.userActivation && !globalThis.navigator.userActivation.isActive)throw Error('audio unlock requires a user gesture');
    const operation=++this._operation;this._unlocking=operation;this._desiredState='running';
    try {
      if(this._context?.state==='closed'){this._context.removeEventListener?.('statechange',this._change);this._context=null;}
      if(!this._context){
        const context=this._factory();
        if(!context||typeof context.resume!=='function')throw Error('Web Audio unavailable');
        this._context=context;context.addEventListener?.('statechange',this._change);
      }
      const context=this._context;await context.resume();
      if(this.state==='disposed'||this._context!==context||this._operation!==operation)throw Error('audio unlock cancelled');
      if(context.state!=='running')throw Error(`audio context ${context.state}`);
      this._needsGesture=false;this._lastError=null;this.state='running';this._pump();this._flush();
    }
    catch(error){if(this._operation===operation&&this.state!=='disposed'){this._error(error);if(this.state!=='suspended'&&this.state!=='interrupted'){this.state='locked';this._desiredState='locked';}}throw error;}
    finally{if(this._unlocking===operation)this._unlocking=null;}
  }
  async resumeFromGesture(){return this.unlockFromGesture();}
  applyAll(intents){this._live();if(!Array.isArray(intents)||intents.length>this._maxVoices*8)throw Error('audio intent batch budget');for(const intent of intents)this.apply(intent);}
  apply(intent) {
    this._live();if(!intent||typeof intent!=='object'||Object.keys(intent).length!==1)throw Error('invalid mixer intent');
    const [kind]=Object.keys(intent), data=intent[kind];if(!data?.voice)throw Error('voice identity required');
    const generation=identity(data.voice.generation),serial=identity(data.voice.serial),id=`${generation}:${serial}`;
    if(generation===0n||serial===0n)throw Error('zero audio identity');
    if(kind==='Start'){
      gainPan(data.gain,data.pan);const key=keyOf(data.sample);const seek=identity(data.seek_frame??0);
      if(typeof data.looped!=='boolean'||!['Fx','Music','Vox','Ambience'].includes(data.group))throw Error('invalid voice parameters');
      if(this._generation!==null && (generation<this._generation||(generation===this._generation&&serial<=this._serial)))throw Error('stale or replayed voice identity');
      if(this._generation!==null&&generation>this._generation)this.reset();
      if(this._voices.size+this._completed.length>=this._maxVoices)throw Error('voice budget exceeded');
      this._generation=generation;this._serial=serial;
      const voice={id,key,asset:Array.isArray(data.sample)?data.sample.slice():data.sample,gain:data.gain,pan:data.pan,group:data.group,looped:data.looped,seek,offset:0,paused:false,status:'queued',node:null,gainNode:null,panNode:null,startedAt:0,epoch:this._epoch};
      this._voices.set(id,voice);this._queue.push(id);this._pump();this._flush();return;
    }
    if(!['Stop','Release','Pause','Resume','SetGainPan'].includes(kind))throw Error('unknown mixer intent');
    if(kind==='SetGainPan')gainPan(data.gain,data.pan);
    if(kind==='Stop'||kind==='Release')this._completed=this._completed.filter(v=>v.id!==id);
    const voice=this._voices.get(id);if(!voice)return;
    if(kind==='Stop'||kind==='Release'){this._finish(voice,true);this._flush();this._pump();}
    else if(kind==='SetGainPan'){voice.gain=data.gain;voice.pan=data.pan;voice.gainNode?.gain.setValueAtTime(data.gain,this._context.currentTime);voice.panNode?.pan.setValueAtTime(data.pan,this._context.currentTime);}
    else if(kind==='Pause'){voice.paused=true;if(voice.node){this._savePhase(voice);this._releaseNodes(voice,true);voice.status='ready';}}
    else if(kind==='Resume'){
      voice.paused=false;
      if(voice.status==='ready'&&this.state==='running')this._start(voice);
      else{if(voice.status==='ready'&&!this._queue.includes(voice.id))this._queue.push(voice.id);this._pump();this._flush();}
    }
  }
  _error(error){this._errors=Math.min(Number.MAX_SAFE_INTEGER,this._errors+1);this._lastError=String(error?.message??error).slice(0,1024);}
  _needed(key,epoch){return epoch===this._epoch&&this.state!=='disposed'&&[...this._voices.values()].some(v=>v.key===key&&v.epoch===epoch);}
  _ownsRequest(request){return !request.cancelled&&this._inflight.get(request.key)===request&&this._needed(request.key,request.epoch);}
  _cancelRequest(request){
    request.cancelled=true;
    if(this._inflight.get(request.key)===request)this._inflight.delete(request.key);
    request.controller.abort();
    // Keep the unsettled request and PCM reservation charged until finally.
  }
  _pump() {
    if(!this._context||this.state!=='running')return;
    for(const voice of this._voices.values()){
      if(voice.status!=='queued'&&voice.status!=='loading')continue;
      if(this._cache.has(voice.key)){voice.status='ready';continue;}
      if(this._inflight.has(voice.key)){voice.status='loading';continue;}
      if(this._requests.size>=this._maxDecodes)break;
      const request={key:voice.key,epoch:this._epoch,controller:new AbortController(),promise:null,reserved:0,cancelled:false};
      this._inflight.set(voice.key,request);this._requests.add(request);voice.status='loading';
      request.promise=Promise.resolve().then(()=>this._load(voice.asset,{signal:request.controller.signal})).then(async resource=>{
        if(!this._ownsRequest(request))return;
        const buffer=await this._decode(resource,request);
        if(!this._ownsRequest(request))return;
        const bytes=buffer.length*buffer.numberOfChannels*4;
        this._validateBuffer(buffer);this._releaseReservation(request);this._evictFor(bytes);
        this._cache.set(request.key,{buffer,bytes,sourceSampleRate:request.sourceSampleRate??buffer.sampleRate,sourceFrames:request.sourceFrames??buffer.length});this._pcmBytes+=bytes;
        for(const v of this._voices.values())if(v.key===request.key&&v.epoch===request.epoch)v.status='ready';
      }).catch(error=>{
        if(this._ownsRequest(request)){
          this._error(error);
          for(const v of [...this._voices.values()])if(v.key===request.key)this._finish(v,true,true);
        }
      }).finally(()=>{
        this._releaseReservation(request);this._requests.delete(request);if(this._inflight.get(request.key)===request)this._inflight.delete(request.key);this._pump();this._flush();
      });
    }
    this._flush();
  }
  _validateBuffer(buffer){if(!buffer||![1,2].includes(buffer.numberOfChannels)||!Number.isInteger(buffer.length)||buffer.length<1||!Number.isInteger(buffer.sampleRate)||buffer.sampleRate<1||buffer.sampleRate>384000||buffer.length*buffer.numberOfChannels*4>this._maxPcm)throw Error('invalid or oversized decoded PCM');}
  _evictFor(bytes){
    if(!Number.isSafeInteger(bytes)||bytes<1||bytes>this._maxPcm)throw Error('PCM memory budget exceeded');
    for(const [key,value] of this._cache){if(this._pcmBytes+this._reserved+bytes<=this._maxPcm&&this._cache.size<this._maxVoices*2)break;if(![...this._voices.values()].some(v=>v.key===key)){this._cache.delete(key);this._pcmBytes-=value.bytes;}}
    if(this._pcmBytes+this._reserved+bytes>this._maxPcm)throw Error('PCM memory budget exceeded');
    if(this._cache.size>=this._maxVoices*2)throw Error('PCM cache entry budget exceeded');
  }
  _releaseReservation(request){if(request.reserved){this._reserved-=request.reserved;request.reserved=0;}}
  _encodedBytes(resource){
    let bytes=positive(resource.decodedBytes,'declared decoded PCM');
    if(resource.sampleRate!==undefined||resource.frames!==undefined||resource.channels!==undefined){
      const {sampleRate,frames,channels}=resource;
      if(!Number.isSafeInteger(sampleRate)||sampleRate<1||sampleRate>384000||!Number.isSafeInteger(frames)||frames<1||![1,2].includes(channels)||!Number.isSafeInteger(frames*channels*4)||bytes<frames*channels*4)throw Error('invalid encoded source PCM metadata');
      const rate=this._context?.sampleRate??sampleRate;
      if(!Number.isSafeInteger(rate)||rate<1||rate>384000)throw Error('invalid audio context sample rate');
      const framesAtDevice=(BigInt(frames)*BigInt(rate)+BigInt(sampleRate)-1n)/BigInt(sampleRate);
      const resampledBytes=framesAtDevice*BigInt(channels)*4n;
      if(resampledBytes>BigInt(Number.MAX_SAFE_INTEGER))throw Error('PCM memory budget exceeded');
      bytes=Math.max(bytes,Number(resampledBytes));
    }
    return bytes;
  }
  requiresStreaming(resource){
    return !!resource?.encoded&&(resource.encoded.byteLength>this._maxEncoded||this._encodedBytes(resource)>this._maxPcm);
  }
  async _decode(resource,request){
    if(resource?.pcm){
      const {sampleRate,channels,samples}=resource.pcm;
      if(![1,2].includes(channels)||!Number.isInteger(sampleRate)||sampleRate<1||sampleRate>384000||!(samples instanceof Int16Array||samples instanceof Float32Array)||samples.length===0||samples.length%channels!==0)throw Error('invalid PCM provider shape');
      this._evictFor(samples.length*4);request.reserved=samples.length*4;this._reserved+=request.reserved;
      const buffer=this._context.createBuffer(channels,samples.length/channels,sampleRate);
      for(let channel=0;channel<channels;channel++){const out=buffer.getChannelData(channel);for(let i=0;i<out.length;i++){const value=samples[i*channels+channel];if(!Number.isFinite(value))throw Error('nonfinite PCM sample');out[i]=samples instanceof Int16Array?value/32768:Math.max(-1,Math.min(1,value));}}
      return buffer;
    }
    if(!resource||!['wav','mp3'].includes(resource.format)||!(resource.encoded instanceof ArrayBuffer))throw Error('unsupported provider encoding; XA/UTK require cooked PCM');
    if(resource.encoded.byteLength<1||resource.encoded.byteLength>this._maxEncoded)throw Error('encoded audio budget exceeded');
    const bytes=this._encodedBytes(resource);this._evictFor(bytes);request.reserved=bytes;this._reserved+=request.reserved;
    request.sourceSampleRate=resource.sampleRate;request.sourceFrames=resource.frames;
    const buffer=await this._context.decodeAudioData(resource.encoded.slice(0));
    this._validateBuffer(buffer);if(resource.channels!==undefined&&buffer.numberOfChannels!==resource.channels)throw Error('decoded PCM channel mismatch');if(buffer.length*buffer.numberOfChannels*4>request.reserved)throw Error('decoded PCM exceeds provider declaration');return buffer;
  }
  _flush(){
    if(this.state!=='running')return;
    while(this._queue.length){const id=this._queue[0],voice=this._voices.get(id);if(!voice){this._queue.shift();continue;}if(voice.status==='queued'||voice.status==='loading')break;this._queue.shift();if(voice.status==='ready'&&!voice.paused)this._start(voice);}
    // Explicitly paused voices leave the initial queue; resuming them is a new
    // presentation action and does not block other source-ordered starts.
  }
  _start(voice){
    if(this.state!=='running'||voice.paused||voice.node)return;
    const item=this._cache.get(voice.key);if(!item)return;
    try{
      const buffer=item.buffer;
      if(voice.seek!==null){let frame=voice.seek;if(voice.looped)frame%=BigInt(item.sourceFrames);if(frame>=BigInt(item.sourceFrames))throw Error('seek beyond sample');voice.offset=Number(frame)/item.sourceSampleRate;voice.seek=null;}
      if(voice.offset>=buffer.duration){if(voice.looped)voice.offset%=buffer.duration;else{this._finish(voice,false,true);return;}}
      if(typeof this._context.createStereoPanner!=='function')throw Error('StereoPanner capability unavailable');
      const node=this._context.createBufferSource(),gain=this._context.createGain(),pan=this._context.createStereoPanner();voice.node=node;voice.gainNode=gain;voice.panNode=pan;
      node.buffer=buffer;node.loop=voice.looped;gain.gain.setValueAtTime(voice.gain,this._context.currentTime);pan.pan.setValueAtTime(voice.pan,this._context.currentTime);node.connect(gain);gain.connect(pan);pan.connect(this._context.destination);
      node.onended=()=>{if(this._voices.get(voice.id)===voice&&voice.node===node)this._finish(voice,false,true);};voice.startedAt=this._context.currentTime;voice.status='playing';node.start(0,voice.offset);
    }catch(error){this._error(error);this._finish(voice,true,true);}
  }
  _savePhase(voice){
    const buffer=this._cache.get(voice.key)?.buffer;if(!buffer)return;voice.offset+=Math.max(0,this._context.currentTime-voice.startedAt);if(voice.looped)voice.offset%=buffer.duration;else voice.offset=Math.min(voice.offset,buffer.duration);
  }
  _releaseNodes(voice,stop){
    const nodes=[voice.node,voice.gainNode,voice.panNode];voice.node=null;voice.gainNode=null;voice.panNode=null;
    if(nodes[0]){nodes[0].onended=null;if(stop)try{nodes[0].stop();}catch{}}
    for(const node of nodes)try{node?.disconnect();}catch{}
  }
  _finish(voice,stop,report=false){
    if(this._voices.get(voice.id)!==voice)return;
    this._releaseNodes(voice,stop);this._voices.delete(voice.id);this._queue=this._queue.filter(id=>id!==voice.id);voice.status='finished';
    if(report){const [generation,serial]=voice.id.split(':');this._completed.push({id:voice.id,generation,serial});}
    if(!this._needed(voice.key,voice.epoch)){const request=this._inflight.get(voice.key);if(request&&request.epoch===voice.epoch)this._cancelRequest(request);}
  }
  // Completed voices reserve capacity until drained, so no lifecycle event is
  // silently lost. These identities are presentation feedback, never A acks.
  takeFinished(){const completed=this._completed;this._completed=[];return completed.map(({generation,serial})=>({generation,serial}));}
  _stateChanged(){
    if(this.state==='disposed'||!this._context)return;
    const state=this._context.state;
    if(state==='interrupted'||state==='closed'){
      this._needsGesture=true;this.state='interrupted';
      for(const voice of [...this._voices.values()]){if(!voice.looped){this._finish(voice,true,true);}else if(voice.node){this._savePhase(voice);this._releaseNodes(voice,true);voice.status='ready';if(!this._queue.includes(voice.id))this._queue.push(voice.id);}}
    }else if(state==='suspended'){this.state=this._needsGesture&&this._desiredState!=='suspended'?'interrupted':'suspended';}
    else if(state==='running'){
      if(this._desiredState==='suspended'){
        this.state='suspended';
        // A resume which was already pending when Pause was pressed may finish
        // late. Keep new voices gated and restore the requested device state.
        const context=this._context,operation=this._operation;
        Promise.resolve(context.suspend()).catch(error=>{if(this._context===context&&this._operation===operation&&this.state!=='disposed')this._error(error);});
      }else if(this._needsGesture&&this._unlocking===null){this.state='interrupted';}
      else if(this._desiredState==='running'){this.state='running';this._pump();this._flush();}
    }
  }
  async suspend(){this._live();const operation=++this._operation;this._unlocking=null;this._desiredState='suspended';this.state='suspended';if(!this._context)return;const context=this._context;await context.suspend();if(this.state!=='disposed'&&this._context===context&&this._operation===operation)this.state='suspended';}
  reset(){
    this._live();this._epoch++;
    for(const voice of [...this._voices.values()])this._finish(voice,true);
    this._queue=[];this._completed=[];this._cache.clear();this._pcmBytes=0;this._inflight.clear();for(const request of this._requests)this._cancelRequest(request);
    // Outstanding decoder reservations remain charged until those asynchronous
    // requests actually settle; reset storms cannot evade the decode budget.
  }
  async dispose(){if(this.state==='disposed')return;this.reset();this._operation++;this._unlocking=null;this.state='disposed';if(this._context){this._context.removeEventListener?.('statechange',this._change);const context=this._context;try{await context.close();}finally{if(this._context===context)this._context=null;}}}
  async settled(){while(this._requests.size){await Promise.all([...this._requests].map(r=>r.promise));}this._flush();}
  snapshot(){return {state:this.state,activeVoices:this._voices.size,queuedEntries:this._queue.length,completedVoices:this._completed.length,pendingStarts:this._queue.filter(id=>this._voices.has(id)).length,pendingDecodes:this._requests.size,cachedSamples:this._cache.size,pcmBytes:this._pcmBytes,reservedPcmBytes:this._reserved,errors:this._errors,lastError:this._lastError};}
}
