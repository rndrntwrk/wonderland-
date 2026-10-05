// Actual software-browser engine evidence. No fake renderer can satisfy this runner.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile,mkdir,writeFile,stat} from 'node:fs/promises';
import {createReadStream} from 'node:fs';
import {dirname,resolve,extname,sep} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {deflateSync} from 'node:zlib';
import {readPng,readPpm,colorDifference,compareIds} from './read-png.mjs';

if(process.argv.includes('--self-test')){
  // Exercise all PNG row filters against hand-specified RGB pixels.
  const rawRows=[Buffer.from([255,0,0,0,255,0]),Buffer.from([0,0,255,255,255,255])];
  function chunk(type,data){const t=Buffer.from(type),size=Buffer.alloc(4),crc=Buffer.alloc(4);size.writeUInt32BE(data.length);let value=0xffffffff;for(const b of Buffer.concat([t,data])){value^=b;for(let i=0;i<8;i++)value=(value>>>1)^((value&1)?0xedb88320:0);}crc.writeUInt32BE((value^0xffffffff)>>>0);return Buffer.concat([size,t,data,crc]);}
  for(let filter=0;filter<5;filter++){
    const scan=[];
    for(let y=0;y<2;y++){
      const row=rawRows[y],prev=y?rawRows[y-1]:Buffer.alloc(6),encoded=Buffer.alloc(7);encoded[0]=filter;
      for(let x=0;x<6;x++){
        const a=x>=3?row[x-3]:0,b=prev[x],c=x>=3?prev[x-3]:0;let predictor=0;
        if(filter===1)predictor=a;else if(filter===2)predictor=b;else if(filter===3)predictor=Math.floor((a+b)/2);
        else if(filter===4){const p=a+b-c,pa=Math.abs(p-a),pb=Math.abs(p-b),pc=Math.abs(p-c);predictor=pa<=pb&&pa<=pc?a:pb<=pc?b:c;}
        encoded[x+1]=(row[x]-predictor)&255;
      }scan.push(encoded);
    }
    const header=Buffer.alloc(13);header.writeUInt32BE(2,0);header.writeUInt32BE(2,4);header[8]=8;header[9]=2;
    const png=Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',deflateSync(Buffer.concat(scan))),chunk('IEND',Buffer.alloc(0))]);
    assert.deepEqual(Array.from(readPng(png).pixels),[255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255]);
    assert.throws(()=>readPng(png.subarray(0,30)));
  }
  const ids=Buffer.alloc(640*480*8),depths=Buffer.alloc(640*480*4),pixels=new Uint8Array(640*480*4);
  for(let i=0;i<640*480;i++){depths.writeFloatLE(0.5,i*4);pixels[i*4+3]=255;}
  assert.equal(compareIds({width:640,height:480,pixels},ids,depths,[]).mismatched,0);
  pixels[(3*640+3)*4]=1;
  assert.equal(compareIds({width:640,height:480,pixels},ids,depths,[]).mismatched,1);
  console.log('PASS: five PNG filters, malformed PNG rejection, exact ownerless ID occlusion and mismatch detection');
  process.exit(0);
}

const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const engine=process.env.WONDERLAND_ENGINE,backend=process.env.WONDERLAND_BACKEND;
const variant=`${engine}-${backend}`;
if(!['bevy-webgpu','bevy-webgl2','fyrox-webgl2'].includes(variant))throw new Error('Set WONDERLAND_ENGINE and WONDERLAND_BACKEND to a browser variant');
const output=resolve(root,'tools/swarm-c/output',variant,'browser');
await mkdir(output,{recursive:true});
const report={schemaVersion:1,variant,startedAt:new Date().toISOString(),status:'running',rendererQualified:false,
  evidenceClass:'headless Chromium with explicitly requested software graphics',physicalDevices:'pending',
  gpuExecutionTiming:'unavailable; submission intervals are not GPU time',gpuAsyncPickReadback:'pending; screenshot visualization readback tested here',
  tolerances:{idStableInteriorMismatches:0,colorMeanAbsoluteByteError:4,colorRootMeanSquareByteError:12,colorChannelFractionOver8:0.03},
  checks:[],scenes:[],lifecycle:[],failures:[],console:[],requests:[]};
