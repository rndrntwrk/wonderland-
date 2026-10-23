// Authenticated source resources are memory-only, same-origin and hash-pinned.
// This loader does not cache, persist, redirect or fall back to public assets.
const digest=value=>typeof value==='string'&&/^[0-9a-f]{64}$/.test(value);
const name=value=>typeof value==='string'&&value.length>0&&value.length<=256
  &&!value.startsWith('/')&&!value.includes('\\')
  &&value.split('/').every(part=>part&&part!=='.'&&part!=='..');
async function bytes(response,maximum,type){
 if(!response.ok||response.headers.get('content-type')?.split(';')[0]!==type)
  throw Error('Authenticated game content is unavailable.');
 const length=response.headers.get('content-length');
 if(length!==null&&(!/^(0|[1-9][0-9]*)$/.test(length)||Number(length)>maximum))
  throw Error('Game content exceeds its byte budget.');
 const reader=response.body?.getReader();if(!reader)throw Error('Missing game content.');
 const chunks=[];let size=0;
 try{
  for(;;){const {value,done}=await reader.read();if(done)break;
   size+=value.byteLength;if(size>maximum)throw Error('Game content exceeds its byte budget.');chunks.push(value);}
  const result=new Uint8Array(size);let offset=0;
  for(const chunk of chunks){result.set(chunk,offset);offset+=chunk.byteLength;}return result;
 }finally{await reader.cancel().catch(()=>{});reader.releaseLock();}
}
export async function loadNativeContent({read,signal,current=()=>true,crypto=globalThis.crypto,expectedRelease}){
 const assertCurrent=()=>{if(signal?.aborted||!current())throw Error('This owned content session changed.');};
 assertCurrent();
 const manifest=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(
  await bytes(await read('/native-content/manifest',signal),128*1024,'application/json')));
 assertCurrent();
 if(manifest.schema_version!==1||!digest(manifest.content_release)||!name(manifest.skeleton_name)
  ||!Array.isArray(manifest.files)||!manifest.files.length||manifest.files.length>256
  ||!Number.isSafeInteger(manifest.total_input_bytes)||manifest.total_input_bytes<1
  ||manifest.total_input_bytes>128*1024*1024)throw Error('Invalid private game content manifest.');
 if(expectedRelease!==undefined&&manifest.content_release!==expectedRelease)throw Error('Game content release does not match the eligible account catalog.');
 for(const key of ['head_collections','body_collections'])
  if(!Array.isArray(manifest[key])||!manifest[key].length||manifest[key].length>16||!manifest[key].every(name))
   throw Error('Invalid game content collection selection.');
 const names=new Set();let total=0;
 for(const row of manifest.files){
  const identity=row.input_name+'\0'+row.sha256;
  if(!name(row.input_name)||names.has(identity)||!digest(row.sha256)
   ||!Number.isSafeInteger(row.size)||row.size<1||row.size>128*1024*1024)
   throw Error('Invalid private game resource.');
  names.add(identity);total+=row.size;
 }
 if(total!==manifest.total_input_bytes)throw Error('Private resource total mismatch.');
 const files=[];
 // Sequential reads bound transfer pressure and preserve source session checks.
 for(const row of manifest.files){
  assertCurrent();
  const data=await bytes(await read('/native-content/file/'+row.sha256,signal),row.size,'application/octet-stream');
  if(data.length!==row.size)throw Error('Private resource size mismatch.');
  const hash=new Uint8Array(await crypto.subtle.digest('SHA-256',data));
  if(Array.from(hash,b=>b.toString(16).padStart(2,'0')).join('')!==row.sha256)
   throw Error('Private resource hash mismatch.');
  assertCurrent();files.push(new File([data],row.input_name));
 }
 return {manifest:JSON.stringify(manifest),files};
}
