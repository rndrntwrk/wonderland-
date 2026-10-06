// Real WebGL2 execution of Rust-prepared source geometry; never a fake renderer.
import assert from 'node:assert/strict';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {createServer} from 'node:http';
import {resolve,relative,sep,extname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {png} from './png.mjs';
const root=fileURLToPath(new URL('../../../../',import.meta.url));
const output=resolve(root,'tests/output/source-gpu');await mkdir(output,{recursive:true});
const {chromium}=await import(process.env.WONDERLAND_PLAYWRIGHT_MODULE||'playwright');
const scenes=JSON.parse(await readFile(resolve(output,'manifest.json'),'utf8'));
const server=createServer(async(req,res)=>{
  try{
    const path=resolve(root,'.'+decodeURIComponent(new URL(req.url,'http://local').pathname));
    const name=relative(root,path);if(name==='..'||name.startsWith('..'+sep)){res.writeHead(403);res.end();return;}
    if(req.url==='/favicon.ico'){res.writeHead(204);res.end();return;}
    const body=req.url==='/'?Buffer.from('<!doctype html><html><body style="margin:0;background:#eee"><canvas id="world" width="256" height="192"></canvas></body></html>'):await readFile(path);
    res.writeHead(200,{'Content-Type':req.url==='/'?'text/html':({'.mjs':'text/javascript','.json':'application/json'})[extname(path)]||'application/octet-stream'});res.end(body);
  }catch{if(!res.headersSent)res.writeHead(404);res.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const browser=await chromium.launch({channel:'chromium',headless:true,args:['--use-gl=angle','--use-angle=swiftshader-webgl','--enable-unsafe-swiftshader']});
const report={schema:1,browser:browser.version(),evidence:'actual Chromium WebGL2 source frame owner; synthetic source document; CPU reference; not live server acceptance',scenes:[],lifecycle:[],errors:[]};
const context=await browser.newContext({viewport:{width:1024,height:768}}),page=await context.newPage();
page.on('pageerror',error=>report.errors.push(String(error)));
try{
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.evaluate(async()=>{
    window.sourceGpu=await import('/apps/web-shell/public/world-gpu.mjs');
    window.canvas=document.querySelector('#world');
    window.setFrame=frame=>window.sourceGpu.paintSourceWorld(window.canvas,JSON.stringify(frame));
  });
  for(const scene of scenes){
    const frame=JSON.parse(await readFile(resolve(output,scene.name+'.json'),'utf8'));
    const cpu=await readFile(resolve(output,scene.name+'.rgba'));
    await page.evaluate(frame=>{window.frame=frame;window.setFrame(frame);},frame);
    const observed=await page.evaluate(()=>{
      const gl=window.canvas.getContext('webgl2');
      const pixels=new Uint8Array(window.canvas.width*window.canvas.height*4);
      // Redraw in this task before reading the default buffer (not preserveDrawingBuffer).
      window.setFrame(window.frame);gl.readPixels(0,0,window.canvas.width,window.canvas.height,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
      const debug=gl.getExtension('WEBGL_debug_renderer_info');
      return {pixels:Array.from(pixels),renderer:debug?gl.getParameter(debug.UNMASKED_RENDERER_WEBGL):gl.getParameter(gl.RENDERER),version:gl.getParameter(gl.VERSION),stats:window.sourceGpu.worldGpuStats(window.canvas)};
    });
    let absolute=0,square=0,over8=0,count=0;
    for(let y=0;y<scene.height;y++)for(let x=0;x<scene.width;x++)for(let c=0;c<3;c++){
      const d=Math.abs(cpu[(y*scene.width+x)*4+c]-observed.pixels[((scene.height-y-1)*scene.width+x)*4+c]);
      absolute+=d;square+=d*d;over8+=d>8?1:0;count++;
    }
    const color={mae:absolute/count,rms:Math.sqrt(square/count),fractionOver8:over8/count};
    assert.ok(color.mae<=4&&color.rms<=12&&color.fractionOver8<=.03,JSON.stringify(color));
    // Keep sample cost bounded while retaining background plus multiple tiles.
    const targets=scene.picks.filter((_,index)=>index%Math.max(1,Math.floor(scene.picks.length/24))===0);
    assert.ok(targets.some(point=>point.index>0));assert.ok(targets.some(point=>point.index===0));
    for(const point of targets){
      const pick=await page.evaluate(async point=>JSON.parse(await window.sourceGpu.pickSourceWorld(window.canvas,point.x,point.y)),point);
      assert.equal(pick.index,point.index);assert.equal(pick.generation,frame.generation);
    }
    const photo=await page.evaluate(async()=>{
      const api=window.sourceGpu,canvas=window.canvas,gl=canvas.getContext('webgl2'),toBlob=canvas.toBlob;
      const before=api.worldGpuStats(canvas);let observed;
      canvas.toBlob=function(callback,type){
        const pixels=new Uint8Array(this.width*this.height*4);gl.readPixels(0,0,this.width,this.height,gl.RGBA,gl.UNSIGNED_BYTE,pixels);observed=Array.from(pixels);
        toBlob.call(this,callback,type);
      };
      const pick=api.pickSourceWorld(canvas,100,100);
      let pending;
      try{pending=api.captureSourceWorld(canvas,window.frame.generation,JSON.stringify({test:'Rust source fixture',revision:{tick:'9007199254740993'}}));}
      finally{canvas.toBlob=toBlob;}
      const receipt=JSON.parse(await pending),image=await (await fetch(receipt.image_url)).arrayBuffer(),details=await (await fetch(receipt.metadata_url)).json();
      const picked=JSON.parse(await pick),after=api.worldGpuStats(canvas);api.clearSourceWorldCapture(canvas);
      return {before,after,receipt,image:Array.from(new Uint8Array(image)),details,observed,picked,released:api.worldGpuStats(canvas)};
    });
    const photoBytes=Buffer.from(photo.image),decoded=png(photoBytes);assert.equal(decoded.width,scene.width);assert.equal(decoded.height,scene.height);
    for(let y=0;y<scene.height;y++)for(let x=0;x<scene.width;x++)for(let c=0;c<4;c++)assert.equal(decoded.pixels[(y*scene.width+x)*4+c],photo.observed[((scene.height-y-1)*scene.width+x)*4+c]);
    assert.equal(photo.details.image.sha256,createHash('sha256').update(photoBytes).digest('hex'));
    assert.equal(photo.details.source.revision.tick,'9007199254740993');assert.equal(photo.picked.generation,frame.generation);
    for(const key of ['generation','meshes','buffers','textures','framebuffers','renderbuffers'])assert.equal(photo.before[key],photo.after[key]);
    assert.equal(photo.after.captureUrls,2);assert.equal(photo.released.captureUrls,0);assert.equal(photo.released.capturePending,false);
    await writeFile(resolve(output,scene.name+'-export.png'),photoBytes);
    report.lifecycle.push({name:'exact PNG and sidecar from '+scene.name,pixels:scene.width*scene.height,concurrentPickPreserved:true,generation:frame.generation});
    await page.locator('canvas').screenshot({path:resolve(output,scene.name+'.png')});
    report.scenes.push({name:scene.name,triangles:scene.triangles,color,picks:targets.length,renderer:observed.renderer,version:observed.version,stats:observed.stats,frameSha256:createHash('sha256').update(JSON.stringify(frame)).digest('hex')});
  }
  await page.evaluate(async()=>{
    const api=window.sourceGpu,canvas=window.canvas,frame=window.frame;
    const initial=api.worldGpuStats(canvas);
    for(let i=0;i<20;i++)window.setFrame({...frame,generation:String(100+i)});
    const final=api.worldGpuStats(canvas);
    for(const key of ['meshes','buffers','textures','framebuffers','renderbuffers'])if(initial[key]!==final[key])throw new Error('Resource growth: '+key);
    // Delay a genuine pixel-transfer fence. A new frame cancels the old request.
    const gl=canvas.getContext('webgl2'),wait=gl.clientWaitSync.bind(gl);gl.clientWaitSync=()=>gl.TIMEOUT_EXPIRED;
    const pending=api.pickSourceWorld(canvas,100,100).then(()=>({completed:true}),error=>({name:error.name}));
    window.setFrame({...frame,generation:'120'});
    const stale=await pending;if(stale.name!=='AbortError')throw new Error('Stale GPU request was accepted');
    gl.clientWaitSync=wait;
    const newer=await api.pickSourceWorld(canvas,100,100);if(JSON.parse(newer).generation!=='120')throw new Error('Current GPU generation not retained');
  });
  report.lifecycle.push('20 replacements retain owned-resource counts; real transfer cancelled on replacement; new generation remains pickable');
  await page.evaluate(async()=>{
    const api=window.sourceGpu,canvas=window.canvas,toBlob=canvas.toBlob;
    let release,encoded;const encoding=new Promise(resolve=>{encoded=resolve;});
    canvas.toBlob=function(callback,type){toBlob.call(this,blob=>{release=()=>callback(blob);encoded();},type);};
    const pending=api.captureSourceWorld(canvas,'120','{}').catch(error=>({name:error.name}));
    await encoding;canvas.toBlob=toBlob;window.setFrame({...window.frame,generation:'121'});
    if((await pending).name!=='AbortError')throw new Error('Replaced frame capture was accepted');
    release();await new Promise(resolve=>setTimeout(resolve,0));
    const state=api.worldGpuStats(canvas);if(state.captureUrls||state.capturePending)throw new Error('Stale photo retained owned resources');
  });
  report.lifecycle.push('actual PNG encoder callback cancelled on source frame replacement; delayed completion creates no URLs');
  await page.evaluate(()=>{
    const canvas=window.canvas,gl=canvas.getContext('webgl2');window.loss=gl.getExtension('WEBGL_lose_context');
    if(!window.loss)throw new Error('Actual context-loss extension unavailable');window.loss.loseContext();
  });
  await page.waitForFunction(()=>window.sourceGpu.worldGpuStats(window.canvas).lost);
  await page.evaluate(()=>window.loss.restoreContext());
  await page.waitForFunction(()=>!window.sourceGpu.worldGpuStats(window.canvas).lost);
  await page.evaluate(async()=>{
    window.setFrame({...window.frame,generation:'121'});
    const result=JSON.parse(await window.sourceGpu.pickSourceWorld(window.canvas,100,100));if(result.generation!=='121')throw new Error('Context restoration is stale');
    window.sourceGpu.disposeSourceWorld(window.canvas);
    const state=window.sourceGpu.worldGpuStats(window.canvas);
    if(state.ready||state.pending||state.buffers||state.textures||state.framebuffers)throw new Error('Disposed owner retained resources');
  });
  report.lifecycle.push('actual WebGL2 loss/restoration rebuilds resources and restores picks; disposal clears owned resources');
  assert.deepEqual(report.errors,[]);report.status='passed';
}catch(error){report.status='failed';report.error=String(error.stack||error);throw error;}
finally{await writeFile(resolve(output,'browser-report.json'),JSON.stringify(report,null,2)+'\n');await browser.close();await new Promise(resolve=>server.close(resolve));}
console.log(JSON.stringify({status:report.status,scenes:report.scenes.length,lifecycle:report.lifecycle.length}));