const failure=(name,error)=>{const item={name,error:String(error?.stack||error)};report.failures.push(item);console.error(`FAIL ${name}: ${item.error}`);};
function check(name,condition,detail){report.checks.push({name,passed:!!condition,detail});if(!condition)failure(name,JSON.stringify(detail));}
async function step(name,fn){try{const detail=await fn();report.lifecycle.push({name,passed:true,detail});return detail;}catch(error){failure(name,error);report.lifecycle.push({name,passed:false,error:String(error)});return null;}}
const types={'.html':'text/html','.js':'text/javascript','.mjs':'text/javascript','.css':'text/css','.wasm':'application/wasm','.json':'application/json'};
const server=createServer(async(req,res)=>{
  try{
    const name=decodeURIComponent(new URL(req.url,'http://localhost').pathname),path=resolve(root,`.${name}`);
    if(name==='/favicon.ico'){res.writeHead(204);res.end();return;}
    if(name==='/__webgpu_diagnostic.html'){res.writeHead(200,{'Content-Type':'text/html','Cache-Control':'no-store'});res.end('<!doctype html><meta charset="utf-8"><canvas width="4" height="4"></canvas>');return;}
    if(path!==root&&!path.startsWith(root+sep)){res.writeHead(403);res.end();return;}
    const file=(await stat(path)).isDirectory()?resolve(path,'index.html'):path;
    res.writeHead(200,{'Content-Type':types[extname(file)]||'application/octet-stream','Cache-Control':'no-store'});
    createReadStream(file).pipe(res);
  }catch{res.writeHead(404);res.end('Not found');}
});
await new Promise(resolveListen=>server.listen(0,'127.0.0.1',resolveListen));
const base=`http://127.0.0.1:${server.address().port}/probes/engine-bakeoff/web/index.html`;
let browser,browserServer;
const browserProcessLog=[];let browserLogBytes=0,forwardedProcessLines=0;
function recordBrowserProcess(chunk){
  const value=String(chunk);if(browserLogBytes<1024*1024){browserProcessLog.push(value.slice(0,1024*1024-browserLogBytes));browserLogBytes+=Buffer.byteLength(value);}
  for(const line of value.split('\n'))if(forwardedProcessLines<64&&/gpu|dawn|vulkan|device|validation/i.test(line)){
    forwardedProcessLines++;console.log('WONDERLAND_BROWSER_PROCESS '+line.slice(0,4000));
  }
}
async function snapshot(page){return page.evaluate(()=>window.__wonderlandProbe.snapshot());}
async function command(page,method,...args){
  const sequence=await page.evaluate(({method,args})=>window.__wonderlandProbe[method](...args),{method,args});
  if(Number.isInteger(sequence))await page.waitForFunction(seq=>window.__wonderlandProbe.snapshot().lastCommand>=seq,sequence,{timeout:20000});
  return snapshot(page);
}
async function renderedAfter(page,before){
  await page.waitForFunction(prior=>{const s=window.__wonderlandProbe.snapshot();return s.gpuSubmissions+s.glDrawCalls>prior;},before,{timeout:30000});
  // Uniform/material changes can be queued one render behind the main-world acknowledgement.
  await page.waitForTimeout(150);
}
async function diagnosticSnapshot(page){
  let timer;
  try{return await Promise.race([
    snapshot(page).catch(error=>({diagnosticError:String(error)})),
    new Promise(resolveTimeout=>{timer=setTimeout(()=>resolveTimeout({diagnosticError:'Snapshot timed out after 5 seconds'}),5000);})
  ]);}finally{clearTimeout(timer);}
}
async function captureCanvas(page,path,expected={width:640,height:480}){
  await page.locator('#engine-canvas').scrollIntoViewIfNeeded();
  const viewport=await page.evaluate(async()=>{
    await document.fonts.ready;
    return window.__wonderlandProbe.alignForCapture();
  });
  assert.equal(viewport.cssWidth,expected.width,'Canvas CSS width differs from the requested drawable size');
  assert.equal(viewport.cssHeight,expected.height,'Canvas CSS height differs from the requested drawable size');
  assert.ok(Math.abs(viewport.documentX-Math.round(viewport.documentX))<0.001,'Canvas document x must lie on an integer CSS pixel');
  assert.ok(Math.abs(viewport.documentY-Math.round(viewport.documentY))<0.001,'Canvas document y must lie on an integer CSS pixel');
  const bytes=await page.locator('#engine-canvas').screenshot({path,scale:'css',style:'#engine-canvas { outline: none !important; }'});
  const image=readPng(bytes);
  if(image.width!==expected.width||image.height!==expected.height){
    console.error('WONDERLAND_CAPTURE_DIMENSIONS '+JSON.stringify({viewport,png:{width:image.width,height:image.height},expected}));
    throw new Error('Canvas screenshot dimensions differ from the exact drawable size: '+image.width+'x'+image.height+' versus '+expected.width+'x'+expected.height);
  }
  return {bytes,image,viewport};
}

