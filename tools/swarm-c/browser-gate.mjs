// Actual software-browser engine evidence. No fake renderer can satisfy this runner.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile,mkdir,writeFile,stat} from 'node:fs/promises';
import {createReadStream,writeFileSync} from 'node:fs';
import {dirname,resolve,extname,sep} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {deflateSync} from 'node:zlib';
import {readPng,readPpm,colorDifference,compareIds,capturePixelBounds,extractPixels,rgbaPng,logicalSelectionPoint} from './read-png.mjs';

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
  for(let y=0;y<480;y++)for(let x=0;x<640;x++){
    const i=y*640+x;depths.writeFloatLE(x<320?0.5:Infinity,i*4);pixels[i*4]=x<320?0:99;
  }
  const background=compareIds({width:640,height:480,pixels},ids,depths,[]);
  assert.ok(background.ownerlessOcclusionChecked>0&&background.mismatched>0,'Corrupt background IDs must fail even while ownerless geometry passes');
  for(const map of [[{objectId:1,generation:1,index:0}],[{objectId:1,generation:0,index:1}],
    [{objectId:1,generation:1,index:1},{objectId:2,generation:1,index:1}],
    [{objectId:1,generation:1,index:1},{objectId:1,generation:1,index:2}],
    [{objectId:1,generation:1,index:0x1000000}]])assert.throws(()=>compareIds({width:640,height:480,pixels},ids,depths,map));
  assert.throws(()=>compareIds({width:640,height:480,pixels:pixels.subarray(4)},ids,depths,[]));
  assert.throws(()=>compareIds({width:640,height:480,pixels},ids.subarray(8),depths,[]));
  assert.throws(()=>compareIds({width:640,height:480,pixels},ids,depths.subarray(4),[]));
  for(const [width,height] of [[640,480],[1280,960]]){
    const physicalIds=Buffer.alloc(width*height*8),physicalDepths=Buffer.alloc(width*height*4),physicalPixels=new Uint8Array(width*height*4);
    for(let i=0;i<width*height;i++){
      physicalIds.writeUInt32LE(77,i*8);physicalIds.writeUInt32LE(9,i*8+4);physicalDepths.writeFloatLE(0.5,i*4);
      physicalPixels.set([37,0,0,255],i*4);
    }
    const gpu={width,height,pixels:physicalPixels},map=[{objectId:77,generation:9,index:37}];
    assert.equal(compareIds(gpu,physicalIds,physicalDepths,map,{width,height}).mismatched,0);
    assert.throws(()=>compareIds(gpu,physicalIds,physicalDepths,[],{width,height}));
    physicalPixels[(3*width+3)*4]=18;
    assert.equal(compareIds(gpu,physicalIds,physicalDepths,map,{width,height}).mismatched,1,'Averaged ID bytes must never pass');
    if(width===640)assert.deepEqual(logicalSelectionPoint(physicalIds),{x:3,y:3,object_id:77,generation:9});
    else assert.throws(()=>compareIds(gpu,physicalIds,physicalDepths,map,{width:640,height:480}));
  }
  for(const dpr of [1,2]){
    for(const expected of [{width:640,height:480},{width:400,height:300}]){
      const measurement={dpr,documentWidth:1400,documentHeight:1100,viewport:{documentX:165,documentY:325,cssWidth:expected.width,cssHeight:expected.height,width:expected.width*dpr,height:expected.height*dpr}};
      for(const scale of ['css','device']){
        const factor=scale==='css'?1:dpr,result=capturePixelBounds(measurement,scale,expected);
        assert.deepEqual(result.bounds,{x:165*factor,y:325*factor,width:expected.width*factor,height:expected.height*factor});
        assert.equal(result.fullWidth,1400*factor);assert.equal(result.fullHeight,1100*factor);
      }
      assert.throws(()=>capturePixelBounds({...measurement,viewport:{...measurement.viewport,width:1}},'device',expected));
      assert.throws(()=>capturePixelBounds({...measurement,viewport:{...measurement.viewport,documentX:165.5}},'device',expected));
      assert.throws(()=>capturePixelBounds({...measurement,documentHeight:100},'css',expected));
    }
    const width=11*dpr,height=9*dpr,pagePixels=new Uint8Array(width*height*4);
    for(let y=0;y<height;y++)for(let x=0;x<width;x++)pagePixels.set([x*3,y*7,x+y,(x*y)%256],(y*width+x)*4);
    const pageImage={width,height,pixels:pagePixels},bounds={x:3*dpr,y:2*dpr,width:5*dpr,height:4*dpr};
    const cropped=extractPixels(readPng(rgbaPng(pageImage)),bounds),roundtrip=readPng(rgbaPng(cropped));
    for(let y=0;y<bounds.height;y++)for(let x=0;x<bounds.width;x++){
      const px=x+bounds.x,py=y+bounds.y;
      assert.deepEqual(Array.from(roundtrip.pixels.subarray((y*bounds.width+x)*4,(y*bounds.width+x+1)*4)),[px*3,py*7,px+py,(px*py)%256]);
    }
    for(const invalid of [{...bounds,x:0.5},{...bounds,y:-1},{...bounds,width:width+1},{...bounds,height:0}])assert.throws(()=>extractPixels(pageImage,invalid));
    assert.throws(()=>extractPixels({...pageImage,pixels:pagePixels.subarray(4)},bounds));
  }
  console.log('PASS: five PNG filters; exact nonzero-origin RGBA crops at DPR1/2; physical ID grids; corrupt background/ownerless IDs; dimensions and injective ID maps; independent logical selection');
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
async function step(name,fn){enterPhase(name);try{const detail=await fn();report.lifecycle.push({name,passed:true,detail});return detail;}catch(error){failure(name,error);report.lifecycle.push({name,passed:false,error:String(error)});return null;}}
const types={'.html':'text/html','.js':'text/javascript','.mjs':'text/javascript','.css':'text/css','.wasm':'application/wasm','.json':'application/json'};
const server=createServer(async(req,res)=>{
  try{
    const name=decodeURIComponent(new URL(req.url,'http://localhost').pathname),path=resolve(root,`.${name}`);
    if(name==='/favicon.ico'){res.writeHead(204);res.end();return;}
    if(name==='/__pixel_geometry.html'){res.writeHead(200,{'Content-Type':'text/html','Cache-Control':'no-store'});res.end('<!doctype html><meta charset="utf-8"><canvas style="width:100px;height:80px"></canvas>');return;}
    if(name==='/__webgpu_diagnostic.html'){res.writeHead(200,{'Content-Type':'text/html','Cache-Control':'no-store'});res.end('<!doctype html><meta charset="utf-8"><style>body{margin:0;background:#99442f}canvas{position:absolute;left:37px;top:61px;width:4px;height:4px}</style><canvas width="4" height="4"></canvas>');return;}
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
let phaseWatchdog;
function enterPhase(name){
  clearTimeout(phaseWatchdog);report.activePhase={name,startedAt:new Date().toISOString()};
  console.log('WONDERLAND_PHASE '+JSON.stringify(report.activePhase));
  // Page evaluation and graphics teardown can outlive Playwright's action
  // timeout. Preserve evidence without requiring the blocked page to respond.
  phaseWatchdog=setTimeout(()=>{
    failure('phase-watchdog',`No phase completion within 8 minutes: ${name}`);
    report.finishedAt=new Date().toISOString();report.status='failed';
    report.watchdog={phase:report.activePhase,trace:'A blocked browser may not finalize its trace; completed PNGs and diagnostics are retained'};
    try{
      writeFileSync(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');
      writeFileSync(resolve(output,'console.json'),JSON.stringify(report.console,null,2)+'\n');
      writeFileSync(resolve(output,'browser-process.log'),browserProcessLog.join(''));
    }catch(error){console.error('WONDERLAND_WATCHDOG_WRITE_ERROR '+String(error));}
    browserServer?.process().kill('SIGKILL');process.exit(1);
  },8*60*1000);
  phaseWatchdog.unref();
}
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
async function captureCanvas(page,path,scale='css',expected={width:640,height:480}){
  enterPhase(`capture-${path.split(sep).at(-1)}`);
  await page.locator('#engine-canvas').scrollIntoViewIfNeeded();
  await page.evaluate(async()=>{
    await document.fonts.ready;
    window.__wonderlandProbe.alignForCapture();
  });
  const measure=()=>page.evaluate(()=>{
    const e=document.documentElement,b=document.body;
    return {viewport:window.__wonderlandProbe.snapshot().viewport,dpr:devicePixelRatio,scrollX,scrollY,
      documentWidth:Math.max(e.scrollWidth,e.offsetWidth,e.clientWidth,b.scrollWidth,b.offsetWidth),
      documentHeight:Math.max(e.scrollHeight,e.offsetHeight,e.clientHeight,b.scrollHeight,b.offsetHeight)};
  });
  const before=await measure(),viewport=before.viewport;
  const {bounds,factor,fullWidth,fullHeight}=capturePixelBounds(before,scale,expected);
  const fullPath=path.replace(/\.png$/,'-page.png');assert.notEqual(fullPath,path);
  // Retain the complete document and verify its exact document-space canvas crop.
  // Correct crop geometry does not certify the browser's composited canvas pixels.
  const whole=readPng(await page.screenshot({path:fullPath,fullPage:true,scale,style:'#engine-canvas { outline: none !important; }'}));
  assert.deepEqual(await measure(),before,'Canvas/document geometry changed during screenshot');
  assert.equal(whole.width,fullWidth,'Full-page screenshot width differs from measured document');
  assert.equal(whole.height,fullHeight,'Full-page screenshot height differs from measured document');
  const image=extractPixels(whole,bounds),bytes=rgbaPng(image);await writeFile(path,bytes);
  return {bytes,image,viewport,capture:{scale,factor,bounds,document:before,fullPage:fullPath,fullWidth:whole.width,fullHeight:whole.height}};
}

async function captureColor(page,path){
  const deadline=Date.now()+15000;
  let bytes,image,distinct,metadata;
  do{
    const capture=await captureCanvas(page,path);bytes=capture.bytes;image=capture.image;metadata=capture.capture;distinct=new Set();
    for(let i=0;i<image.pixels.length;i+=4)distinct.add(image.pixels[i]|image.pixels[i+1]<<8|image.pixels[i+2]<<16);
    if(distinct.size>16)break;
    await page.waitForTimeout(250);
  }while(Date.now()<deadline);
  return {bytes,image,capture:metadata,distinctColors:distinct.size};
}
async function directCanvasSnapshot(page,path){
  try{
    const url=await page.evaluate(()=>window.__wonderlandProbe.canvasSnapshot());
    assert.ok(url.startsWith('data:image/png;base64,'));
    const bytes=Buffer.from(url.slice('data:image/png;base64,'.length),'base64'),image=readPng(bytes),state=await snapshot(page);
    assert.equal(image.width,state.viewport.width);assert.equal(image.height,state.viewport.height);await writeFile(path,bytes);
    return {path,width:image.width,height:image.height,sha256:createHash('sha256').update(bytes).digest('hex'),evidence:'Canvas snapshot through HTMLCanvasElement.toDataURL; separate from screenshot and GPU mapped readback'};
  }catch(error){return {error:String(error.stack||error),evidence:'Direct canvas snapshot unavailable'};}
}
async function diagnoseEngineOutput(page,avatars,dpr,manifest){
  const results=[],diagnostic={evidence:'Diagnostic executed after all ordinary scene screenshots; adds COPY_SRC to the existing engine surface, without replacing its renderer',avatars,dpr,results,presentationQualified:false};
  (report.engineOutputReadbacks??=[]).push(diagnostic);
  for(const mode of ['full2d','hybrid2d','full3d'])for(const pass of ['color','pick']){
    enterPhase(`engine-raw-readback-${mode}-${avatars}-${pass}`);
    try{
    const prior=await snapshot(page);await command(page,'setMode',mode);await command(page,'setPass',pass);
    await renderedAfter(page,prior.gpuSubmissions+prior.glDrawCalls);
    const expected=manifest.scenes.find(s=>s.mode===mode&&s.avatars===avatars),before=await snapshot(page);
    const raw=await page.evaluate(()=>window.__wonderlandProbe.readGpuFrame()),{base64,...metadata}=raw;
    assert.equal(raw.stamp.sceneHash,expected.fixtureHash);assert.equal(raw.stamp.mode,mode);assert.equal(raw.stamp.pass,pass);
    assert.equal(raw.width,before.viewport.width);assert.equal(raw.height,before.viewport.height);
    const image={width:raw.width,height:raw.height,pixels:Buffer.from(base64,'base64')};assert.equal(image.pixels.length,raw.width*raw.height*4);
    const path=resolve(output,`${mode}-${avatars}-dpr${dpr}-${pass}-gpu-copy.png`),bytes=rgbaPng(image);await writeFile(path,bytes);
    const result={...metadata,mode,pass,avatars,path,sha256:createHash('sha256').update(bytes).digest('hex'),presentationQualified:false};
    if(pass==='pick'){
      const references=expected.idReferences.filter(r=>r.width===raw.width&&r.height===raw.height);assert.equal(references.length,1);
      const reference=references[0],ids=await readFile(resolve(root,'tools/swarm-c/output/reference',reference.ids)),depth=await readFile(resolve(root,'tools/swarm-c/output/reference',reference.depth));
      result.idComparison=compareIds(image,ids,depth,before.idMap,reference);
    }else if(dpr===1){
      result.color=colorDifference(image,readPpm(await readFile(resolve(root,`tools/swarm-c/output/reference/${mode}-${avatars}.ppm`))));
    }else result.colorComparison='Physical DPR2 color oracle is not generated; retain raw pixels without resampling';
    results.push({...result,readbackCompleted:true});
    }catch(error){results.push({mode,pass,avatars,readbackCompleted:false,error:String(error.stack||error),presentationQualified:false});}
  }
  await command(page,'setPass','color');
  if(results.some(result=>!result.readbackCompleted))throw new Error('One or more engine output reads failed; partial results retained in engineOutputReadbacks');
  return diagnostic;
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
    const result=await Promise.race([
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
        finally{window.__wonderlandDiagnosticDevice=device;}
      }),
      new Promise(resolveTimeout=>{timer=setTimeout(()=>resolveTimeout({passed:false,error:'WebGPU diagnostic timed out after 20 seconds'}),20000);})
    ]);
    if(result.passed){
      const compare=image=>({width:image.width,height:image.height,passed:image.width===4&&image.height===4&&image.pixels.every((v,i)=>Math.abs(v-result.expected[i%4])<=1),firstPixel:Array.from(image.pixels.slice(0,4))});
      try{
        const canvasUrl=await page.evaluate(()=>document.querySelector('canvas').toDataURL('image/png'));
        assert.ok(canvasUrl.startsWith('data:image/png;base64,'));
        const directBytes=Buffer.from(canvasUrl.slice('data:image/png;base64,'.length),'base64'),direct=readPng(directBytes);
        const directPath=resolve(output,'independent-webgpu-clear-canvas.png');await writeFile(directPath,directBytes);
        result.canvasSnapshot={...compare(direct),path:directPath};
      }catch(error){result.canvasSnapshot={passed:false,error:String(error.stack||error)};}
      try{
        const measurement=await page.evaluate(()=>{const r=document.querySelector('canvas').getBoundingClientRect(),e=document.documentElement;return {x:r.x+scrollX,y:r.y+scrollY,width:r.width,height:r.height,documentWidth:Math.max(e.scrollWidth,e.clientWidth),documentHeight:Math.max(e.scrollHeight,e.clientHeight)};});
        const pagePath=resolve(output,'independent-webgpu-clear-page.png'),whole=readPng(await page.screenshot({path:pagePath,fullPage:true,scale:'css'}));
        assert.equal(whole.width,measurement.documentWidth);assert.equal(whole.height,measurement.documentHeight);
        const crop=extractPixels(whole,measurement),cropPath=resolve(output,'independent-webgpu-clear-crop.png');await writeFile(cropPath,rgbaPng(crop));
        result.presentation={...compare(crop),path:cropPath,pagePath,measurement};
      }catch(error){result.presentation={passed:false,error:String(error.stack||error)};}
      result.evidence='Separate WebGPU clear: mapped bytes, direct canvas snapshot and document presentation are recorded independently; no engine qualification';
    }
    return result;
  }finally{clearTimeout(timer);await page.evaluate(()=>window.__wonderlandDiagnosticDevice?.destroy()).catch(()=>{});await context.close();}
}

try{
  const manifest=JSON.parse(await readFile(resolve(root,'tools/swarm-c/output/reference/manifest.json'),'utf8'));
  assert.equal(manifest.width,640);assert.equal(manifest.height,480);
  report.referenceEvidence=manifest.evidence;
  const {chromium}=await import('playwright');
  const args=backend==='webgl2'?['--use-gl=angle','--use-angle=swiftshader-webgl','--enable-unsafe-swiftshader']:
    // Chromium 151's own VulkanSwiftShader pixel tests initialize both the
    // display compositor and WebGPU on SwiftShader; WebGPU alone cannot create
    // the shared swapchain image on this headless Linux runner.
    ['--enable-features=Vulkan','--use-gl=angle','--use-angle=swiftshader','--use-vulkan=swiftshader',
      '--use-webgpu-adapter=swiftshader','--disable-vulkan-surface','--enable-unsafe-webgpu'];
  report.browserArgs=args;
  report.browserProcesses=[];
  for(const avatars of [32,64]){
    enterPhase(`browser-launch-${avatars}`);
    const dpr=avatars===32?1:2;
    // Winit uses devicePixelContentBoxSize, which can differ from an emulated
    // window.devicePixelRatio. Set the process scale too, then measure both.
    if(browser)await browser.close();if(browserServer)await browserServer.close();
    browser=null;browserServer=null;
    const processArgs=[...args,`--force-device-scale-factor=${dpr}`];
    browserServer=await chromium.launchServer({channel:'chromium',headless:true,args:processArgs,host:'127.0.0.1'});
    browserServer.process().stderr?.on('data',recordBrowserProcess);
    browser=await chromium.connect(browserServer.wsEndpoint());report.browserVersion=browser.version();
    report.browserProcesses.push({avatars,dpr,args:processArgs,version:browser.version()});
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
      // This page creates no graphics context and cannot warm the engine.
      await page.goto(new URL('/__pixel_geometry.html',base).href);
      const pixelGeometry=await page.evaluate(()=>new Promise(resolveGeometry=>{
        const canvas=document.querySelector('canvas'),observer=new ResizeObserver(entries=>{
          const entry=entries[0],box=entry.devicePixelContentBoxSize?.[0];observer.disconnect();
          resolveGeometry({dpr:devicePixelRatio,cssWidth:entry.contentRect.width,cssHeight:entry.contentRect.height,
            physicalWidth:box?.inlineSize??null,physicalHeight:box?.blockSize??null});
        });observer.observe(canvas,{box:'device-pixel-content-box'});
      }));
      check(`browser-pixel-density-${avatars}`,pixelGeometry.dpr===dpr&&pixelGeometry.physicalWidth===100*dpr&&pixelGeometry.physicalHeight===80*dpr,pixelGeometry);
      assert.equal(pixelGeometry.physicalWidth,100*dpr,'Browser physical pixel content box must match the requested DPR');
      assert.equal(pixelGeometry.physicalHeight,80*dpr,'Browser physical pixel content box must match the requested DPR');
      const url=`${base}?variant=${variant}&mode=hybrid2d&avatars=${avatars}&tick=30`;
      enterPhase(`engine-startup-${avatars}`);
      await page.goto(url,{waitUntil:'domcontentloaded'});const initial=await ready(page);
      check(`startup-${avatars}`,initial.actualBackend===backend&&initial.wasmMemoryShared===false&&initial.crossOriginIsolated===false,initial);
      await page.evaluate(()=>window.__wonderlandProbe.resize(642,482));
      await page.waitForTimeout(250);
      const box=await page.locator('#engine-canvas').boundingBox();assert.equal(Math.round(box.width),640);assert.equal(Math.round(box.height),480);
      for(const mode of ['full2d','hybrid2d','full3d']){
        enterPhase(`scene-${mode}-${avatars}`);
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
        const pickState=await snapshot(page),pickCapture=await captureCanvas(page,pickPath,'device'),pickBytes=pickCapture.bytes;
        const idReferences=expected.idReferences?.filter(reference=>reference.width===pickCapture.image.width&&reference.height===pickCapture.image.height);
        assert.equal(idReferences?.length,1,'Exactly one rerasterized physical ID reference must match the observed backing dimensions');
        const idReference=idReferences[0];
        const ids=await readFile(resolve(root,'tools/swarm-c/output/reference',idReference.ids));
        const depths=await readFile(resolve(root,'tools/swarm-c/output/reference',idReference.depth));
        const idComparison=compareIds(pickCapture.image,ids,depths,pickState.idMap,idReference);
        check(`gpu-id-parity-${mode}-${avatars}`,idComparison.checked>100&&idComparison.ownerlessOcclusionChecked>0&&idComparison.mismatched===0,idComparison);
        const directPick=backend==='webgpu'?await directCanvasSnapshot(page,resolve(output,`${prefix}-pick-canvas.png`)):null;
        const scene={mode,avatars,dpr,state:pickState,color,idComparison,idReference,captures:{color:capture.capture,pick:pickCapture.capture},directCanvas:directPick,colorScreenshot:`${prefix}-color.png`,idScreenshot:`${prefix}-pick.png`,
          artifactSha256:{color:createHash('sha256').update(colorBytes).digest('hex'),pick:createHash('sha256').update(pickBytes).digest('hex')}};
        report.scenes.push(scene);
        const logicalIds=await readFile(resolve(root,`tools/swarm-c/output/reference/${mode}-${avatars}.ids`));
        const target=logicalSelectionPoint(logicalIds);
        if(target){
          const {x,y,object_id,generation}=target,selected=await command(page,'selectAt',x,y);
          check(`stable-selection-${mode}-${avatars}`,selected.selection?.object_id===object_id&&selected.selection?.generation===generation,{expected:{object_id,generation},actual:selected.selection,source:selected.selectionSource});
        }else check(`stable-selection-${mode}-${avatars}`,false,{reason:'CPU reference has no selectable stable interior'});
        await command(page,'setPass','color');
      }

      if(backend==='webgpu')await step(`engine-output-readback-diagnostic-${avatars}`,async()=>diagnoseEngineOutput(page,avatars,dpr,manifest));

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
        await captureCanvas(page,resolve(output,`resize-${avatars}-dpr${dpr}.png`),'css',{width:400,height:300});
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
    finally{enterPhase(`context-teardown-${avatars}`);await context.tracing.stop({path:resolve(output,`trace-${avatars}.zip`)});await context.close();}
  }
  if(backend==='webgpu'){
    enterPhase('independent-webgpu-diagnostic');
    report.webgpuDiagnostic=await diagnoseWebGpu();
    console.log('WONDERLAND_WEBGPU_DIAGNOSTIC '+JSON.stringify(report.webgpuDiagnostic));
  }
}catch(error){failure('runner',error);}
finally{
  enterPhase('runner-teardown');
  if(browser)await browser.close();if(browserServer)await browserServer.close();await new Promise(resolveClose=>server.close(resolveClose));
  report.finishedAt=new Date().toISOString();report.status=report.failures.length?'failed':'software-browser-checks-passed';
  await writeFile(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');
  await writeFile(resolve(output,'console.json'),JSON.stringify(report.console,null,2)+'\n');
  await writeFile(resolve(output,'browser-process.log'),browserProcessLog.join(''));
  console.log(JSON.stringify({variant,status:report.status,scenes:report.scenes.length,failures:report.failures.length,rendererQualified:false,report:resolve(output,'report.json')}));
  clearTimeout(phaseWatchdog);
  process.exitCode=report.failures.length?1:0;
}
