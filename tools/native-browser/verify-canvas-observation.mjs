// TEST ONLY: executable WebGL/compositor contract for the observation helper.
// This authored 64x64 fixture is not the Rust game or an application substitute.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {mkdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {chromium} from 'playwright';
import {observeNativeCanvas,canvasPixels,canvasImage,visibleAvatarPixels} from './canvas-evidence.mjs';

const output=resolve(process.env.WONDERLAND_CANVAS_EVIDENCE_OUTPUT??'/tmp/wonderland-canvas-observation');
await mkdir(output,{recursive:true});
const report={passed:false,scope:'Authored real WebGL2 default-buffer and ordinary compositor observation, not game acceptance',checks:[],errors:[]};
const html=`<!doctype html><meta charset="utf-8"><title>Canvas observation regression</title>
<style>.native-lot{position:relative;width:64px;height:64px}.world-viewport,canvas{width:64px;height:64px;display:block}
#hud{position:absolute;top:0;left:0;width:64px;height:16px;background:white;font:12px monospace;color:black}</style>
<section class="native-lot"><div class="world-viewport"><canvas width="64" height="64"></canvas></div><div id="hud">Tick 10</div></section>
<script>
const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl2',{alpha:false,antialias:false,depth:true,stencil:true,premultipliedAlpha:false});
if(!gl)throw Error('Real WebGL2 is required');
const loss=gl.getExtension('WEBGL_lose_context');if(!loss)throw Error('Context-loss fixture requires extension');
let generation=0;
function draw(shift=0){
 gl.disable(gl.SCISSOR_TEST);gl.clearColor(.2,.3,.4,1);gl.clear(gl.COLOR_BUFFER_BIT);
 gl.enable(gl.SCISSOR_TEST);gl.scissor(20+shift,20,12,12);gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);gl.disable(gl.SCISSOR_TEST);
 canvas.dataset.gpuState='ready';canvas.dataset.frameGeneration=String(++generation);
}
canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();canvas.dataset.gpuState='lost';});
canvas.addEventListener('webglcontextrestored',()=>{canvas.dataset.gpuState='restored-awaiting-frame';});
window.fixture={draw,attributes:()=>gl.getContextAttributes(),clearWithoutPublication:()=>{
 gl.disable(gl.SCISSOR_TEST);gl.clearColor(.2,.3,.4,1);gl.clear(gl.COLOR_BUFFER_BIT);
},lose:()=>loss.loseContext(),
restore:()=>loss.restoreContext()};
requestAnimationFrame(()=>draw());
</script>`;
const server=createServer((request,response)=>{response.writeHead(200,{'Content-Type':'text/html','Cache-Control':'no-store'});response.end(html);});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
let browser;
const record=(name,details={})=>report.checks.push({name,...details});
const pixelHash=image=>createHash('sha256').update(image.pixels).digest('hex');
try{
 browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 report.browser=browser.version();
 const page=await browser.newPage({viewport:{width:300,height:220},deviceScaleFactor:1});
 page.on('pageerror',error=>report.errors.push(String(error)));
 await observeNativeCanvas(page);
 await page.goto(`http://127.0.0.1:${server.address().port}`);
 const canvas=page.locator('canvas');
 const first=await canvasPixels(canvas),visible=await visibleAvatarPixels(canvas);
 assert.equal(await page.evaluate(()=>fixture.attributes().preserveDrawingBuffer),false);
 record('Actual non-preserved WebGL2 buffer captured on its post-draw publication');
 const pageBefore=pixelHash(await canvasImage(canvas));
 await page.evaluate(()=>document.querySelector('#hud').textContent='Tick 999');
 assert.equal(await canvasPixels(canvas),first);
 assert.equal(await visibleAvatarPixels(canvas),visible);
 assert.notEqual(pixelHash(await canvasImage(canvas)),pageBefore);
 record('Changing HUD changes ordinary screenshot but neither complete canvas nor visible-avatar identity');
 await page.screenshot({path:resolve(output,'hud-change.png')});
 await page.evaluate(()=>fixture.draw(8));
 const moved=await canvasPixels(canvas);
 assert.notEqual(moved,first);assert.notEqual(await visibleAvatarPixels(canvas),visible);
 record('An actual draw changes both full-frame and independently visible avatar observations');
 await page.screenshot({path:resolve(output,'moved.png')});
 // Fault control: a secretly cleared visible buffer cannot be certified by an
 // older accepted snapshot. The independent ordinary-compositor witness fails.
 await page.evaluate(()=>fixture.clearWithoutPublication());
 await assert.rejects(visibleAvatarPixels(canvas),/No synthetic avatar/);
 await assert.rejects(canvasPixels(canvas),/No synthetic avatar/);
 record('The same native retention helper rejects a blank compositor despite an older accepted snapshot');
 // A stale nonblank page must also invalidate equality, with no generation update.
 await page.evaluate(()=>{fixture.draw(8);});
 const beforeCover=await canvasPixels(canvas);
 await page.evaluate(()=>{const c=document.createElement('div');c.id='fault-cover';c.style.cssText='position:absolute;left:0;top:0;width:64px;height:64px;background:rgb(51,77,102)';c.innerHTML='<div style="position:absolute;left:4px;top:32px;width:12px;height:12px;background:red"></div>';document.querySelector('.native-lot').append(c);});
 assert.notEqual(await canvasPixels(canvas),beforeCover,'A stale nonblank composited pose cannot pass native retention equality');
 await page.evaluate(()=>document.querySelector('#fault-cover').remove());
 assert.equal(await canvasPixels(canvas),beforeCover);
 record('Stale nonblank compositor output is detected by the native retention comparison');
 await page.evaluate(()=>fixture.draw());
 assert.equal(await canvasPixels(canvas),first);
 await page.evaluate(()=>fixture.lose());
 await page.waitForFunction(()=>document.querySelector('canvas').dataset.gpuState==='lost');
 assert.equal(await canvas.evaluate(c=>globalThis.__wonderlandCanvasEvidence.read(c)),null);
 await page.evaluate(()=>fixture.restore());
 await page.waitForFunction(()=>document.querySelector('canvas').dataset.gpuState==='restored-awaiting-frame');
 assert.equal(await canvas.evaluate(c=>globalThis.__wonderlandCanvasEvidence.read(c)),null);
 await page.evaluate(()=>fixture.draw());
 assert.equal(await canvasPixels(canvas),first);
 assert.equal(await visibleAvatarPixels(canvas),visible);
 record('Actual context loss invalidates observations until a fresh restored draw');
 await page.evaluate(()=>document.querySelector('.native-lot').remove());
 assert.equal(await page.evaluate(()=>document.querySelector('canvas')),null);
 assert.deepEqual(report.errors,[]);
 report.passed=true;
}catch(error){report.failure=String(error.stack||error);process.exitCode=1;}
finally{
 if(browser)await browser.close();
 await new Promise(resolve=>server.close(resolve));
 await writeFile(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');
 console.log(JSON.stringify(report,null,2));
}