async function captureColor(page,path){
  const deadline=Date.now()+15000;
  let bytes,image,distinct;
  do{
    const capture=await captureCanvas(page,path);bytes=capture.bytes;image=capture.image;distinct=new Set();
    for(let i=0;i<image.pixels.length;i+=4)distinct.add(image.pixels[i]|image.pixels[i+1]<<8|image.pixels[i+2]<<16);
    if(distinct.size>16)break;
    await page.waitForTimeout(250);
  }while(Date.now()<deadline);
  return {bytes,image,distinctColors:distinct.size};
}
async function ready(page){
  try{
    await page.waitForFunction(()=>{const state=window.__wonderlandProbe?.snapshot();return state&&(state.ready||state.errors.length||state.lifecycle==='lost');},undefined,{timeout:60000});
  }catch(error){
    console.error('WONDERLAND_STARTUP_TIMEOUT '+JSON.stringify(await diagnosticSnapshot(page)));
    throw error;
  }
  const s=await snapshot(page);assert.equal(s.errors.length,0,s.errors.join('\n'));
  assert.notEqual(s.lifecycle,'lost','Engine graphics device was lost during startup: '+JSON.stringify(s.lossEvents));
  assert.equal(s.ready,true);
  assert.equal(s.actualBackend,backend);assert.equal(s.wasmMemoryShared,false);assert.equal(s.crossOriginIsolated,false);
  await renderedAfter(page,0);return snapshot(page);
}

