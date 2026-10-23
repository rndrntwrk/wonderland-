import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtemp,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {createPrivateContentProvider} from './private-content.mjs';
const sha=value=>createHash('sha256').update(value).digest('hex');
async function fixture(change=x=>x){
 const root=await mkdtemp(join(tmpdir(),'wonderland-private-content-'));
 const path=join(root,'private.bin'),bytes=Buffer.from('isolated fixture');await writeFile(path,bytes);
 const input=change({schema_version:1,skeleton_name:'adult.skel',head_collections:['heads.col'],body_collections:['body.col'],
  files:[{input_name:'adult.skel',path,size:bytes.length,sha256:sha(bytes)}],total_input_bytes:bytes.length});
 const manifestPath=join(root,'manifest.json');await writeFile(manifestPath,JSON.stringify(input));
 return {input,path,manifestPath};
}
test('release-pinned resource projection never includes local paths and is independently cloned',async()=>{
 const f=await fixture(),p=await createPrivateContentProvider({release:'a'.repeat(64),manifestPath:f.manifestPath});
 assert.equal(JSON.stringify(p.manifest()).includes(f.path),false);
 assert.equal(p.manifest().content_release,'a'.repeat(64));
 const changed=p.manifest();changed.files[0].input_name='changed';assert.equal(p.manifest().files[0].input_name,'adult.skel');
 assert.equal((await p.read(f.input.files[0].sha256)).toString(),'isolated fixture');
 await assert.rejects(p.read('../private.bin'),/Unknown/);
});
test('changed source bytes cannot be served under the pinned hash',async()=>{
 const f=await fixture(),p=await createPrivateContentProvider({release:'a'.repeat(64),manifestPath:f.manifestPath});
 await writeFile(f.path,'different bytes');await assert.rejects(p.read(f.input.files[0].sha256),/hash mismatch/);
});
test('invalid resource names, duplicate names, wrong totals and oversized budgets fail before serving',async()=>{
 for(const change of [x=>({...x,skeleton_name:'../adult.skel'}),x=>({...x,files:[...x.files,...x.files]}),
  x=>({...x,total_input_bytes:0}),x=>({...x,files:[{...x.files[0],input_name:'/absolute'}]}),
  x=>({...x,files:[{...x.files[0],size:128*1024*1024+1}]})]){
  const f=await fixture(change);await assert.rejects(createPrivateContentProvider({release:'a'.repeat(64),manifestPath:f.manifestPath}));
 }
});
test('distinct original archives preserve repeated basenames and pinned identities',async()=>{
 const f=await fixture(),second=Buffer.from('second original archive'),path=f.path+'.second';
 await writeFile(path,second);
 f.input.files.push({input_name:'adult.skel',path,size:second.length,sha256:sha(second)});
 f.input.total_input_bytes+=second.length;await writeFile(f.manifestPath,JSON.stringify(f.input));
 const p=await createPrivateContentProvider({release:'a'.repeat(64),manifestPath:f.manifestPath});
 assert.equal(p.manifest().files.length,2);
 assert.equal((await p.read(sha(second))).toString(),second.toString());
});
