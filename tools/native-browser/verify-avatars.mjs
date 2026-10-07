// TEST ONLY: built native player plus synthetic standalone Vitaboy resources.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {readdir,readFile,mkdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {chromium} from 'playwright';
import {createFixture} from './peer.mjs';
const output=resolve(process.env.WONDERLAND_NATIVE_QA_OUTPUT??'/tmp/wonderland-native-avatar-evidence');
const resources=resolve(process.env.WONDERLAND_AVATAR_QA_FILES??'target/native-avatar-fixtures');
await mkdir(output,{recursive:true});
const gateway=spawn(resolve('target/debug/examples/controlled_replay'),[],{
 env:{...process.env,WONDERLAND_REPLAY_BIND:'127.0.0.1:18787',WONDERLAND_REPLAY_BROWSER_ORIGINS:'http://127.0.0.1:18888'},
 stdio:['ignore','ignore','pipe']});
let fixture,browser;const report={passed:false,scope:'Synthetic standalone-format avatar assets, real Rust native runtime and real browser content loader',checks:[],errors:[],screenshots:[]};
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function shot(page,name) {const file=resolve(output,name+'.png');await page.screenshot({path:file});report.screenshots.push({file:name+'.png',sha256:createHash('sha256').update(await readFile(file)).digest('hex')});}
try {
 let healthy=false;
 for(let i=0;i<100;i++){try{if((await fetch('http://127.0.0.1:18787/health')).ok){healthy=true;break;}}catch{}await delay(100);}
 assert.ok(healthy,'Controlled gateway did not become healthy');
 process.env.WONDERLAND_NATIVE_AVATAR_FIXTURE='1';
 fixture=await createFixture({dist:resolve(process.env.WONDERLAND_NATIVE_QA_DIST??'apps/web-shell/dist'),
   gateway:'http://127.0.0.1:18787',runtimeExecutable:resolve('target/debug/examples/native_browser_peer')});
 browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 report.browser=browser.version();
 const page=await browser.newPage({viewport:{width:1200,height:900}});
 const png=await readFile(resolve(resources,'texture.000002bc0000000e.png'));
 const decoded=await page.evaluate(async data=> {
  const image=await createImageBitmap(new Blob([new Uint8Array(data)],{type:'image/png'}));
  const c=document.createElement('canvas');c.width=image.width;c.height=image.height;
  const ctx=c.getContext('2d');ctx.drawImage(image,0,0);
  const pixel=Array.from(ctx.getImageData(0,0,1,1).data);
  image.close();return {width:c.width,height:c.height,pixel};
 },Array.from(png));
 assert.deepEqual(decoded,{width:1,height:1,pixel:[255,0,0,255]});
 report.checks.push({name:'Synthetic PNG accepted by the real browser decoder',decoded});

 page.on('pageerror',error=>report.errors.push(error.message));
 page.on('console',m=>{if(m.type()==='error')report.errors.push(m.text());});
 await page.goto(fixture.origin);
 await page.getByLabel('Account name',{exact:true}).fill('controlled-player');
 await page.getByLabel('Password',{exact:true}).fill('test-only');
 await page.getByRole('button',{name:'Sign in',exact:true}).click();
 await page.getByRole('heading',{name:'Choose your Sim',exact:true}).waitFor();
 await page.locator('.connected-content-details > summary').click();
 const loader=page.locator('.connected-content-details .content-loader');
 await loader.locator('input[type="file"]').first().setInputFiles((await readdir(resources)).filter(f=>!f.endsWith('.json')).map(f=>resolve(resources,f)));
 await loader.getByText(/original files read/).waitFor();
 await loader.getByText('Resource names and collection roles',{exact:true}).click();
 await loader.getByLabel('Head collections',{exact:true}).fill('');
 await loader.getByLabel('Body collections',{exact:true}).fill('');
 await loader.getByRole('button',{name:'Apply files',exact:true}).click();
 await loader.getByText(/heads.*bodies ready/).waitFor({state:'attached'});
 await page.getByRole('button',{name:'Play as Controlled Alice',exact:true}).click();
 await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();
 await page.getByRole('button',{name:'Select Controlled Source Lot',exact:true}).click();
 await page.getByRole('heading',{name:'Property',exact:true}).waitFor();
 await page.getByRole('button',{name:'Visit',exact:true}).click();
 await page.locator('.native-lot[data-native-live="true"][data-native-avatar-models="1"]').waitFor({timeout:30000});
 const canvas=page.locator('.native-lot canvas');
 await canvas.waitFor();
 report.checks.push({name:'Real content loader binds the admitted native avatar',models:1});
 const frames=new Set();
 for(let i=0;i<14;i++){frames.add(await canvas.evaluate(c=>c.toDataURL()));await delay(120);}
 assert.ok(frames.size>=2,'Accepted changing animation frames must alter the actual canvas');
 report.checks.push({name:'Accepted animation changes render real pixels',distinctCanvasFrames:frames.size});
 assert.equal(await page.locator('.world-view-error').count(),0,'No renderer admission error');
 // Find real rendered synthetic red geometry, then use ordinary pointer events.
 let picked=false;
 for(let attempt=0;attempt<8&&!picked;attempt++){
  const point=await canvas.evaluate(c=>{
   const ctx=c.getContext('2d'),rgba=ctx.getImageData(0,0,c.width,c.height).data,samples=[];
   for(let y=0;y<c.height;y++)for(let x=0;x<c.width;x++){
    const i=(y*c.width+x)*4;if(rgba[i]>150&&rgba[i+1]<30&&rgba[i+2]<30)samples.push([x,y]);
   }
   if(!samples.length)return null;
   const [x,y]=samples[Math.floor(samples.length/2)],b=c.getBoundingClientRect();
   return {x:b.left+x*b.width/c.width,y:b.top+y*b.height/c.height,pixels:samples.length};
  });
  assert.ok(point&&point.pixels>0,'Synthetic avatar geometry must have visible pixels');
  await page.mouse.click(point.x,point.y);
  await delay(70);
  picked=await page.getByRole('heading',{name:'Actions',exact:true}).isVisible();
 }
 assert.ok(picked,'Rendered native avatar must open its source action menu when selected');
 report.checks.push({name:'Depth-tested avatar selection opens actual source actions'});
 await page.getByRole('button',{name:'Close source actions',exact:true}).click();

 await page.getByRole('button',{name:'Needs',exact:true}).click();
 await page.locator('.native-avatar-status').getByText(/sampled from original resources/).waitFor();
 await shot(page,'native-avatar-desktop');
 await page.setViewportSize({width:390,height:844});
 await shot(page,'native-avatar-mobile');
 await page.getByRole('button',{name:'Close needs',exact:true}).click();
 await delay(180);
 await shot(page,'native-avatar-mobile-scene');
 fixture.disconnect();
 await page.locator('.native-lot[data-native-live="false"]').waitFor();
 const frozen=await canvas.evaluate(c=>c.toDataURL());await delay(450);
 assert.equal(await canvas.evaluate(c=>c.toDataURL()),frozen,'Disconnected avatar must not advance');
 report.checks.push({name:'Disconnect freezes accepted avatar pixels'});
 await page.getByRole('button',{name:'Reconnect',exact:true}).click();
 await page.locator('.native-lot[data-native-live="true"][data-native-avatar-models="1"]').waitFor();
 report.checks.push({name:'Reconnect restores original-format avatar resources'});
 assert.deepEqual(report.errors,[]);
 report.passed=true;
} catch(error) {report.failure=error.stack;process.exitCode=1;
 if(browser)for(const ctx of browser.contexts())for(const p of ctx.pages()){
  if(await p.locator('.native-lot').count()) {
   await p.getByRole('button',{name:'Needs',exact:true}).click().catch(()=>{});
   report.nativeStatus=await p.locator('.native-avatar-status').textContent().catch(()=>null);
   report.sceneryDiagnostics=await p.locator('.world-diagnostics,.source-world-diagnostics').allTextContents();
   report.body=await p.locator('body').innerText();
  }
  await shot(p,'failure').catch(()=>{});
 }
}
finally {if(browser)await browser.close();if(fixture)await fixture.close();gateway.kill();
 await writeFile(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report,null,2));}
