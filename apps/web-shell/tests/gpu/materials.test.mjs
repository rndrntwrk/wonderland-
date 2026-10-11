import test from 'node:test';
import assert from 'node:assert/strict';
import {validateWorldGpuFrame} from '../../public/world-gpu.mjs';
const face=()=>({compare:'equal',pass:'keep',fail:'keep',depth_fail:'keep'});
const pipeline=()=>({depth_compare:'less_equal',depth_write:true,forced_depth:null,stencil:null,blend:'non_premultiplied'});
const frame=()=>({schema:2,width:64,height:48,generation:'1',meshes:[{vertices:[0,0,0,0,0,1,1,1,1,1,0,0,1,0,1,1,1,1,0,1,0,0,1,1,1,1,1],indices:[0,1,2]}],textures:[],draws:[{mesh:0,texture:null,matrix:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],pick_id:1,depth_equal:true,pipeline:pipeline()}]});
const stencil=()=>({reference:1,clockwise:face(),counterclockwise:face()});
test('v2 preserves bounded source object commands and both winding faces',()=>{
  const f=frame();f.draws[0].pipeline.stencil=stencil();
  assert.equal(validateWorldGpuFrame(f),f);
});
test('v1 cannot silently drop material pipeline commands',()=>{
  const f=frame();f.schema=1;assert.throws(()=>validateWorldGpuFrame(f),/pipeline|schema/);
});
test('v2 requires an explicit pipeline or null for every draw',()=>{
  const f=frame();delete f.draws[0].pipeline;assert.throws(()=>validateWorldGpuFrame(f));
  f.draws[0].pipeline=null;assert.equal(validateWorldGpuFrame(f),f);
});
test('source depth and blend state reject unsupported or nonfinite values',()=>{
  for(const patch of [{depth_compare:'greater'},{depth_write:1},{forced_depth:NaN},{forced_depth:2},{blend:'multiply'},{unknown:true}]){
    const f=frame();Object.assign(f.draws[0].pipeline,patch);assert.throws(()=>validateWorldGpuFrame(f));
  }
  const f=frame();f.draws[0].depth_equal=false;assert.throws(()=>validateWorldGpuFrame(f));
});
test('source stencil state requires complete independent bounded faces',()=>{
  for(const mutate of [s=>s.reference=256,s=>s.reference=.5,s=>delete s.clockwise,s=>s.clockwise.compare='never',s=>s.counterclockwise.pass='invert',s=>delete s.counterclockwise.depth_fail,s=>s.clockwise.surprise=1]){
    const f=frame(),s=stencil();mutate(s);f.draws[0].pipeline.stencil=s;assert.throws(()=>validateWorldGpuFrame(f));
  }
});
test('invisible mask commands cannot sample textures or write selection identities',()=>{
  const f=frame();Object.assign(f.draws[0].pipeline,{blend:'no_color',stencil:stencil()});
  assert.throws(()=>validateWorldGpuFrame(f));f.draws[0].pick_id=0;assert.equal(validateWorldGpuFrame(f),f);
  f.textures=[{width:1,height:1,pixels:[[255,255,255,255]]}];f.draws[0].texture=0;assert.throws(()=>validateWorldGpuFrame(f));
});
