// Exercises the actual Trunk-built application, not a substitute canvas host.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile,writeFile,mkdir,stat,readdir} from 'node:fs/promises';
import {createReadStream} from 'node:fs';
import {resolve,relative,sep,extname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {inflateSync} from 'node:zlib';
import {materialPixel} from './material-pixels.mjs';
const root=fileURLToPath(new URL('../../../../',import.meta.url));
const dist=resolve(process.env.WONDERLAND_DIST||resolve(root,'apps/web-shell/dist'));
const output=resolve(process.env.WONDERLAND_APPLICATION_REPORT||resolve(root,'tests/output/source-application'));
await mkdir(output,{recursive:true});
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
// Decode only bounded Chromium RGB/RGBA screenshots for the nonempty-pixel gate.
function png(bytes){
  assert.ok(bytes.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])));
  let width,height,channels,offset=8,end=false;const data=[];
  while(offset+12<=bytes.length){
    const n=bytes.readUInt32BE(offset),kind=bytes.toString('ascii',offset+4,offset+8);assert.ok(n<=64*1024*1024&&offset+n+12<=bytes.length);
    const chunk=bytes.subarray(offset+8,offset+8+n);
    if(kind==='IHDR'){assert.equal(n,13);width=chunk.readUInt32BE(0);height=chunk.readUInt32BE(4);assert.ok(width>0&&height>0&&width<=4096&&height<=4096);assert.equal(chunk[8],8);assert.ok([2,6].includes(chunk[9]));assert.equal(chunk[12],0);channels=chunk[9]===6?4:3;}
    if(kind==='IDAT')data.push(chunk);if(kind==='IEND'){end=true;break;}offset+=n+12;
  }
  assert.ok(end&&channels);const stride=width*channels,raw=inflateSync(Buffer.concat(data),{maxOutputLength:(stride+1)*height});assert.equal(raw.length,(stride+1)*height);
  const pixels=new Uint8Array(width*height*4);let previous=new Uint8Array(stride);
  for(let y=0;y<height;y++){
    const filter=raw[y*(stride+1)],row=new Uint8Array(stride);assert.ok(filter<=4);
    for(let x=0;x<stride;x++){
      const a=x>=channels?row[x-channels]:0,b=previous[x],c=x>=channels?previous[x-channels]:0;let prediction=0;
      if(filter===1)prediction=a;else if(filter===2)prediction=b;else if(filter===3)prediction=Math.floor((a+b)/2);
      else if(filter===4){const p=a+b-c,pa=Math.abs(p-a),pb=Math.abs(p-b),pc=Math.abs(p-c);prediction=pa<=pb&&pa<=pc?a:pb<=pc?b:c;}
      row[x]=(raw[y*(stride+1)+1+x]+prediction)&255;
    }
    for(let x=0;x<width;x++){pixels.set(row.subarray(x*channels,x*channels+3),(y*width+x)*4);pixels[(y*width+x)*4+3]=channels===4?row[x*channels+3]:255;}previous=row;
  }
  return {width,height,pixels};
}
async function inventory(path=dist){
  const entries=[];for(const entry of await readdir(path,{withFileTypes:true})){const name=resolve(path,entry.name);if(entry.isDirectory())entries.push(...await inventory(name));else if(entry.isFile()){const bytes=await readFile(name);entries.push({path:relative(dist,name),bytes:bytes.length,sha256:sha(bytes)});}}
  return entries.sort((a,b)=>a.path.localeCompare(b.path));
}
const bundle=await inventory();assert.ok(bundle.some(file=>file.path.endsWith('.wasm')),'A real built WASM distribution is required');
const mime={'.html':'text/html','.wasm':'application/wasm','.js':'text/javascript','.mjs':'text/javascript','.css':'text/css','.json':'application/json','.svg':'image/svg+xml','.png':'image/png','.webp':'image/webp','.woff2':'font/woff2'};
const server=createServer(async(req,res)=>{
  try{
    const pathname=decodeURIComponent(new URL(req.url,'http://localhost').pathname);if(pathname==='/favicon.ico'){res.writeHead(204);res.end();return;}
    const path=resolve(dist,'.'+(pathname==='/'?'/index.html':pathname));const name=relative(dist,path);if(name==='..'||name.startsWith('..'+sep)){res.writeHead(403);res.end();return;}
    const info=await stat(path);if(!info.isFile()||info.size>128*1024*1024)throw new Error('Invalid asset');
    res.writeHead(200,{'Content-Type':mime[extname(path)]||'application/octet-stream','Cache-Control':'no-store'});createReadStream(path).on('error',()=>res.destroy()).pipe(res);
  }catch{if(!res.headersSent)res.writeHead(404);res.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const {chromium}=await import(process.env.WONDERLAND_PLAYWRIGHT_MODULE||'playwright');
const browser=await chromium.launch({channel:'chromium',headless:true,args:['--use-gl=angle','--use-angle=swiftshader-webgl','--enable-unsafe-swiftshader']});
const report={schema:1,source:process.env.GITHUB_SHA||null,browser:browser.version(),bundle,scenarios:[],errors:[],requests:[],qualification:'Built application, source-world GPU viewer, desktop and narrow viewport; not multiplayer or physical mobile acceptance'};
const context=await browser.newContext({viewport:{width:1364,height:936}}),page=await context.newPage();page.setDefaultTimeout(30000);
await page.addInitScript(()=>{const revoke=URL.revokeObjectURL.bind(URL);window.__revokedPhotoUrls=[];URL.revokeObjectURL=url=>{window.__revokedPhotoUrls.push(url);return revoke(url);};});
page.on('pageerror',error=>report.errors.push(String(error.stack||error)));
page.on('console',message=>{if(message.type()==='error')report.errors.push(message.text());});
page.on('response',response=>{if(response.status()>=400)report.requests.push({url:response.url(),status:response.status()});});
const canvas=()=>page.locator('.world-viewport canvas');
async function ready(){await page.waitForFunction(()=>{const c=document.querySelector('.world-viewport canvas');return c?.dataset.gpuState==='ready'&&!document.querySelector('.world-viewport.world-busy');});assert.equal(await page.locator('.world-view-error').count(),0);}
async function changed(before){await page.waitForFunction(before=>document.querySelector('.world-viewport canvas')?.dataset.frameGeneration!==before,before);await ready();}
async function capture(name){const bytes=await page.screenshot({path:resolve(output,name+'.png'),fullPage:true});const image=png(bytes);report.scenarios.push({name,width:image.width,height:image.height,sha256:sha(bytes)});return image;}
// Observe the real source color buffer in the same task that the production
// capture calls toBlob; do not replace the image encoder or renderer.
async function installCaptureObserver(){
  await page.evaluate(()=>{
    const canvas=document.querySelector('.world-viewport canvas'),toBlob=canvas.toBlob.bind(canvas);
    canvas.toBlob=(callback,type)=>{
      const gl=canvas.getContext('webgl2'),pixels=new Uint8Array(canvas.width*canvas.height*4);
      gl.readPixels(0,0,canvas.width,canvas.height,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
      window.__photoObserved={width:canvas.width,height:canvas.height,pixels:Array.from(pixels)};
      toBlob(blob=>{if(window.__delayPhoto){window.__finishPhoto=()=>callback(blob);}else callback(blob);},type);
    };
  });
}
async function photo(name){
  const before=await canvas().getAttribute('data-frame-generation');
  await page.getByRole('button',{name:'Capture PNG',exact:true}).click();
  const imageLink=page.getByRole('link',{name:'Save PNG',exact:true});await imageLink.waitFor();
  const urls={image:await imageLink.getAttribute('href'),details:await page.getByRole('link',{name:'Photo details',exact:true}).getAttribute('href')};
  const [download]=await Promise.all([page.waitForEvent('download'),imageLink.click()]);
  assert.equal(download.suggestedFilename(),`wonderland-source-view-${before}.png`);
  const path=resolve(output,name+'.png');await download.saveAs(path);const bytes=await readFile(path),image=png(bytes);
  const actual=await page.evaluate(()=>window.__photoObserved);assert.equal(image.width,actual.width);assert.equal(image.height,actual.height);
  for(let y=0;y<image.height;y++)for(let x=0;x<image.width;x++)for(let c=0;c<4;c++){
    assert.equal(image.pixels[(y*image.width+x)*4+c],actual.pixels[((image.height-y-1)*image.width+x)*4+c],`PNG changed the displayed pixel at ${x},${y},${c}`);
  }
  const [detailsDownload]=await Promise.all([page.waitForEvent('download'),page.getByRole('link',{name:'Photo details',exact:true}).click()]);
  const detailsPath=resolve(output,name+'.json');await detailsDownload.saveAs(detailsPath);const metadata=JSON.parse(await readFile(detailsPath,'utf8'));
  assert.equal(metadata.image.sha256,sha(bytes));assert.equal(metadata.image.bytes,bytes.length);assert.equal(metadata.generation,before);
  assert.equal(metadata.kind,'source-view-capture');assert.ok(metadata.source.provenance.origin);assert.ok(Array.isArray(metadata.source.diagnostics));
  assert.equal(await canvas().getAttribute('data-frame-generation'),before,'Capture advanced the frame generation');
  report.scenarios.push({name,width:image.width,height:image.height,sha256:sha(bytes),pixelComparisons:image.width*image.height,generation:before});
  return urls;
}
async function revoked(urls){
  const released=await page.evaluate(()=>window.__revokedPhotoUrls);
  for(const url of Object.values(urls))assert.ok(released.includes(url),'Capture did not release its actual object URL');
}

try{
  await page.goto(`http://127.0.0.1:${server.address().port}/`,{waitUntil:'domcontentloaded'});
  await page.getByRole('button',{name:'Original lot',exact:true}).waitFor();
  await page.getByRole('button',{name:'Original lot',exact:true}).click();await ready();
  const initial=await canvas().getAttribute('data-frame-generation');
  assert.equal(await canvas().getAttribute('data-renderer'),'source-webgl2');
  assert.ok(Number(await page.locator('.world-render-evidence').getAttribute('data-triangles'))>0);
  const image=await capture('original-source-world');const colors=new Set();for(let i=0;i<image.pixels.length;i+=4)colors.add(image.pixels[i]|image.pixels[i+1]<<8|image.pixels[i+2]<<16);assert.ok(colors.size>8,'Actual application screenshot is empty');
  await installCaptureObserver();const originalPhoto=await photo('original-view-export');
  await canvas().focus();await page.keyboard.press('Enter');await page.getByRole('button',{name:'Clear selection',exact:true}).waitFor();
  assert.match(await page.locator('.source-world-inspector strong').innerText(),/^(Tile |Object )/);report.scenarios.push({name:'WASM-resolved GPU selection',selected:await page.locator('.source-world-inspector strong').innerText()});
  await page.getByRole('button',{name:'Rotate right',exact:true}).click();await changed(initial);await page.getByRole('link',{name:'Save PNG',exact:true}).waitFor({state:'detached'});await revoked(originalPhoto);report.scenarios.push({name:'camera changes discard the previous photo without changing source selection'});await capture('rotated-source-world');
  await page.getByRole('button',{name:'Cutaway',exact:true}).click();await ready();assert.equal(await page.getByRole('button',{name:'Cutaway',exact:true}).getAttribute('aria-pressed'),'true');
  await page.getByRole('button',{name:'Floor down',exact:true}).isDisabled().then(disabled=>assert.equal(disabled,true));
  const beforeReset=await canvas().getAttribute('data-frame-generation');await page.getByRole('button',{name:'Reset view',exact:true}).click();await changed(beforeReset);
  // The source import must replace only this viewer, not a saved Home.
  const xml='<house><size>4</size><world><floors><floor level="0" x="1" y="1" value="9"/></floors><walls/></world><objects/></house>';
  await page.locator('.source-open-lot input').setInputFiles({name:'application-source.xml',mimeType:'application/xml',buffer:Buffer.from(xml)});
  await page.getByRole('heading',{name:'application-source.xml',exact:true}).waitFor();await ready();await capture('imported-source-world');const importedPhoto=await photo('imported-view-export');
  const beforeFailure=await canvas().getAttribute('data-frame-generation');await page.locator('.source-open-lot input').setInputFiles({name:'invalid.xml',mimeType:'application/xml',buffer:Buffer.from('<broken>')});
  await page.getByRole('status').filter({hasText:'This blueprint is incomplete or unsupported.'}).waitFor();assert.equal(await canvas().getAttribute('data-frame-generation'),beforeFailure);assert.equal(await page.getByRole('link',{name:'Save PNG',exact:true}).getAttribute('href'),importedPhoto.image);report.scenarios.push({name:'invalid import retains the admitted world and its photo'});
  await page.evaluate(()=>{window.__delayPhoto=true;});await page.getByRole('button',{name:'Capture PNG',exact:true}).click();await page.waitForFunction(()=>typeof window.__finishPhoto==='function');
  await page.evaluate(()=>{const c=document.querySelector('.world-viewport canvas');window.__sourceLoss=c.getContext('webgl2').getExtension('WEBGL_lose_context');if(!window.__sourceLoss)throw new Error('Actual context-loss extension missing');window.__sourceLoss.loseContext();});
  await page.waitForFunction(()=>document.querySelector('.world-viewport canvas').dataset.gpuState==='lost');await page.getByRole('link',{name:'Save PNG',exact:true}).waitFor({state:'detached'});await revoked(importedPhoto);
  await page.evaluate(()=>window.__sourceLoss.restoreContext());await ready();await changed(beforeFailure);await page.evaluate(()=>{window.__delayPhoto=false;window.__finishPhoto();});await page.waitForTimeout(50);assert.equal(await page.getByRole('link',{name:'Save PNG',exact:true}).count(),0);report.scenarios.push({name:'actual context loss/restoration cancels pending PNG and rejects its delayed encoder callback'});
  const beforeNarrow=await canvas().getAttribute('data-frame-generation');await page.setViewportSize({width:390,height:844});await changed(beforeNarrow);await photo('narrow-view-export');
  // A successful download alone must not accept a panel hidden by an import notice.
  const narrowLayout=await page.evaluate(()=>{
    const bounds=selector=>{const r=document.querySelector(selector).getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,bottom:r.bottom};};
    return {inspector:bounds('.source-world-inspector'),notice:bounds('.source-world-notice'),viewport:innerWidth,scrollWidth:document.documentElement.scrollWidth};
  });
  assert.ok(narrowLayout.inspector.y>=narrowLayout.notice.bottom+8,'Photo panel overlaps the active import notice');
  assert.ok(narrowLayout.inspector.x>=0&&narrowLayout.inspector.x+narrowLayout.inspector.width<=narrowLayout.viewport,'Photo panel escapes the narrow viewport');
  assert.equal(narrowLayout.scrollWidth,narrowLayout.viewport,'Photo controls create horizontal page overflow');
  report.scenarios.push({name:'narrow photo inspector clears the import notice',...narrowLayout});
  await capture('narrow-source-world');
  await page.getByRole('button',{name:'Discard photo',exact:true}).click();await page.getByRole('link',{name:'Save PNG',exact:true}).waitFor({state:'detached'});
  await page.getByRole('button',{name:'Reset view',exact:true}).click();await ready();
  // Import normalized source materials through the real Rust application's file
  // control. These intentionally synthetic documents exercise nonempty masks;
  // they are not redistributed original-game assets or proof of corpus parity.
  await page.setViewportSize({width:1364,height:936});await ready();
  for(const name of ['normal-mask','portal-mask']){
    const source=await readFile(resolve(root,'tests/output/source-gpu',name+'.world.json'));
    await page.locator('.source-open-lot input').setInputFiles({name:name+'.json',mimeType:'application/json',buffer:source});
    await page.getByRole('heading',{name:name+'.json',exact:true}).waitFor();await ready();
    await photo(name+'-application-export');
    const image=png(await readFile(resolve(output,name+'-application-export.png')));
    const colored=(x,y,channel)=>materialPixel(image,x,y,channel);
    let red=0,blue=0,target;
    for(let y=2;y<image.height-2;y++)for(let x=2;x<image.width-2;x++){
      red+=colored(x,y,0)?1:0;blue+=colored(x,y,2)?1:0;
      const channel=name==='portal-mask'?2:0;
      if(!target&&[-2,-1,0,1,2].every(dy=>[-2,-1,0,1,2].every(dx=>colored(x+dx,y+dy,channel))))target={x,y};
    }
    assert.ok(red>20,'Masked source body was omitted');
    if(name==='portal-mask')assert.ok(blue>20,'The portal final-group stencil was never exercised');
    else assert.equal(blue,0,'An inactive dynamic group became visible');
    assert.ok(target,'No unambiguous source-object interior for picking');
    const box=await canvas().boundingBox();
    await page.mouse.click(box.x+(target.x+.5)*box.width/image.width,box.y+(target.y+.5)*box.height/image.height);
    await page.waitForFunction(()=>document.querySelector('.source-world-inspector strong')?.textContent.startsWith('Object '));
    const selected=await page.locator('.source-world-inspector strong').innerText();
    assert.match(selected,/^Object /);
    const before=await canvas().getAttribute('data-frame-generation');
    await page.evaluate(()=>{const gl=document.querySelector('.world-viewport canvas').getContext('webgl2');window.__materialLoss=gl.getExtension('WEBGL_lose_context');if(!window.__materialLoss)throw new Error('Missing real context loss');window.__materialLoss.loseContext();});
    await page.waitForFunction(()=>document.querySelector('.world-viewport canvas').dataset.gpuState==='lost');
    await page.evaluate(()=>window.__materialLoss.restoreContext());await changed(before);
    await photo(name+'-restored-export');
    const restored=png(await readFile(resolve(output,name+'-restored-export.png')));
    assert.deepEqual(restored.pixels,image.pixels,'Restoration changed ordered source material pixels');
    assert.equal(await page.locator('.source-world-inspector strong').innerText(),selected);
    report.scenarios.push({name:'source materials '+name,redPixels:red,portalPixels:blue,selected,sourceSha256:sha(source),restorationIdentical:true});
    await capture(name+'-application');
  }
  // Source room lighting must travel through the real JSON import and WASM
  // viewport, not merely a standalone shader host. Compare a wall-shadow recipe
  // against the identical unshadowed control; an all-black fallback cannot pass.
  const litSource=JSON.parse(await readFile(resolve(root,'tests/output/source-gpu/room-lit.world.json'),'utf8'));
  const shadowSource=JSON.parse(await readFile(resolve(root,'tests/output/source-gpu/room-shadow.world.json'),'utf8'));
  shadowSource.lighting.revision='2';
  const importLighting=async(name,world)=>{
    await page.locator('.source-open-lot input').setInputFiles({name:name+'.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(world))});
    await page.getByRole('heading',{name:name+'.json',exact:true}).waitFor();await ready();
  };
  await importLighting('room-lit',litSource);await photo('room-lit-application-export');
  const litImage=png(await readFile(resolve(output,'room-lit-application-export.png')));
  await importLighting('room-shadow',shadowSource);const shadowPhoto=await photo('room-shadow-application-export');
  const shadowImage=png(await readFile(resolve(output,'room-shadow-application-export.png')));
  assert.equal(litImage.width,shadowImage.width);assert.equal(litImage.height,shadowImage.height);
  let shadowPixels=0,litPixels=0;
  for(let i=0;i<litImage.pixels.length;i+=4){
    if(litImage.pixels[i]+litImage.pixels[i+1]+litImage.pixels[i+2]>30)litPixels++;
    if(litImage.pixels[i]+litImage.pixels[i+1]+litImage.pixels[i+2]>shadowImage.pixels[i]+shadowImage.pixels[i+1]+shadowImage.pixels[i+2]+12)shadowPixels++;
  }
  assert.ok(litPixels>100&&shadowPixels>100,'Room lighting or the supplied wall shadow did not reach the real source viewport');
  const beforeBadLight=await canvas().getAttribute('data-frame-generation');
  const malformed=structuredClone(shadowSource);malformed.lighting.geometry[0].walls[0][1]=malformed.lighting.geometry[0].walls[0][0];
  await page.locator('.source-open-lot input').setInputFiles({name:'invalid-light.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(malformed))});
  await page.getByRole('status').filter({hasText:'This lot contains invalid or unsupported world data.'}).waitFor();
  assert.equal(await canvas().getAttribute('data-frame-generation'),beforeBadLight);
  assert.equal(await page.getByRole('link',{name:'Save PNG',exact:true}).getAttribute('href'),shadowPhoto.image,'Rejected lighting discarded the admitted photo');
  await canvas().focus();await page.keyboard.press('Enter');
  await page.waitForFunction(()=>document.querySelector('.source-world-inspector strong')?.textContent.startsWith('Tile '));
  const selectedLit=await page.locator('.source-world-inspector strong').innerText();
  await page.evaluate(()=>{const gl=document.querySelector('.world-viewport canvas').getContext('webgl2');window.__lightingLoss=gl.getExtension('WEBGL_lose_context');if(!window.__lightingLoss)throw new Error('Missing real lighting context loss');window.__lightingLoss.loseContext();});
  await page.waitForFunction(()=>document.querySelector('.world-viewport canvas').dataset.gpuState==='lost');
  await page.evaluate(()=>window.__lightingLoss.restoreContext());await changed(beforeBadLight);
  await photo('room-shadow-restored-export');
  assert.deepEqual(png(await readFile(resolve(output,'room-shadow-restored-export.png'))).pixels,shadowImage.pixels,'Lighting changed after actual GPU restoration');
  assert.equal(await page.locator('.source-world-inspector strong').innerText(),selectedLit);
  report.scenarios.push({name:'source room lighting, rejected malformed shadow, and actual context recovery',litPixels,shadowPixels,selected:selectedLit,restorationIdentical:true});
  await capture('room-shadow-application');
  const beforeClose=await photo('before-close-export');
  await page.getByRole('button',{name:'Back to your Sims',exact:true}).click();await page.locator('.source-world-screen').waitFor({state:'detached'});await revoked(beforeClose);
  await page.getByRole('button',{name:'Original lot',exact:true}).click();await ready();report.scenarios.push({name:'close and reopen creates a fresh GPU owner'});
  assert.deepEqual(report.requests,[]);assert.deepEqual(report.errors,[]);report.status='passed';
}catch(error){report.status='failed';report.error=String(error.stack||error);await page.screenshot({path:resolve(output,'failure.png'),fullPage:true}).catch(()=>{});process.exitCode=1;}
finally{await writeFile(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');await browser.close();await new Promise(resolve=>server.close(resolve));}
console.log(JSON.stringify({status:report.status,scenarios:report.scenarios.length,errors:report.errors.length,requests:report.requests.length}));
