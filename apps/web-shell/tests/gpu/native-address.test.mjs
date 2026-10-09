import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import * as gpu from '../../public/world-gpu.mjs';
const id=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
const frame=()=>({schema:4,width:8,height:8,generation:'9007199254740993',meshes:[{vertices:[0,0,0,1.25,-.75,1,1,1,1, 1,0,0,2.25,-.75,1,1,1,1, 0,1,0,1.25,.25,1,1,1,1],indices:[0,1,2]}],textures:[],draws:[{mesh:0,texture:null,matrix:id,pick_id:1,depth_equal:true,pipeline:null,texture_address:'wrap'}]});
test('schema 4 preserves explicit per-draw wrap or clamp',()=>{for(const value of ['wrap','clamp']){const f=frame();f.draws[0].texture_address=value;assert.equal(gpu.validateWorldGpuFrame(f),f);}});
test('older GPU schemas must not silently ignore addressing',()=>{for(const schema of [1,2,3]){const f=frame();f.schema=schema;if(schema===1)delete f.draws[0].pipeline;assert.throws(()=>gpu.validateWorldGpuFrame(f),/address|schema/);}});
test('schema 4 requires a valid addressing mode on every command',()=>{for(const value of [undefined,null,1,'repeat','mirrored_repeat']){const f=frame();f.draws[0].texture_address=value;assert.throws(()=>gpu.validateWorldGpuFrame(f));}});
test('wrap resets to clamp on every later color and ID draw',()=>{
 const writes=[];const gl={getUniformLocation:(_,name)=>name,uniform1i:(location,value)=>writes.push([location,value])};
 assert.equal(typeof gpu.applySourceTextureState,'function');
 for(const texture_address of ['wrap','clamp',undefined])gpu.applySourceTextureState(gl,{}, {texture_address});
 assert.deepEqual(writes,[['uWrap',1],['uWrap',0],['uWrap',0]]);
});
test('actual draw loop uses the tested addressing binder and fragment wrapping',async()=>{
 const source=await readFile(new URL('../../public/world-gpu.mjs',import.meta.url),'utf8');
 assert.match(source,/applySourceTextureState\(gl,this\.program,draw\)/);
 assert.match(source,/texture\(uImage,uWrap\?fract\(vUv\):vUv\)/);
});
