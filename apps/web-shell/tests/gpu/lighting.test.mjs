import test from 'node:test';
import assert from 'node:assert/strict';
import {validateWorldGpuFrame} from '../../public/world-gpu.mjs';
const identity=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
function frame(){return {schema:3,width:8,height:8,generation:'1',meshes:[{vertices:[0,0,0,0,0,1,1,1,1, 1,0,0,1,0,1,1,1,1, 0,1,0,0,1,1,1,1,1],indices:[0,1,2]}],textures:[{width:1,height:1,pixels:[[255,255,255,255]]}],draws:[{mesh:0,texture:null,matrix:[...identity],pick_id:1,depth_equal:false,pipeline:null,light:{texture:0,matrix:[...identity]}}]};}
test('schema 3 admits explicit independent light UV and texture',()=>{const f=frame();assert.equal(validateWorldGpuFrame(f),f);});
test('older schemas reject lighting rather than silently discarding it',()=>{
 for(const schema of [1,2]){const f=frame();f.schema=schema;if(schema===1)delete f.draws[0].pipeline;assert.throws(()=>validateWorldGpuFrame(f),/light/);}
});
test('light image and transform references are checked before GPU allocation',()=>{
 for(const mutate of [l=>l.texture=1,l=>l.texture=-1,l=>l.matrix[0]=NaN,l=>l.matrix[0]=Infinity,l=>l.matrix.pop(),l=>l.unknown=1]){
  const f=frame();mutate(f.draws[0].light);assert.throws(()=>validateWorldGpuFrame(f));
 }
});
test('invisible stencil passes cannot carry a lightmap or visible identity',()=>{
 const f=frame(),face={compare:'always',pass:'keep',fail:'keep',depth_fail:'keep'};
 Object.assign(f.draws[0],{pick_id:0,pipeline:{depth_compare:'less',depth_write:true,forced_depth:null,blend:'no_color',stencil:{reference:0,clockwise:{...face},counterclockwise:{...face}}}});
 assert.throws(()=>validateWorldGpuFrame(f),/light/);
});
