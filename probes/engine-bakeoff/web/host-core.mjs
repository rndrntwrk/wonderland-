const variants=new Set(['bevy-webgpu','bevy-webgl2','fyrox-webgl2']);
const modes=new Set(['full2d','hybrid2d','full3d']);
function integer(value,min,max,label){
  const n=Number(value);if(!Number.isSafeInteger(n)||n<min||n>max)throw new Error(`${label} must be an integer in ${min}..${max}`);return n;
}
export function parseConfig(search=''){
  const q=new URLSearchParams(search);
  const variant=q.get('variant')||`${q.get('engine')||'bevy'}-${q.get('backend')||'webgpu'}`;
  const mode=q.get('mode')||'hybrid2d';
  if(!variants.has(variant))throw new Error(`Unsupported engine/backend artifact: ${variant}`);
  if(!modes.has(mode))throw new Error(`Unknown view mode: ${mode}`);
  return Object.freeze({variant,mode,avatars:integer(q.get('avatars')??32,1,64,'avatars'),
    tick:integer(q.get('tick')??30,0,Number.MAX_SAFE_INTEGER,'tick'),frames:0});
}
export class ProbeController {
  constructor(config){
    this.config=config;this.queue=[];this.sequence=0;this.engine={};
    this.status={schemaVersion:1,requestedBackend:config.variant.split('-')[1],actualBackend:null,adapter:null,
      lifecycle:'starting',readiness:'loading',ready:false,errors:[],gpuSubmissions:0,glDrawCalls:0,
      simulatedLossCount:0,actualLossCount:0,lossEvents:[],suspended:false,focus:'none',
      submittedFrameTimes:[],wasmMemoryShared:null};
  }
  snapshot(){return structuredClone({...this.engine,...this.status,config:this.config,
    ready:this.engine.ready===true && this.status.actualBackend!==null && this.status.errors.length===0 && !['lost','restored-awaiting-reload'].includes(this.status.lifecycle),
    readiness:this.status.errors.length?'failed':this.engine.readiness||this.status.readiness,
  });}
  publish(value){
    this.engine=value;
    if(this.status.lifecycle==='starting')this.status.lifecycle='running';
    for(const error of value.engineErrors||[])if(!this.status.errors.includes(error))this.fail(error);
  }
  observeBackend(backend,adapter){
    if(backend!==this.status.requestedBackend)throw new Error(`Requested ${this.status.requestedBackend}, engine created ${backend}; no fallback is counted as success`);
    this.status.actualBackend=backend;this.status.adapter=adapter;
  }
  submission(backend){
    if(backend==='webgpu')this.status.gpuSubmissions++;
    else if(backend==='webgl2')this.status.glDrawCalls++;
  }
  enqueue(kind,args={}){
    if(this.queue.length>=128)throw new Error('Presentation command queue limit (128) reached');
    switch(kind){
      case 'setMode':if(!modes.has(args.mode))throw new Error('Unknown view mode');break;
      case 'setTick':integer(args.tick,0,Number.MAX_SAFE_INTEGER,'tick');break;
      case 'selectAt':
        integer(args.x,0,639,'pick x');integer(args.y,0,479,'pick y');
        if(this.status.suspended||this.status.lifecycle==='lost')throw new Error('Picking is suspended');break;
      case 'setPass':if(!['color','pick'].includes(args.pass))throw new Error('Unknown render pass');break;
      case 'suspend':this.status.suspended=true;break;
      case 'resume':this.status.suspended=false;break;
      case 'simulateLoss':this.status.suspended=true;this.status.simulatedLossCount++;break;
      case 'reloadFixture':break;
      default:throw new Error(`Unknown presentation command: ${kind}`);
    }
    const seq=++this.sequence;this.queue.push({seq,kind,...args});return seq;
  }
  drain(){return this.queue.splice(0);}
  actualLoss(backend,reason){
    this.status.actualLossCount++;this.status.lifecycle='lost';this.status.suspended=true;
    this.status.lossEvents.push({backend,reason,kind:'observed-context-or-device-loss'});
    if(this.status.lossEvents.length>16)this.status.lossEvents.shift();
    // Invalidate the engine's pick generation too; this is not a simulated-loss event.
    if(this.queue.length<128)this.queue.push({seq:++this.sequence,kind:'deviceLost'});
  }
  fail(reason){this.status.errors.push(String(reason));if(this.status.errors.length>32)this.status.errors.shift();}
}
