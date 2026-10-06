// Real-device protocol test. This does not qualify an engine or browser page
// presentation; those remain the independent image gates in browser-gate.mjs.
import test from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';

test('actual WebGPU repeats mapped canvas copies and retires a lost device',async()=>{
  const {chromium}=await import(process.env.WONDERLAND_PLAYWRIGHT_MODULE||new URL('../../../tools/swarm-c/node_modules/playwright/index.mjs',import.meta.url).href);
  const source=await readFile(new URL('./gpu-readback.mjs',import.meta.url));
  const server=createServer((request,response)=>{
    response.writeHead(200,{'Content-Type':request.url==='/gpu-readback.mjs'?'text/javascript':'text/html'});
    response.end(request.url==='/gpu-readback.mjs'?source:'<!doctype html><canvas width="3" height="2"></canvas>');
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  let browser;
  try{
    browser=await chromium.launch({headless:true,channel:'chromium',
      ...(process.env.WONDERLAND_CHROMIUM?{executablePath:process.env.WONDERLAND_CHROMIUM}:{}),
      args:['--enable-features=Vulkan','--use-gl=angle','--use-angle=swiftshader','--use-vulkan=swiftshader',
        '--use-webgpu-adapter=swiftshader','--disable-vulkan-surface','--enable-unsafe-webgpu']});
    const page=await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    const result=await page.evaluate(async()=>{
      const {EngineCanvasReadback}=await import('./gpu-readback.mjs');
      const adapter=await navigator.gpu.requestAdapter();
      if(!adapter)throw new Error('This explicit GPU test requires a real WebGPU adapter');
      const device=await adapter.requestDevice(),canvas=document.querySelector('canvas'),context=canvas.getContext('webgpu');
      let scene=0;
      const errors=[],reader=new EngineCanvasReadback({stamp:()=>({scene})});
      device.addEventListener('uncapturederror',event=>errors.push(event.error.message));
      const configure=context.configure.bind(context);
      context.configure=configuration=>{configure(configuration);reader.configured(context,canvas,configuration);};
      context.configure({device,format:'bgra8unorm',alphaMode:'opaque'});
      const submit=device.queue.submit.bind(device.queue);
      const copies=[];
      for(scene=1;scene<=12;scene++){
        const pending=reader.read();
        const texture=context.getCurrentTexture();reader.acquired(context,texture);
        const encoder=device.createCommandEncoder();
        const pass=encoder.beginRenderPass({colorAttachments:[{view:texture.createView(),
          loadOp:'clear',storeOp:'store',clearValue:{r:scene/255,g:0,b:0,a:1}}]});
        pass.end();submit([encoder.finish()]);reader.submitted(device,submit);
        const raw=await pending;
        copies.push({scene,rgba:Array.from(atob(raw.base64),character=>character.charCodeAt(0)),stage:raw.readback.stage});
      }
      const pending=reader.read();reader.deviceLost(device,'test destruction');device.destroy();
      let cancellation;
      try{await pending;throw new Error('Lost read unexpectedly resolved');}catch(error){cancellation=String(error);}
      return {copies,errors,cancellation,snapshot:reader.snapshot(),adapter:{vendor:adapter.info?.vendor,architecture:adapter.info?.architecture}};
    });
    assert.equal(result.copies.length,12);
    for(const {scene,rgba,stage} of result.copies){
      assert.deepEqual(rgba,Array.from({length:6},()=>[scene,0,0,255]).flat());
      assert.equal(stage,'mapped');
    }
    assert.deepEqual(result.errors,[]);
    assert.match(result.cancellation,/device lost/);
    assert.equal(result.snapshot.pending,null);assert.equal(result.snapshot.configured,false);
    console.log(JSON.stringify({browser:browser.version(),copies:12,exactRgba:true,deviceCancellation:true,adapter:result.adapter,
      evidence:'Actual WebGPU protocol execution, independent of engine rendering and browser page presentation'}));
  }finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
});
