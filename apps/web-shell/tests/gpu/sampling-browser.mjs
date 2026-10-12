// Actual production WebGL2 upload/draw/pick owner. Authored sampler fixtures, not playable content.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {resolve} from 'node:path';
const root=fileURLToPath(new URL('../../../../',import.meta.url));
const {chromium}=await import(process.env.WONDERLAND_PLAYWRIGHT_MODULE||'playwright');
const source=await readFile(resolve(root,'apps/web-shell/public/world-gpu.mjs'));
const rustCases=JSON.parse(await readFile(resolve(root,'tests/output/source-gpu/sampler-mips.json'),'utf8'));
const server=createServer((req,res)=>{res.setHeader('Content-Type',req.url==='/gpu.mjs'?'text/javascript':'text/html');res.end(req.url==='/gpu.mjs'?source:'<!doctype html><canvas id="world"></canvas>');});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({...(process.env.WONDERLAND_CHROMIUM_EXECUTABLE?{executablePath:process.env.WONDERLAND_CHROMIUM_EXECUTABLE}:{channel:'chromium'}),headless:true,args:['--use-gl=angle','--use-angle=swiftshader-webgl','--enable-unsafe-swiftshader']});
try{
 const page=await browser.newPage();await page.goto(`http://127.0.0.1:${server.address().port}/`);
 const result=await page.evaluate(async rustCases=>{
  const api=await import('/gpu.mjs'),canvas=document.querySelector('canvas');
  const identity=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
  const vertices=[-1,-1,.5,0,0,1,1,1,1, 1,-1,.5,1,0,1,1,1,1, 1,1,.5,1,1,1,1,1,1, -1,1,.5,0,1,1,1,1,1];
  const frame={texture_mips:[[{width:1,height:1,pixels:[[127,127,127,127]]}]],schema:5,width:64,height:64,generation:'1',meshes:[{vertices,indices:[0,1,2,0,2,3]}],textures:[{width:2,height:1,pixels:[[0,0,0,255],[255,255,255,255]]}],draws:[{mesh:0,texture:0,matrix:identity,pick_id:7,depth_equal:false,pipeline:null,texture_address:'clamp',texture_filter:'linear_mipmap'}]};
  const observe=async f=>{
   api.paintSourceWorld(canvas,JSON.stringify(f));const gl=canvas.getContext('webgl2');const color=new Uint8Array(4);gl.readPixels(Math.floor(f.width/2),Math.floor(f.height/2),1,1,gl.RGBA,gl.UNSIGNED_BYTE,color);
   const min=gl.getTexParameter(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER),mag=gl.getTexParameter(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER);
   const pick=JSON.parse(await api.pickSourceWorld(canvas,Math.floor(f.width/2),Math.floor(f.height/2)));return {color:[...color],min,mag,pick:pick.index,error:gl.getError()};
  };
  const linear=await observe(frame);
  frame.generation='2';frame.draws[0].texture_filter='nearest';const nearest=await observe(frame);
  frame.schema=4;frame.generation='3';delete frame.draws[0].texture_filter;delete frame.texture_mips;const legacy=await observe(frame);
  frame.schema=5;frame.generation='4';frame.draws[0].texture_filter='linear_mipmap';frame.width=8;frame.height=8;
  frame.textures=[{width:64,height:64,pixels:Array.from({length:4096},(_,i)=>{const c=((i%64+Math.floor(i/64))%2)*255;return [c,c,c,255];})}];
  frame.texture_mips=[[]];for(let n=32;n>=1;n/=2)frame.texture_mips[0].push({width:n,height:n,pixels:Array.from({length:n*n},()=>[127,127,127,255])});
  const minified=await observe(frame);
  frame.generation='5';frame.width=64;frame.height=64;frame.textures=[{width:3,height:5,pixels:Array.from({length:15},()=>[70,90,110,255])}];
  frame.texture_mips=[[{width:1,height:2,pixels:[[70,90,110,255],[70,90,110,255]]},{width:1,height:1,pixels:[[70,90,110,127]]}]];
  const nonPowerOfTwo=await observe(frame);
  frame.generation='6';frame.width=1;frame.height=1;
  frame.textures=[{width:64,height:64,pixels:Array.from({length:4096},(_,i)=>i===0?[255,0,0,255]:[0,0,0,0])}];
  frame.texture_mips=[[]];for(let n=32;n>=1;n/=2)frame.texture_mips[0].push({width:n,height:n,pixels:Array.from({length:n*n},(_,i)=>i===0?[255,0,0,255]:[0,0,0,0])});
  const cutout=await observe(frame);
  const sourceResults=[];
  for(const entry of rustCases){
   frame.generation=String(Number(frame.generation)+1);frame.textures=[entry.image];frame.texture_mips=[entry.mips];
   sourceResults.push({name:entry.name,...await observe(frame)});
  }
  const refusals=[];
  for(const kind of ['missing','unknown','legacy','unqualified-wrap','missing-mips','wrong-mip-size']){
   const bad=structuredClone(frame);if(kind==='missing')delete bad.draws[0].texture_filter;
   if(kind==='unknown')bad.draws[0].texture_filter='anisotropic';
   if(kind==='legacy')bad.schema=4;
   if(kind==='unqualified-wrap')bad.draws[0].texture_address='wrap';
   if(kind==='missing-mips')bad.texture_mips=[[]];
   if(kind==='wrong-mip-size')bad.texture_mips[0][0].width=2;
   try{api.paintSourceWorld(canvas,JSON.stringify(bad));refusals.push(false);}catch{refusals.push(true);}
  }
  frame.generation='12';frame.width=64;frame.height=64;frame.textures=[{width:2,height:1,pixels:[[0,0,0,255],[255,255,255,255]]}];frame.texture_mips=[[{width:1,height:1,pixels:[[127,127,127,127]]}]];
  const left=structuredClone(frame.draws[0]);left.matrix[0]=.5;left.matrix[12]=-.5;
  const right=structuredClone(left);right.matrix[12]=.5;right.texture_filter='nearest';right.pick_id=8;
  frame.draws=[left,right];api.paintSourceWorld(canvas,JSON.stringify(frame));
  const gl=canvas.getContext('webgl2'),a=new Uint8Array(4),b=new Uint8Array(4);
  gl.readPixels(16,32,1,1,gl.RGBA,gl.UNSIGNED_BYTE,a);gl.readPixels(48,32,1,1,gl.RGBA,gl.UNSIGNED_BYTE,b);
  const shared={left:[...a],right:[...b],leftPick:JSON.parse(await api.pickSourceWorld(canvas,16,32)).index,rightPick:JSON.parse(await api.pickSourceWorld(canvas,48,32)).index};
  api.disposeSourceWorld(canvas);
  return {linear,nearest,legacy,minified,nonPowerOfTwo,cutout,sourceResults,shared,refusals,constants:{linear:gl.LINEAR,mipmap:gl.LINEAR_MIPMAP_LINEAR,nearest:gl.NEAREST},renderer:gl.getParameter(gl.RENDERER)};
 },rustCases);
 assert.ok(result.linear.color[0]>120&&result.linear.color[0]<140,JSON.stringify(result));
 assert.equal(result.linear.min,result.constants.mipmap);assert.equal(result.linear.mag,result.constants.linear);
 for(const r of [result.nearest,result.legacy]){assert.equal(r.color[0],255);assert.equal(r.min,result.constants.nearest);}
 assert.ok(result.minified.color[0]===127,JSON.stringify(result.minified));
 assert.deepEqual(result.cutout.color,[255,0,0,255]);assert.equal(result.cutout.pick,7);
 assert.deepEqual(result.nonPowerOfTwo.color,[70,90,110,255]);assert.deepEqual(result.refusals,[true,true,true,true,true,true]);
 assert.deepEqual(result.sourceResults[0].color,[255,0,0,255]);assert.equal(result.sourceResults[0].pick,7);
 assert.deepEqual(result.sourceResults.map(row=>row.color),[[255,0,0,255],[235,177,167,255],[32,0,0,255],[132,118,111,255],[10,0,0,255],[175,177,167,255]]);
 for(const row of result.sourceResults){assert.equal(row.pick,7);assert.equal(row.error,0);}
 assert.ok(result.shared.left[0]>120&&result.shared.left[0]<145);assert.equal(result.shared.right[0],255);
 assert.equal(result.shared.leftPick,7);assert.equal(result.shared.rightPick,8);
 for(const r of [result.linear,result.nearest,result.legacy,result.minified,result.nonPowerOfTwo]){assert.equal(r.pick,7);assert.equal(r.error,0);}
 const dir=resolve(root,'tests/output/source-gpu');await mkdir(dir,{recursive:true});await writeFile(resolve(dir,'sampling-browser.json'),JSON.stringify({passed:true,scope:'Actual Chromium WebGL2 production renderer with declared synthetic sampler geometry; software GPU, not device or live-content qualification',...result},null,2));console.log(JSON.stringify({passed:true,...result}));
}finally{await browser.close();server.close();}
