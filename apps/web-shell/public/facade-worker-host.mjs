// Own at most one disposable, non-shared-memory facade worker. A cancellation
// terminates computation even while Rust validation/preparation/encoding is busy.
const MAX_INPUT=32*1024*1024,MAX_OUTPUT=16*1024*1024;
let active=null,nextId=1;
const defaults=()=>({Worker:globalThis.Worker,baseURI:globalThis.document?.baseURI,
 setTimeout:(...args)=>globalThis.setTimeout(...args),clearTimeout:id=>globalThis.clearTimeout(id)});
function stop(job){
 if(job.stopped)return;
 job.stopped=true;job.environment.clearTimeout(job.timer);job.worker.terminate();
}
export function cancelFacadeWorker(id){
 if(active?.id!==id)return;
 const job=active;active=null;stop(job);job.result=null;
}
export function startFacadeWorker(bytes,environment=defaults()){
 if(active)throw new Error('One facade worker is already active');
 if(!(bytes instanceof Uint8Array)||!bytes.byteLength||bytes.byteLength>MAX_INPUT||
   (typeof SharedArrayBuffer!=='undefined'&&bytes.buffer instanceof SharedArrayBuffer))
  throw new Error('Invalid facade worker input budget');
 if(typeof environment.Worker!=='function')throw new Error('Facade workers are unavailable in this browser');
 if(nextId>0xffffffff)throw new Error('Reopen the page to renew facade worker identities');
 const url=new URL('facade-worker-entry.mjs',environment.baseURI);
 if(!['https:','http:'].includes(url.protocol))throw new Error('Invalid facade worker URL');
 const copy=new Uint8Array(bytes); // copy the borrowed WASM view before returning
 const id=nextId++,worker=new environment.Worker(url,{type:'module',name:'wonderland-facade'});
 const job={id,worker,environment,phase:'starting',timer:null,stopped:false,result:null,error:null};
 active=job;
 const fail=message=>{if(active!==job||job.stopped)return;job.error=message;job.phase='failed';stop(job);};
 worker.onerror=event=>{event.preventDefault?.();fail('Facade worker failed');};
 worker.onmessageerror=()=>fail('Facade worker result could not be decoded');
 worker.onmessage=event=>{
  if(active!==job||job.stopped)return; // old owners cannot complete a replacement
  const m=event.data;
  if(!m||m.schema!==1||m.id!==id){fail('Invalid facade worker protocol or identity');return;}
  if(m.phase==='preparing'&&job.phase==='starting'){job.phase='preparing';return;}
  if(m.phase==='failed'){fail('Facade worker failed: '+String(m.message??'unknown error').slice(0,512));return;}
  if(m.phase!=='done'||!(m.bytes instanceof ArrayBuffer)||!m.bytes.byteLength||m.bytes.byteLength>MAX_OUTPUT||
    typeof m.metadata!=='string'||m.metadata.length>65536||typeof m.key!=='string'||! /^[0-9a-f]{64}$/.test(m.key)){
   fail('Invalid facade worker result budget or protocol');return;
  }
  job.result=[new Uint8Array(m.bytes),m.metadata,m.key];job.phase='done';stop(job);
 };
 try{
  job.timer=environment.setTimeout(()=>fail('Facade worker timeout'),120000);
  worker.postMessage({schema:1,id,bytes:copy.buffer},[copy.buffer]);
 }catch(error){cancelFacadeWorker(id);throw error;}
 return id;
}
export function pollFacadeWorker(id){
 if(active?.id!==id)throw new Error('Facade worker finished, cancelled or unknown');
 const job=active;
 if(job.error){const message=job.error;cancelFacadeWorker(id);throw new Error(message);}
 if(job.result){const result=job.result;job.result=null;cancelFacadeWorker(id);return result;}
 return job.phase;
}
