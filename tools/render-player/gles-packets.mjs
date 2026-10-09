// TEST ONLY: retain calls emitted by the production material/texture binders.
// The Python runner executes them on native Mesa GLES, not a browser context.
import {readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {validateWorldGpuFrame,applySourceMaterialState,applySourceTextureState} from '../../apps/web-shell/public/world-gpu.mjs';
const dir=resolve(process.argv[2]||'tests/output/source-gpu');
const source=await readFile(new URL('../../apps/web-shell/public/world-gpu.mjs',import.meta.url),'utf8');
const shader=name=>{const value=source.match(new RegExp('const '+name+'=`([\\s\\S]*?)`;'));if(!value)throw Error('Shader source missing');return value[1];};
const scenes=JSON.parse(await readFile(resolve(dir,'manifest.json'))),frames=[];
for(const scene of scenes){
 const frame=validateWorldGpuFrame(JSON.parse(await readFile(resolve(dir,scene.name+'.json'))));
 const calls=picking=>frame.draws.map(draw=>{
  const commands=[];
  const gl=new Proxy({}, {get(_,name){
   if(/^[A-Z][A-Z0-9_]*$/.test(name))return name;
   if(name==='getUniformLocation')return (_,uniform)=>uniform;
   return (...args)=>commands.push({name,args});
  }});
  applySourceTextureState(gl,{},draw);applySourceMaterialState(gl,{},draw,picking);return commands;
 });
 frames.push({scene,frame,colorCalls:calls(false),pickCalls:calls(true)});
}
await writeFile(resolve(dir,'gles-input.json'),JSON.stringify({scope:'Native Mesa GLES, not browser/WASM/physical-device acceptance',sourceSha256:createHash('sha256').update(source).digest('hex'),vertex:shader('vertexSource'),fragment:shader('fragmentSource'),frames}));
