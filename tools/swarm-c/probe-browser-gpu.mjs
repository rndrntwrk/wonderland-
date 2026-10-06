// Isolate Chromium software-adapter configuration from Wonderland/Bevy itself.
import {createServer} from 'node:http';
import {mkdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {readPng} from './read-png.mjs';
const {chromium}=await import(process.env.WONDERLAND_PLAYWRIGHT_MODULE||'playwright');
const output=resolve('tools/swarm-c/output/gpu-bootstrap');await mkdir(output,{recursive:true});
const profiles={
  current:['--enable-features=Vulkan','--use-gl=angle','--use-angle=swiftshader','--use-vulkan=swiftshader','--use-webgpu-adapter=swiftshader','--disable-vulkan-surface','--enable-unsafe-webgpu'],
  minimal:['--enable-unsafe-webgpu','--use-webgpu-adapter=swiftshader'],
  'vulkan-angle':['--enable-features=Vulkan','--use-gl=angle','--use-angle=vulkan','--use-vulkan=swiftshader','--use-webgpu-adapter=swiftshader','--disable-vulkan-surface','--enable-unsafe-webgpu'],
  'angle-swiftshader':['--enable-unsafe-webgpu','--use-webgpu-adapter=swiftshader','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader'],
  'angle-swiftshader-webgl':['--enable-unsafe-webgpu','--use-webgpu-adapter=swiftshader','--use-gl=angle','--use-angle=swiftshader-webgl','--enable-unsafe-swiftshader'],
};
const server=createServer((req,res)=>{res.writeHead(200,{'Content-Type':'text/html'});res.end('<!doctype html><html><body style="margin:0;background:rgb(153,68,47)"><canvas width="4" height="4" style="display:block"></canvas></body></html>');});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const reports=[];
try{
  for(const [name,args] of Object.entries(profiles)){
    const record={name,args,errors:[]};reports.push(record);let browser;
    try{
      browser=await chromium.launch({channel:'chromium',headless:true,args});record.browser=browser.version();
      const page=await browser.newPage({viewport:{width:128,height:128}});page.on('pageerror',error=>record.errors.push(String(error)));
      await page.goto(`http://127.0.0.1:${server.address().port}/`);
      record.gpu=await page.evaluate(async()=>{
        async function measure(){
          if(!navigator.gpu)return {available:false,reason:'navigator.gpu absent'};
          const adapter=await navigator.gpu.requestAdapter();if(!adapter)return {available:false,reason:'adapter null'};
          const device=await adapter.requestDevice(),canvas=document.querySelector('canvas'),context=canvas.getContext('webgpu');
          const format=navigator.gpu.getPreferredCanvasFormat();context.configure({device,format,alphaMode:'opaque',usage:GPUTextureUsage.RENDER_ATTACHMENT|GPUTextureUsage.COPY_SRC});
          const bytes=device.createBuffer({size:1024,usage:GPUBufferUsage.COPY_DST|GPUBufferUsage.MAP_READ});
          const draw=()=>{
            const texture=context.getCurrentTexture(),encoder=device.createCommandEncoder();
            const pass=encoder.beginRenderPass({colorAttachments:[{view:texture.createView(),clearValue:[.25,.5,.75,1],loadOp:'clear',storeOp:'store'}]});pass.end();
            encoder.copyTextureToBuffer({texture},{buffer:bytes,bytesPerRow:256},{width:4,height:4});device.queue.submit([encoder.finish()]);
          };
          draw();await bytes.mapAsync(GPUMapMode.READ);const mapped=Array.from(new Uint8Array(bytes.getMappedRange()).slice(0,4));bytes.unmap();
          if(format.startsWith('bgra'))[mapped[0],mapped[2]]=[mapped[2],mapped[0]];
          // A regular presented frame, not a CPU replacement, is kept current.
          function present(){const encoder=device.createCommandEncoder();const pass=encoder.beginRenderPass({colorAttachments:[{view:context.getCurrentTexture().createView(),clearValue:[.25,.5,.75,1],loadOp:'clear',storeOp:'store'}]});pass.end();device.queue.submit([encoder.finish()]);requestAnimationFrame(present);}
          requestAnimationFrame(present);
          const info=adapter.info||{};
          return {available:true,format,mapped,info:{vendor:info.vendor,architecture:info.architecture,device:info.device,description:info.description}};
        }
        return Promise.race([measure(),new Promise(resolve=>setTimeout(()=>resolve({available:false,reason:'GPU initialization/map exceeded 15 seconds'}),15000))]);
      });
      if(record.gpu.available){
        await page.waitForTimeout(500);
        const png=await page.locator('canvas').screenshot({path:resolve(output,name+'.png')});const image=readPng(png);
        record.pagePixel=Array.from(image.pixels.slice(0,4));
        record.correctMapped=record.gpu.mapped.every((value,i)=>Math.abs(value-[64,128,191,255][i])<=1);
        record.correctPresented=record.pagePixel.every((value,i)=>Math.abs(value-[64,128,191,255][i])<=1);
      }
    }catch(error){record.error=String(error.stack||error);}finally{await browser?.close();}
    console.log(JSON.stringify(record));
  }
}finally{await new Promise(resolve=>server.close(resolve));await writeFile(resolve(output,'profiles.json'),JSON.stringify(reports,null,2)+'\n');}
if(!reports.some(record=>record.correctMapped&&record.correctPresented))process.exitCode=1;
