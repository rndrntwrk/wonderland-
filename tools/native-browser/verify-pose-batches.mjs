import {canvasPixels} from './canvas-evidence.mjs';
// Real built browser + Rust authority: the entire animation may end inside one
// accepted packet. No synthetic DOM, altered product WASM or injected game state.
// Test geometry/timeline setup is explicitly the standalone Vitaboy fixture.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdir,readdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {chromium} from 'playwright';
import {createFixture} from './peer.mjs';

const output=resolve(process.env.WONDERLAND_NATIVE_QA_OUTPUT??'/tmp/wonderland-native-pose-batches');
const resources=resolve(process.env.WONDERLAND_AVATAR_QA_FILES??'target/native-avatar-fixtures');
const report={passed:false,scope:'Every accepted intermediate pose in actual browser; synthetic original-format geometry',checks:[],screenshots:[],errors:[],batches:[]};
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
const record=(name,evidence)=>{report.checks.push({name,...evidence});console.log(name,JSON.stringify(evidence));};
await mkdir(output,{recursive:true});
const gateway=spawn(resolve('target/debug/examples/controlled_replay'),[],{
 env:{...process.env,WONDERLAND_REPLAY_BIND:'127.0.0.1:19687',WONDERLAND_REPLAY_BROWSER_ORIGINS:'http://127.0.0.1:19688'},stdio:['ignore','ignore','pipe']});
