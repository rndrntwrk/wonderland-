import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {stageContainerBootstrap,installPreviewExpiry} from './container-bootstrap.mjs';
const sha=b=>createHash('sha256').update(b).digest('hex');
function fixture(){
 const data=new Map([['runtime/lot.bin',Buffer.from('lot')],['runtime/world.json',Buffer.from('appearance')],
  ['release/manifest.json',Buffer.from('{}')],['browser/asset.dat',Buffer.from('archive')]]);
 data.set('runtime/config.json',Buffer.from(JSON.stringify({lot_file:'runtime/lot.bin',lot_sha256:sha(data.get('runtime/lot.bin')),
  appearance_file:'runtime/world.json',appearance_sha256:sha(data.get('runtime/world.json')),release_dir:'release'})));
 data.set('browser/content.json',Buffer.from(JSON.stringify({schema_version:1,files:[{path:'browser/asset.dat',sha256:sha(data.get('browser/asset.dat')),size:7}]})));
 const plan={schema_version:1,public_origin:'https://fixture.invalid',host_config:'runtime/config.json',content_manifest:'browser/content.json',
  manifest:{host_config:'runtime/config.json'},files:[...data].map(([name,bytes])=>({name,size:bytes.length,sha256:sha(bytes)}))};
 const bytes=Buffer.from(JSON.stringify(plan)),blobs=new Map([...data.values()].map(b=>[sha(b),b]));
 return {plan,bytes,readBlob:async hash=>blobs.get(hash)};
}
test('private bootstrap verifies all inputs and resolves only listed relative paths',async()=>{
 const f=fixture(),parent=await mkdtemp(join(tmpdir(),'native-bootstrap-'));
 const staged=await stageContainerBootstrap({root:join(parent,'fresh'),bytes:f.bytes,expectedSha256:sha(f.bytes),origin:f.plan.public_origin,readBlob:f.readBlob});
 const config=JSON.parse(await readFile(staged.manifest.host_config));assert.ok(config.lot_file.startsWith(parent));
 const content=JSON.parse(await readFile(staged.contentManifest));assert.ok(content.files[0].path.startsWith(parent));
 await assert.rejects(stageContainerBootstrap({root:join(parent,'fresh'),bytes:f.bytes,expectedSha256:sha(f.bytes),origin:f.plan.public_origin,readBlob:f.readBlob}),/EEXIST/);
});
test('wrong plan hash, escaped/unlisted paths and corrupt bytes fail closed',async()=>{
 for(const mode of ['plan-hash','escape','unlisted','corrupt']){
  const f=fixture(),parent=await mkdtemp(join(tmpdir(),'native-bootstrap-bad-'));
  if(mode==='escape')f.plan.files[0].name='../outside';
  if(mode==='unlisted')f.plan.host_config='not-listed.json';
  const bytes=Buffer.from(JSON.stringify(f.plan));
  await assert.rejects(stageContainerBootstrap({root:join(parent,'fresh'),bytes,
   expectedSha256:mode==='plan-hash'?'0'.repeat(64):sha(bytes),origin:f.plan.public_origin,
   readBlob:mode==='corrupt'?async()=>Buffer.from('corrupt'):f.readBlob}));
 }
});
test('expiry is fixed, independent of traffic and bounded under wall-clock rollback',()=>{
 let wall=1000,mono=0,pending,expired=0;
 installPreviewExpiry({expires:3000,now:()=>wall,monotonic:()=>mono,timer:fn=>{pending=fn;return 1;},clear:()=>{},expire:()=>expired++});
 wall=1500;mono=500;pending();assert.equal(expired,0);
 wall=500;mono=2000;pending();assert.equal(expired,1);pending();assert.equal(expired,1);
});
test('cancel clears expiry without granting a longer window',()=>{
 let pending,cleared=false,expired=false;
 const cancel=installPreviewExpiry({expires:2000,now:()=>1000,monotonic:()=>0,timer:fn=>{pending=fn;return 1;},clear:()=>{cleared=true;},expire:()=>{expired=true;}});
 cancel();pending();assert.equal(cleared,true);assert.equal(expired,false);
 assert.throws(()=>installPreviewExpiry({expires:3601001,now:()=>1000,expire:()=>{}}));
});
