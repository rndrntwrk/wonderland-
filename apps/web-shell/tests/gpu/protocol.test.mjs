import test from 'node:test';
import assert from 'node:assert/strict';
import {validateWorldGpuFrame} from '../../public/world-gpu.mjs';
function frame(){return {schema:1,width:64,height:48,generation:'1',meshes:[{vertices:[0,0,0,0,0,1,1,1,1, 1,0,0,1,0,1,1,1,1, 0,1,0,0,1,1,1,1,1],indices:[0,1,2]}],textures:[],draws:[{mesh:0,texture:null,matrix:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],pick_id:1,depth_equal:false}]};}
test('invalid dimensions and identities fail before graphics allocation',()=>{
  for(const patch of [{width:0},{height:8192},{width:2048,height:2048},{generation:1},{generation:'0'},{generation:'18446744073709551616'}]){
    assert.throws(()=>validateWorldGpuFrame({...frame(),...patch}));
  }
});
test('non-finite geometry, incomplete vertices and invalid indices are rejected',()=>{
  for(const mutate of [f=>f.meshes[0].vertices[1]=NaN,f=>f.meshes[0].vertices.pop(),f=>f.meshes[0].indices[0]=99,f=>f.meshes[0].indices.push(1),f=>f.draws[0].matrix[2]=Infinity]){
    const f=frame();mutate(f);assert.throws(()=>validateWorldGpuFrame(f));
  }
});
test('draw references and exact RGB24 IDs are validated',()=>{
  for(const patch of [{mesh:1},{texture:0},{pick_id:-1},{pick_id:0x1000000},{pick_id:0.5},{depth_equal:'true'}]){
    const f=frame();Object.assign(f.draws[0],patch);assert.throws(()=>validateWorldGpuFrame(f));
  }
});
test('dimensions, RGBA shape and byte values are validated before upload',()=>{
  for(const image of [{width:1,height:1,pixels:[]},{width:1,height:1,pixels:[[0,0,0,256]]},{width:1,height:1,pixels:[[0,0,0]]},{width:0,height:1,pixels:[]}]){
    const f=frame();f.textures=[image];assert.throws(()=>validateWorldGpuFrame(f));
  }
});
test('valid shared geometry, textures and decimal generation are preserved',()=>{
  const f=frame();f.generation='18446744073709551615';
  f.textures=[{width:1,height:1,pixels:[[17,34,51,255]]}];f.draws[0].texture=0;
  f.draws.push({...f.draws[0],pick_id:2});
  assert.equal(validateWorldGpuFrame(f),f);assert.equal(f.meshes.length,1);assert.equal(f.textures.length,1);
});
