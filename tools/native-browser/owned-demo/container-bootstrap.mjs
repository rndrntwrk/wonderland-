import assert from 'node:assert/strict';
import {mkdir,writeFile,readFile} from 'node:fs/promises';
import {resolve,dirname} from 'node:path';
import {createHash} from 'node:crypto';
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const relative=name=>typeof name==='string'&&/^[a-zA-Z0-9_./-]{1,192}$/.test(name)
 &&!name.startsWith('/')&&name.split('/').every(p=>p&&p!=='.'&&p!=='..');
export async function boundedResource(response,maximum){
 assert.ok(response.ok,'Private bootstrap resource unavailable');
 const reader=response.body?.getReader();assert.ok(reader,'Missing bootstrap body');
 const parts=[];let length=0;
 try{for(;;){const {value,done}=await reader.read();if(done)break;
  length+=value.byteLength;assert.ok(length<=maximum,'Bootstrap byte limit');parts.push(value);}
  return Buffer.concat(parts,length);
 }finally{await reader.cancel().catch(()=>{});reader.releaseLock();}
}
export async function stageContainerBootstrap({root,bytes,expectedSha256,origin,readBlob}){
 assert.match(expectedSha256,/^[0-9a-f]{64}$/);assert.equal(sha(bytes),expectedSha256,'Bootstrap hash mismatch');
 assert.ok(bytes.length<=2*1024*1024);const plan=JSON.parse(bytes);
 assert.equal(plan.schema_version,1);assert.equal(plan.public_origin,origin);
 const url=new URL(origin);assert.equal(url.protocol,'https:');assert.equal(url.origin,origin);
 assert.ok(relative(plan.host_config)&&relative(plan.content_manifest));
 assert.equal(plan.manifest?.host_config,plan.host_config);
 assert.ok(Array.isArray(plan.files)&&plan.files.length>0&&plan.files.length<=64);
 const files=new Map();let total=0;
 for(const f of plan.files){assert.ok(relative(f.name)&&!files.has(f.name));assert.match(f.sha256,/^[0-9a-f]{64}$/);
  assert.ok(Number.isSafeInteger(f.size)&&f.size>0&&f.size<=128*1024*1024);total+=f.size;
  assert.ok(total<=160*1024*1024);files.set(f.name,f);}
 assert.ok(files.has(plan.host_config)&&files.has(plan.content_manifest));
 // Fresh ephemeral cache only. A prior partial cache never substitutes for a
 // verified object, and no supplied path can escape this directory.
 await mkdir(root);
 for(const f of files.values()){
  const data=await readBlob(f.sha256,f.size);assert.equal(data.length,f.size);assert.equal(sha(data),f.sha256,'Private resource hash mismatch');
  const path=resolve(root,f.name);await mkdir(dirname(path),{recursive:true});await writeFile(path,data,{flag:'wx'});
 }
 const config=JSON.parse(await readFile(resolve(root,plan.host_config),'utf8'));
 for(const [field,hashField] of [['lot_file','lot_sha256'],['appearance_file','appearance_sha256']]){
  const f=files.get(config[field]);assert.ok(f,`Unlisted ${field}`);assert.equal(config[hashField],f.sha256);
  config[field]=resolve(root,f.name);
 }
 assert.ok(relative(config.release_dir));
 assert.ok([...files.keys()].some(name=>name.startsWith(config.release_dir+'/')),'Missing sealed release');
 config.release_dir=resolve(root,config.release_dir);
 const content=JSON.parse(await readFile(resolve(root,plan.content_manifest),'utf8'));
 assert.equal(content.schema_version,1);assert.ok(Array.isArray(content.files));
 for(const row of content.files){const f=files.get(row.path);assert.ok(f,'Unlisted browser resource');
  assert.equal(f.sha256,row.sha256);assert.equal(f.size,row.size);row.path=resolve(root,f.name);}
 const configPath=resolve(root,'resolved-host-config.json'),contentPath=resolve(root,'resolved-browser-content.json');
 assert.ok(!files.has('resolved-host-config.json')&&!files.has('resolved-browser-content.json'));
 await writeFile(configPath,JSON.stringify(config),{flag:'wx'});await writeFile(contentPath,JSON.stringify(content),{flag:'wx'});
 return {manifest:{...plan.manifest,host_config:configPath},contentManifest:contentPath};
}

export function installPreviewExpiry({expires,expire,now=Date.now,monotonic=()=>performance.now(),timer=setTimeout,clear=clearTimeout}){
 const remaining=expires-now();assert.ok(Number.isSafeInteger(expires)&&remaining>0&&remaining<=3600000,'Invalid preview deadline');
 const began=monotonic();let closed=false;
 const check=()=>{if(closed)return;
  if(now()>=expires||monotonic()-began>=remaining){closed=true;expire();return;}
  handle=timer(check,Math.min(1000,expires-now(),remaining-(monotonic()-began)));};
 let handle=timer(check,Math.min(1000,remaining));
 return ()=>{closed=true;clear(handle);};
}
