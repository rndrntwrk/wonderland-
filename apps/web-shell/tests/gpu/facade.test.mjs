import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFile} from 'node:fs/promises';
const root=new URL('../../',import.meta.url);
test('source inspector exposes the actual Rust facade job, cooperative yield and cleanup',async()=>{
 const screen=await readFile(new URL('src/source_world_screen.rs',root),'utf8');
 assert.match(screen,/<WorldFacadePanel/);
 const source=await readFile(new URL('src/world_facade.rs',root),'utf8');
 assert.match(source,/WorldFacadeJob::new/); assert.match(source,/job\.step\(4\)/);
 assert.match(source,/JsFuture::from\(yield_facade\(\)\)/);
 assert.match(source,/on_cleanup/); assert.match(source,/Arc::ptr_eq/);
});

import {makeFacadeLinks,releaseFacadeLinks} from '../../public/facade-links.mjs';
const key='a'.repeat(64);
const bytes=new Uint8Array([70,83,79,102,1,0,0,0,1,31,139,8]);
const metadata=()=>JSON.stringify({kind:'presentation_facade',source_hash:key,bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex'),not_a_game_save:true});
test('facade URLs contain the actual bytes and are revoked on discard',async()=>{
 const [file,details]=await makeFacadeLinks(bytes,metadata(),key);
 assert.deepEqual(new Uint8Array(await (await fetch(file)).arrayBuffer()),bytes);
 assert.equal(await (await fetch(details)).text(),metadata());
 releaseFacadeLinks(file,details);
 await assert.rejects(fetch(file)); await assert.rejects(fetch(details));
});
test('facade transport rejects mixed identities, wrong sizes and unsupported input before publication',async()=>{
 for(const bad of [null,new Uint8Array(8),new Uint8Array(16*1024*1024+1)])await assert.rejects(()=>makeFacadeLinks(bad,metadata(),key));
 await assert.rejects(()=>makeFacadeLinks(bytes,metadata(),'../filename'));
 await assert.rejects(()=>makeFacadeLinks(bytes,metadata().replace(key,'b'.repeat(64)),key));
 await assert.rejects(()=>makeFacadeLinks(bytes,metadata().replace('true','false'),key));
 await assert.rejects(()=>makeFacadeLinks(bytes,'x'.repeat(65537),key));
});
test('a second URL allocation failure releases the first URL',async()=>{
 const create=URL.createObjectURL,revoke=URL.revokeObjectURL;
 let calls=0;const released=[];
 URL.createObjectURL=()=>{if(++calls===2)throw new Error('quota');return 'blob:test-facade';};
 URL.revokeObjectURL=url=>released.push(url);
 try{await assert.rejects(()=>makeFacadeLinks(bytes,metadata(),key),/quota/);assert.deepEqual(released,['blob:test-facade']);}
 finally{URL.createObjectURL=create;URL.revokeObjectURL=revoke;}
});