let gatewayLog='',browser,fixture;
gateway.stderr.on('data',data=>{if(gatewayLog.length<64000)gatewayLog+=data;});
async function shot(page,name){const file=resolve(output,name+'.png');await page.screenshot({path:file});report.screenshots.push({file:name+'.png',sha256:digest(await readFile(file))});}
async function accepted(page,tick){
 await page.waitForFunction(tick=>document.querySelector('#native-tick')?.textContent?.match(/Tick (\d+)/)?.[1]===tick,tick);
 // Let the real reactive/renderer scheduling complete without changing its clock.
 await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
}
async function enter(page){
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
 await page.getByRole('button',{name:'Visit',exact:true}).click();
 await page.locator('.native-lot[data-native-live="true"][data-native-avatar-models="1"]').waitFor();
 await accepted(page,(await fixture.state()).tick);
}
try{
 let healthy=false;
 for(let i=0;i<100;i++){try{if((await fetch('http://127.0.0.1:19687/health')).ok){healthy=true;break;}}catch{}await delay(100);}
 assert.ok(healthy,'Controlled gateway failed to start');
 process.env.WONDERLAND_NATIVE_AVATAR_FIXTURE='1';
 process.env.WONDERLAND_NATIVE_AVATAR_BURST='1';
 browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 report.browser=browser.version();
 let referencePixels,referenceHash;
 for(const batchSize of [1,3,18]){
  fixture=await createFixture({dist:resolve(process.env.WONDERLAND_NATIVE_QA_DIST??'apps/web-shell/dist'),gateway:'http://127.0.0.1:19687',runtimeExecutable:resolve('target/debug/examples/native_browser_peer'),port:19688,manualTicks:true});
  const context=await browser.newContext({viewport:{width:1200,height:900}});
  const page=await context.newPage();
  page.on('pageerror',error=>report.errors.push(error.message));
  page.on('console',message=>{if(message.type()==='error')report.errors.push(message.text());});
  await enter(page);
  const canvas=page.locator('.native-lot canvas');
  const initialState=await fixture.state();
  assert.equal(initialState.avatars[0].animations.layers[0].current_frame,0);
  assert.equal(initialState.avatars[0].animations.layers[0].looping,false);
  const initialPixels=await canvasPixels(canvas);
  const initialTick=BigInt(initialState.tick),packets=[];
  for(let n=0;n<18;n+=batchSize){
   const result=await fixture.burst(batchSize);packets.push(result);
   await accepted(page,result.tick);
  }
  const ended=await fixture.state();
  assert.equal(BigInt(ended.tick),initialTick+18n);
  assert.equal(ended.avatars[0].animations.layers[0].end_reached,true);
  assert.equal(ended.avatars[0].animations.layers[0].current_frame,2);
  assert.equal(fixture.stats.bursts,18/batchSize);
  const retained=await canvasPixels(canvas);
  assert.notEqual(retained,initialPixels,'POSE_BATCH_ASSERTION: the final-only sample lost the preceding bone transforms');
  if(referencePixels===undefined){referencePixels=retained;referenceHash=ended.hash;}
  else {assert.equal(retained,referencePixels,'Packet grouping changed actual rendered pixels');assert.equal(ended.hash,referenceHash,'Packet grouping changed authority state');}
  await shot(page,`01-retained-batch-${batchSize}`);
  for(let n=0;n<3;n++){
   await accepted(page,(await fixture.burst(batchSize)).tick);
   assert.equal(await canvasPixels(canvas),retained,'Accepted idle ticks drifted the retained pose');
  }
  record('All intermediate accepted poses survive packet grouping',{batchSize,acceptedFrames:18,packets:packets.length,canvasSha256:digest(retained),authorityHash:ended.hash});
  report.batches.push({batchSize,initialTick:initialState.tick,finalTick:ended.tick,packets,canvasSha256:digest(retained),authorityHash:ended.hash});
  await page.getByRole('button',{name:'Rotate right',exact:true}).click();await delay(100);
  assert.notEqual(await canvasPixels(canvas),retained);
  await page.getByRole('button',{name:'Rotate left',exact:true}).click();await delay(100);
  assert.equal(await canvasPixels(canvas),retained);
  await page.getByRole('button',{name:'Your Sim',exact:true}).click();
  await page.locator('.native-source-action').first().click();
  await page.getByText('Accepted by the server',{exact:true}).waitFor();
  assert.equal(fixture.stats.actions,1);assert.equal(fixture.stats.accepted,1);
  await page.getByRole('button',{name:'Close source actions',exact:true}).click();
  record('Source actions and camera controls remain usable after the batched animation ends',{batchSize,acceptedActions:1});
  if(batchSize===18){
   fixture.disconnect();await page.locator('.native-lot[data-native-live="false"]').waitFor();
   const frozen=await canvasPixels(canvas);await delay(200);assert.equal(await canvasPixels(canvas),frozen);
   await page.getByRole('button',{name:'Reconnect',exact:true}).click();
   await page.locator('.native-lot[data-native-live="true"][data-native-avatar-models="1"]').waitFor();
   await accepted(page,(await fixture.state()).tick);
   assert.equal(await canvasPixels(canvas),initialPixels,'Checkpoint without pose history must deliberately reseed the bind pose');
   assert.equal(fixture.stats.actions,1);
   await shot(page,'02-batched-checkpoint-reset');
   record('Recovery resets unavailable bone history without retrying the accepted operation',{batchSize,actions:1});
  }
  await page.getByRole('button',{name:'Return to city',exact:true}).click();
  await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();
  for(let n=0;n<30&&fixture.active()!==0;n++)await delay(50);
  assert.equal(fixture.active(),0);assert.equal(await page.locator('.native-lot canvas').count(),0);
  await context.close();await fixture.close();fixture=undefined;
 }
 assert.deepEqual(report.errors,[]);
 record('All packet sizes agree and all isolated browser sessions dispose cleanly',{equalBatchSizes:[1,3,18],pageErrors:0,consoleErrors:0});
 report.passed=true;
}catch(error){report.failure=error.stack;process.exitCode=1;console.error(error);if(browser)for(const context of browser.contexts())for(const page of context.pages())await shot(page,'failure').catch(()=>{});}
finally{
 await browser?.close();await fixture?.close();gateway.kill();
 await writeFile(resolve(output,'gateway.log'),gatewayLog);
 await writeFile(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');
}