async function diagnoseWebGpu(){
  // A separate API diagnostic after both engine runs. It cannot set the engine's
  // observed backend or satisfy any renderer/parity gate, or warm its cold start.
  const context=await browser.newContext(),page=await context.newPage();let timer;
  try{
    await page.goto(new URL('/__webgpu_diagnostic.html',base).href);
    return await Promise.race([
      page.evaluate(async()=>{
        if(!navigator.gpu)return {passed:false,error:'navigator.gpu unavailable'};
        let device,loss=null;const errors=[];
        try{
          const adapter=await navigator.gpu.requestAdapter({powerPreference:'high-performance'});
          if(!adapter)return {passed:false,error:'No WebGPU adapter'};
          device=await adapter.requestDevice();device.lost.then(info=>{loss={reason:info.reason,message:info.message};});
          device.addEventListener('uncapturederror',event=>errors.push(event.error.message));
          const canvas=document.querySelector('canvas'),gpu=canvas.getContext('webgpu');
          gpu.configure({device,format:'rgba8unorm',viewFormats:['rgba8unorm-srgb'],usage:GPUTextureUsage.RENDER_ATTACHMENT|GPUTextureUsage.COPY_SRC,alphaMode:'opaque'});
          const expected=[64,128,191,255],decode=value=>{const s=value/255;return s<=0.04045?s/12.92:((s+0.055)/1.055)**2.4;};
          let pixels=[];
          for(let frame=0;frame<3;frame++){
            const texture=gpu.getCurrentTexture(),buffer=device.createBuffer({size:1024,usage:GPUBufferUsage.COPY_DST|GPUBufferUsage.MAP_READ});
            const encoder=device.createCommandEncoder(),pass=encoder.beginRenderPass({colorAttachments:[{view:texture.createView({format:'rgba8unorm-srgb'}),clearValue:{r:decode(64),g:decode(128),b:decode(191),a:1},loadOp:'clear',storeOp:'store'}]});
            pass.end();encoder.copyTextureToBuffer({texture},{buffer,bytesPerRow:256},{width:4,height:4,depthOrArrayLayers:1});
            device.queue.submit([encoder.finish()]);await buffer.mapAsync(GPUMapMode.READ);pixels=Array.from(new Uint8Array(buffer.getMappedRange(),0,4));buffer.unmap();buffer.destroy();
            await new Promise(resolveFrame=>requestAnimationFrame(()=>setTimeout(resolveFrame,100)));
            if(loss)break;
          }
          return {passed:!loss&&!errors.length&&pixels.every((v,i)=>Math.abs(v-expected[i])<=1),pixels,expected,loss,errors,framesRequested:3,adapter:{vendor:adapter.info?.vendor,architecture:adapter.info?.architecture},evidence:'separate WebGPU clear and mapped pixel readback; no engine qualification'};
        }catch(error){return {passed:false,error:String(error.stack||error),loss,errors};}
        finally{device?.destroy();}
      }),
      new Promise(resolveTimeout=>{timer=setTimeout(()=>resolveTimeout({passed:false,error:'WebGPU diagnostic timed out after 20 seconds'}),20000);})
    ]);
  }finally{clearTimeout(timer);await context.close();}
}

