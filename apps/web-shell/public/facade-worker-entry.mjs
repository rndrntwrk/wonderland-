// Generated Rust bindings are packaged by Trunk's explicit worker target.
// This bootstrap has no DOM, session, server API or live renderer capability.
let used=false;
self.onmessage=async event=>{
 const {schema,id,bytes}=event.data??{};
 if(used)return;
 used=true;
 try{
  if(schema!==1||!Number.isInteger(id)||id<1||id>0xffffffff||
    !(bytes instanceof ArrayBuffer)||!bytes.byteLength||bytes.byteLength>32*1024*1024)
   throw new Error('Invalid facade worker request');
  const {default:init,executeFacadeRequest}=await import('./wonderland-facade-worker.js');
  await init();
  self.postMessage({schema:1,id,phase:'preparing'});
  // All expensive constructor work and output compression happen here. The
  // owner can terminate this Worker even while this synchronous Rust call runs.
  const [data,metadata,key]=executeFacadeRequest(new Uint8Array(bytes));
  if(!(data instanceof Uint8Array)||!data.byteLength||data.byteLength>16*1024*1024||
    typeof metadata!=='string'||metadata.length>65536||typeof key!=='string'||!/^[0-9a-f]{64}$/.test(key))
   throw new Error('Invalid facade worker output');
  self.postMessage({schema:1,id,phase:'done',bytes:data.buffer,metadata,key},[data.buffer]);
 }catch(error){self.postMessage({schema:1,id,phase:'failed',message:String(error).slice(0,512)});}
 finally{self.close();}
};