try{
  const manifest=JSON.parse(await readFile(resolve(root,'tools/swarm-c/output/reference/manifest.json'),'utf8'));
  assert.equal(manifest.width,640);assert.equal(manifest.height,480);
  report.referenceEvidence=manifest.evidence;
  const {chromium}=await import('playwright');
  const args=backend==='webgl2'?['--use-gl=angle','--use-angle=swiftshader-webgl','--enable-unsafe-swiftshader']:
    ['--enable-unsafe-webgpu','--use-webgpu-adapter=swiftshader'];
  report.browserArgs=args;
  browserServer=await chromium.launchServer({channel:'chromium',headless:true,args,host:'127.0.0.1'});
  browserServer.process().stderr?.on('data',recordBrowserProcess);
  browser=await chromium.connect(browserServer.wsEndpoint());report.browserVersion=browser.version();
  for(const avatars of [32,64]){
    const dpr=avatars===32?1:2;
    const context=await browser.newContext({viewport:{width:1400,height:1100},deviceScaleFactor:dpr});
    await context.tracing.start({screenshots:true,snapshots:true,sources:true});
    const page=await context.newPage();page.setDefaultTimeout(20000);
    let forwardedConsole=0;
    page.on('console',message=>{
      const item={avatars,type:message.type(),text:message.text()};report.console.push(item);
      if(forwardedConsole<64&&(item.type==='error'||item.text.startsWith('WONDERLAND_BOOTSTRAP '))){forwardedConsole++;console.log('WONDERLAND_BROWSER_CONSOLE '+JSON.stringify({...item,text:item.text.slice(0,4000)}));}
    });
    page.on('pageerror',error=>report.console.push({avatars,type:'pageerror',text:String(error.stack||error)}));
    page.on('requestfailed',request=>report.requests.push({avatars,url:request.url(),error:request.failure()}));
    page.on('response',response=>{if(response.status()>=400){const item={avatars,url:response.url(),status:response.status()};report.requests.push(item);console.log('WONDERLAND_HTTP_ERROR '+JSON.stringify(item));}});
    try{
      const url=`${base}?variant=${variant}&mode=hybrid2d&avatars=${avatars}&tick=30`;
      await page.goto(url,{waitUntil:'domcontentloaded'});const initial=await ready(page);
      check(`startup-${avatars}`,initial.actualBackend===backend&&initial.wasmMemoryShared===false&&initial.crossOriginIsolated===false,initial);
      await page.evaluate(()=>window.__wonderlandProbe.resize(642,482));
      await page.waitForTimeout(250);
      const box=await page.locator('#engine-canvas').boundingBox();assert.equal(Math.round(box.width),640);assert.equal(Math.round(box.height),480);
      for(const mode of ['full2d','hybrid2d','full3d']){
        const expected=manifest.scenes.find(s=>s.mode===mode&&s.avatars===avatars);assert.ok(expected);
        const prior=await snapshot(page);await command(page,'setMode',mode);await command(page,'setPass','color');
        await renderedAfter(page,prior.gpuSubmissions+prior.glDrawCalls);
        const state=await snapshot(page);check(`fixture-hash-${mode}-${avatars}`,state.sceneHash===expected.fixtureHash,{expected:expected.fixtureHash,actual:state.sceneHash});
        const prefix=`${mode}-${avatars}-dpr${dpr}`;
        const colorPath=resolve(output,`${prefix}-color.png`),pickPath=resolve(output,`${prefix}-pick.png`);
        const capture=await captureColor(page,colorPath),colorBytes=capture.bytes,gpuColor=capture.image;
        const cpuColor=readPpm(await readFile(resolve(root,`tools/swarm-c/output/reference/${mode}-${avatars}.ppm`)));
        const color=colorDifference(gpuColor,cpuColor);
        check(`nonempty-render-${mode}-${avatars}`,capture.distinctColors>16,{distinctColors:capture.distinctColors});
        check(`color-parity-${mode}-${avatars}`,color.meanAbsoluteByteError<=4&&color.rootMeanSquareByteError<=12&&color.channelFractionOver8<=0.03,color);
        await command(page,'setPass','pick');await renderedAfter(page,state.gpuSubmissions+state.glDrawCalls);
        const pickState=await snapshot(page),pickCapture=await captureCanvas(page,pickPath),pickBytes=pickCapture.bytes;
        const ids=await readFile(resolve(root,`tools/swarm-c/output/reference/${mode}-${avatars}.ids`));
        const depths=await readFile(resolve(root,`tools/swarm-c/output/reference/${mode}-${avatars}.depth`));
        const idComparison=compareIds(readPng(pickBytes),ids,depths,pickState.idMap);
        check(`gpu-id-parity-${mode}-${avatars}`,idComparison.checked>100&&idComparison.ownerlessOcclusionChecked>0&&idComparison.mismatched===0,idComparison);
        const scene={mode,avatars,dpr,state:pickState,color,idComparison,colorScreenshot:`${prefix}-color.png`,idScreenshot:`${prefix}-pick.png`,
          artifactSha256:{color:createHash('sha256').update(colorBytes).digest('hex'),pick:createHash('sha256').update(pickBytes).digest('hex')}};
        report.scenes.push(scene);
        const target=idComparison.groups.find(g=>g.identity!=='0:0'&&g.points.length);
        if(target){
          const point=target.points[0],selected=await command(page,'selectAt',point.x,point.y),[object_id,generation]=target.identity.split(':').map(Number);
          check(`stable-selection-${mode}-${avatars}`,selected.selection?.object_id===object_id&&selected.selection?.generation===generation,{expected:{object_id,generation},actual:selected.selection,source:selected.selectionSource});
        }else check(`stable-selection-${mode}-${avatars}`,false,{reason:'CPU reference has no selectable stable interior'});
        await command(page,'setPass','color');
      }

      await step(`DOM-focus-${avatars}`,async()=>{
        const before=await snapshot(page);
        for(const selector of ['#login-name','#chat-input','#search-input','#interaction-input']){
          await page.locator(selector).click();await page.locator(selector).pressSequentially('123');assert.equal(await page.locator(selector).inputValue(),'123');
          assert.equal((await snapshot(page)).mode,before.mode);
        }
        await page.locator('#engine-canvas').focus();await page.keyboard.press('1');
        await page.waitForFunction(()=>window.__wonderlandProbe.snapshot().mode==='full2d');
        return {typedInputs:4,canvasKeyChangedView:true};
      });
      await step(`pointer-cancellation-${avatars}`,async()=>{
        const before=await snapshot(page),r=await page.locator('#engine-canvas').boundingBox();
        await page.mouse.move(r.x+40,r.y+40);await page.mouse.down();
        await page.locator('#engine-canvas').dispatchEvent('pointercancel',{pointerId:1,pointerType:'mouse',bubbles:true});await page.mouse.up();
        const after=await snapshot(page);assert.deepEqual(after.selection,before.selection);assert.ok(after.pointerCancelCount>0);return after.pointerCancelCount;
      });
      await step(`resize-and-DPR-${avatars}`,async()=>{
        const before=await snapshot(page);await page.evaluate(()=>window.__wonderlandProbe.resize(402,302));await page.waitForTimeout(250);
        const after=await snapshot(page);assert.equal(after.sceneHash,before.sceneHash);assert.equal(Math.round(after.viewport.cssWidth),400);assert.equal(Math.round(after.viewport.cssHeight),300);
        assert.equal(after.devicePixelRatio,dpr);
        assert.ok(Math.abs(after.viewport.width-after.viewport.cssWidth*dpr)<=1,'Canvas backing width must track CSS width times actual DPR');
        assert.ok(Math.abs(after.viewport.height-after.viewport.cssHeight*dpr)<=1,'Canvas backing height must track CSS height times actual DPR');
        await captureCanvas(page,resolve(output,`resize-${avatars}-dpr${dpr}.png`),{width:400,height:300});
        await page.evaluate(()=>window.__wonderlandProbe.resize(642,482));return {before:before.viewport,after:after.viewport,dpr};
      });
      await step(`presentation-suspend-${avatars}`,async()=>{
        await command(page,'suspend');const before=await snapshot(page);await page.waitForTimeout(250);const paused=await snapshot(page);
        assert.equal(paused.updateCount,before.updateCount);assert.equal(paused.tick,before.tick);await command(page,'resume');
        await page.waitForFunction(n=>window.__wonderlandProbe.snapshot().updateCount>n,paused.updateCount);return {pausedUpdates:paused.updateCount};
      });
      await step(`audio-gesture-and-lifecycle-${avatars}`,async()=>{
        const locked=await page.evaluate(()=>window.__wonderlandProbe.audioSnapshot());assert.equal(locked.state,'locked');
        await page.evaluate(()=>window.__wonderlandProbe.startAudio());
        const queued=await page.evaluate(()=>window.__wonderlandProbe.audioSnapshot());assert.equal(queued.state,'locked');assert.equal(queued.activeVoices,1);
        await page.locator('#unlock-audio').click();await page.waitForFunction(()=>window.__wonderlandProbe.audioSnapshot().state==='running');
        await page.waitForFunction(()=>window.__wonderlandProbe.audioSnapshot().pendingDecodes===0&&window.__wonderlandProbe.audioSnapshot().pcmBytes>0);
        const playing=await page.evaluate(()=>window.__wonderlandProbe.audioSnapshot());assert.equal(playing.errors,0);
        await page.locator('#suspend').click();await page.waitForFunction(()=>window.__wonderlandProbe.audioSnapshot().state==='suspended');
        await page.locator('#resume').click();await page.waitForFunction(()=>window.__wonderlandProbe.audioSnapshot().state==='running');
        await page.evaluate(()=>window.__wonderlandProbe.stopAudio());assert.equal((await page.evaluate(()=>window.__wonderlandProbe.audioSnapshot())).activeVoices,0);
        // Queue a real provider completion, then reset the lot before accepting its playback.
        await page.evaluate(()=>{void window.__wonderlandProbe.startAudio();window.__wonderlandProbe.reloadFixture();});
        await page.waitForTimeout(200);const reset=await page.evaluate(()=>window.__wonderlandProbe.audioSnapshot());assert.equal(reset.activeVoices,0);assert.equal(reset.pendingDecodes,0);
        return {locked,queued,playing,reset,audibility:'AudioContext and source graph observed; speaker output not measured'};
      });
      await step(`repeat-resource-reset-${avatars}`,async()=>{
        const before=await snapshot(page);
        for(let i=0;i<8;i++)await command(page,'reloadFixture');await page.waitForTimeout(500);const after=await snapshot(page);
        assert.equal(after.sceneHash,before.sceneHash);assert.equal(after.meshCount,before.meshCount);assert.equal(after.spriteCount,before.spriteCount);
        if(before.engineResourceOwnership&&after.engineResourceOwnership)assert.deepEqual(after.engineResourceOwnership,before.engineResourceOwnership,'Owned engine resources changed after identical resets');
        if(before.memoryBytes!==null&&after.memoryBytes!==null)assert.ok(after.memoryBytes-before.memoryBytes<=256*1024*1024,'WASM memory grew more than 256 MiB over eight identical resets');
        return {resets:8,beforeMemory:before.memoryBytes,afterMemory:after.memoryBytes,beforeOwnership:before.engineResourceOwnership??null,afterOwnership:after.engineResourceOwnership??null};
      });
      await step(`simulated-loss-${avatars}`,async()=>{
        const before=await snapshot(page);await command(page,'simulateLoss');const after=await snapshot(page);
        assert.equal(after.simulatedLossCount,before.simulatedLossCount+1);assert.equal(after.actualLossCount,before.actualLossCount);assert.equal(after.suspended,true);await command(page,'resume');return after.simulatedLossCount;
      });
      const beforeLoss=await snapshot(page);check(`no-runtime-errors-before-loss-${avatars}`,beforeLoss.errors.length===0,beforeLoss.errors);
      const consoleErrors=report.console.filter(item=>item.avatars===avatars&&['error','pageerror'].includes(item.type));
      check(`no-console-errors-before-loss-${avatars}`,consoleErrors.length===0,consoleErrors);
      await step(`actual-device-loss-and-recovery-${avatars}`,async()=>{
        await page.evaluate(backend=>window.__wonderlandProbe[backend==='webgpu'?'loseDevice':'loseContext'](),backend);
        await page.waitForFunction(()=>window.__wonderlandProbe.snapshot().actualLossCount>0);const lost=await snapshot(page);assert.equal(lost.ready,false);assert.equal(lost.lifecycle,'lost');
        await Promise.all([page.waitForNavigation({waitUntil:'domcontentloaded'}),page.evaluate(()=>window.__wonderlandProbe.recover())]);
        const recovered=await ready(page);const expected=manifest.scenes.find(s=>s.mode==='hybrid2d'&&s.avatars===avatars);
        assert.equal(recovered.sceneHash,expected.fixtureHash);return {lost,recovered};
      });
    }catch(error){failure(`browser-context-${avatars}`,error);try{await page.screenshot({path:resolve(output,`failure-${avatars}.png`),fullPage:true});report.lifecycle.push({name:`failure-state-${avatars}`,state:await diagnosticSnapshot(page)});}catch{} }
    finally{await context.tracing.stop({path:resolve(output,`trace-${avatars}.zip`)});await context.close();}
  }
  if(backend==='webgpu'){
    report.webgpuDiagnostic=await diagnoseWebGpu();
    console.log('WONDERLAND_WEBGPU_DIAGNOSTIC '+JSON.stringify(report.webgpuDiagnostic));
  }
}catch(error){failure('runner',error);}
finally{
  if(browser)await browser.close();if(browserServer)await browserServer.close();await new Promise(resolveClose=>server.close(resolveClose));
  report.finishedAt=new Date().toISOString();report.status=report.failures.length?'failed':'software-browser-checks-passed';
  await writeFile(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');
  await writeFile(resolve(output,'console.json'),JSON.stringify(report.console,null,2)+'\n');
  await writeFile(resolve(output,'browser-process.log'),browserProcessLog.join(''));
  console.log(JSON.stringify({variant,status:report.status,scenes:report.scenes.length,failures:report.failures.length,rendererQualified:false,report:resolve(output,'report.json')}));
  process.exitCode=report.failures.length?1:0;
}
