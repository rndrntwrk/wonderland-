// TEST ONLY: real built Rust/WASM, accepted primitive events, real AudioContext
// and an analyser connected to its existing output. No synthetic ACK, audio
// backend replacement, autoplay policy override or browser-clock modification.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdir,readdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {chromium} from 'playwright';
import {createFixture} from './peer.mjs';
import {audioFixture} from './audio-fixture.mjs';
import {presentedCanvas} from './canvas-readiness.mjs';
const output=resolve(process.env.WONDERLAND_NATIVE_QA_OUTPUT??'/tmp/wonderland-native-browser-evidence/game-audio');
await mkdir(output,{recursive:true});
const report={passed:false,scope:'Authored original-format sound fixtures and accepted native primitives; real browser device graph',checks:[],screenshots:[],errors:[]};
const combined=process.env.WONDERLAND_NATIVE_COMBINED_FIXTURE==='1';
let combinedCanvas, initialPixels, retainedPixels, initialRaster;
const delay=ms=>new Promise(r=>setTimeout(r,ms));
const gateway=spawn(resolve('target/debug/examples/controlled_replay'),[],{env:{...process.env,WONDERLAND_REPLAY_BIND:'127.0.0.1:18787',WONDERLAND_REPLAY_BROWSER_ORIGINS:'http://127.0.0.1:18888'},stdio:['ignore','ignore','pipe']});
let browser,fixture,page;
const record=(name,evidence)=>report.checks.push({name,...evidence});
async function shot(name){const file=resolve(output,name+'.png');await page.screenshot({path:file});report.screenshots.push({file:name+'.png',sha256:createHash('sha256').update(await readFile(file)).digest('hex')});}
async function status(){return page.evaluate(async()=>{const {acceptedAudioHost}=await import('/audio/source-audio.mjs');return acceptedAudioHost().snapshot();});}
async function waitVoices(count){await page.waitForFunction(async n=>{const {acceptedAudioHost}=await import('/audio/source-audio.mjs');return acceptedAudioHost().snapshot().activeVoices===n;},count,{timeout:12000});}
async function choose(name){await page.getByRole('button',{name:'Your Sim',exact:true}).click();await page.getByRole('button',{name,exact:true}).click();await page.getByText('Accepted by the server',{exact:true}).waitFor();}
async function closeActions(){const button=page.getByRole('button',{name:'Close source actions',exact:true});if(await button.isVisible())await button.click();}
async function measure(){return page.evaluate(async()=>{
 const {acceptedAudioHost}=await import('/audio/source-audio.mjs');const host=acceptedAudioHost(),backend=host.backend;
 const voice=[...backend._voices.values()].find(v=>v.status==='playing');if(!voice?.panNode)throw Error('No real playing voice');
 const analyser=backend._context.createAnalyser();analyser.fftSize=2048;voice.panNode.connect(analyser);
 await new Promise(r=>setTimeout(r,100));const samples=new Float32Array(analyser.fftSize);analyser.getFloatTimeDomainData(samples);
 voice.panNode.disconnect(analyser);analyser.disconnect();
 return {voiceId:voice.id,rms:Math.sqrt(samples.reduce((n,v)=>n+v*v,0)/samples.length),context:backend._context.constructor.name,state:backend._context.state,gain:voice.gainNode.gain.value,pan:voice.panNode.pan.value,looped:voice.node.loop};
 });}
try{
 let ready=false;for(let i=0;i<100;i++){try{if((await fetch('http://127.0.0.1:18787/health')).ok){ready=true;break;}}catch{}await delay(100);}assert.ok(ready);
 process.env.WONDERLAND_NATIVE_AUDIO_FIXTURE='1';
 if(combined){
  process.env.WONDERLAND_NATIVE_AVATAR_FIXTURE='1';
  process.env.WONDERLAND_NATIVE_TERRAIN_FIXTURE='1';
  process.env.WONDERLAND_NATIVE_AVATAR_BURST='1';
 }
 fixture=await createFixture({dist:resolve('apps/web-shell/dist'),gateway:'http://127.0.0.1:18787',runtimeExecutable:resolve('target/debug/examples/native_browser_peer'),actionDelayTicks:3,manualTicks:combined});
 const paths=await audioFixture(resolve('target/native-audio-fixtures'));
 if(combined){
  const resources=resolve('target/native-avatar-fixtures');
  paths.push(...(await readdir(resources)).filter(f=>!f.endsWith('.json')).map(f=>resolve(resources,f)));
 }
 browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 report.browser=browser.version();page=await browser.newPage({viewport:{width:1200,height:900}});
 page.on('pageerror',error=>report.errors.push(error.message));page.on('console',m=>{if(m.type()==='error')report.errors.push(m.text());});
 await page.goto(fixture.origin);
 await page.getByLabel('Account name',{exact:true}).fill('controlled-player');await page.getByLabel('Password',{exact:true}).fill('test-only');await page.getByRole('button',{name:'Sign in',exact:true}).click();
 await page.getByRole('heading',{name:'Choose your Sim',exact:true}).waitFor();
 await page.locator('.connected-content-details > summary').click();const loader=page.locator('.connected-content-details .content-loader');
 await loader.locator('input[type="file"]').first().setInputFiles(paths);await loader.getByText(/original files read/).waitFor();
 await loader.getByText('Resource names and collection roles',{exact:true}).click();await loader.getByLabel('Head collections',{exact:true}).fill('');await loader.getByLabel('Body collections',{exact:true}).fill('');
 await loader.getByRole('button',{name:'Apply files',exact:true}).click();await loader.getByText(/1 game sound samples/).waitFor({state:'attached'});
 record('Real content UI imports the explicit local HIT/EVT/TRK/PCM/FWAV cohort',{selectedFiles:paths.length});
 await page.getByRole('button',{name:'Play as Controlled Alice',exact:true}).click();await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();
 await page.getByRole('button',{name:'Select Controlled Source Lot',exact:true}).click();await page.getByRole('button',{name:'Visit',exact:true}).click();
 await page.locator('.native-lot[data-native-live="true"]').waitFor();
 if(combined){
  await page.locator('.native-lot[data-native-avatar-models="1"]').waitFor();
  combinedCanvas=page.locator('.native-lot canvas');
  const initialHandle=await page.waitForFunction(presentedCanvas,'.native-lot canvas',{timeout:5000});
  initialRaster=await initialHandle.jsonValue();await initialHandle.dispose();
  initialPixels=initialRaster.pixels;
  const batch=await fixture.burst(18);
  await page.waitForFunction(t=>document.querySelector('#native-tick')?.textContent?.match(/Tick (\d+)/)?.[1]===t,batch.tick);
  await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
  const ended=await fixture.state();
  assert.equal(ended.avatars[0].animations.layers[0].end_reached,true);
  retainedPixels=await combinedCanvas.evaluate(c=>c.toDataURL());
  assert.ok(retainedPixels!==initialPixels,'Combined player must retain intermediate poses from a complete animation packet');
  const point=await combinedCanvas.evaluate(c=>{
   const data=c.getContext('2d').getImageData(0,0,c.width,c.height).data,points=[];
   for(let y=0;y<c.height;y++)for(let x=0;x<c.width;x++){
    const i=(y*c.width+x)*4;if(data[i]>150&&data[i+1]<30&&data[i+2]<30)points.push([x,y]);
   }
   if(!points.length)return null;
   const [x,y]=points[Math.floor(points.length/2)],b=c.getBoundingClientRect();
   return {x:b.left+x*b.width/c.width,y:b.top+y*b.height/c.height,pixels:points.length};
  });
  assert.ok(point?.pixels>0,'Retained mesh remains above the raised terrain');
  await page.mouse.click(point.x,point.y);
  await page.getByRole('heading',{name:'Actions',exact:true}).waitFor();await closeActions();
  record('One admitted player combines raised terrain, mesh picking and all eighteen pose transitions',{tick:batch.tick,canvasSha256:createHash('sha256').update(retainedPixels).digest('hex'),visiblePixels:point.pixels});
  await fixture.setHidden(2);await page.locator('.native-lot[data-native-avatar-models="0"]').waitFor();
  await page.waitForFunction(()=>{
   const lot=document.querySelector('.native-lot[data-native-avatar-models="0"]');
   const view=lot?.querySelector('.world-viewport'),c=view?.querySelector('canvas');
   if(!c||view.classList.contains('world-busy'))return false;
   const rgba=c.getContext('2d').getImageData(0,0,c.width,c.height).data;
   for(let i=0;i<rgba.length;i+=4)if(rgba[i]>150&&rgba[i+1]<30&&rgba[i+2]<30)return false;
   return true;
  },null,{timeout:5000});
  await page.mouse.click(point.x,point.y);await delay(100);
  assert.equal(await page.getByRole('heading',{name:'Actions',exact:true}).count(),0);
  await fixture.setHidden(0);await page.locator('.native-lot[data-native-avatar-models="1"]').waitFor();
  await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
  assert.equal(await combinedCanvas.evaluate(c=>c.toDataURL()),retainedPixels,'Hidden/restored state retains the accepted ended pose');
  record('Terrain visibility repair coexists with retained pose history',{hiddenValue:2,restored:true});
 }
 await page.getByRole('button',{name:'Needs',exact:true}).click();await page.getByText(/Game sounds ready/).waitFor();
 await choose('Play sound (test harness)');assert.equal(await page.getByRole('button',{name:'Close needs',exact:true}).count(),0,'Source action selection closes the overlapping Needs inspector');await delay(200);assert.equal((await status()).activeVoices,0);record('Unactivated playback discards accepted cues, not game actions',{accepted:fixture.stats.accepted,voices:0});
 await closeActions();await page.getByRole('button',{name:'Sound settings',exact:true}).click();
 await page.getByRole('button',{name:'Enable sound',exact:true}).click();await page.getByRole('button',{name:'Sound enabled',exact:true}).waitFor();
 await page.getByRole('button',{name:'Close sound settings',exact:true}).click();await delay(100);assert.equal((await status()).activeVoices,0);record('Real gesture unlock does not replay pre-activation history',{voices:0});
 await choose('Play sound (test harness)');await waitVoices(1);await delay(150);
 const audible=await measure();assert.equal(audible.context,'AudioContext');assert.ok(audible.rms>0.05,JSON.stringify(audible));assert.equal(audible.looped,true);record('Accepted source primitive reaches real nonzero WebAudio output',audible);
 // A real foreground main-thread task delays setInterval. This does not hide
 // the document, alter clocks, send commands or replace audio/device state.
 const delayEvidence=await page.evaluate(()=>{
   const start=performance.now(), visibility=document.visibilityState;
   while(performance.now()-start<400) { /* real event-loop load */ }
   return {visibility, elapsedMs:performance.now()-start};
 });
 await delay(100);
 const afterForegroundDelay=await status();
 assert.equal(delayEvidence.visibility,'visible');
 assert.equal(afterForegroundDelay.state,'running');
 assert.equal(afterForegroundDelay.activeVoices,1,'Visible event-loop delay must not discard an accepted looping voice');
 const afterDelaySignal=await measure();
 assert.equal(afterDelaySignal.voiceId,audible.voiceId,'The existing accepted voice must survive, not a restarted substitute');
 assert.ok(afterDelaySignal.rms>0.05);
 assert.equal(fixture.stats.actions,2,'Foreground scheduling must not resubmit gameplay');
 record('Visible event-loop delay preserves the active game sound',{...delayEvidence, voices:afterForegroundDelay.activeVoices});
 await shot('01-native-game-sound');
 await closeActions();await page.getByRole('button',{name:'Sound settings',exact:true}).click();
 await page.getByLabel('Mute all sound',{exact:true}).check();const muted=await measure();assert.ok(muted.rms<0.000001);record('Existing mute controls silence the actual game voice',muted);
 await page.getByLabel('Mute all sound',{exact:true}).uncheck();
 const volume=page.getByLabel('Effects volume',{exact:true});await volume.focus();await volume.press('Home');for(let i=0;i<25;i++)await volume.press('ArrowRight');const quiet=await measure();assert.ok(quiet.rms<audible.rms*0.35&&quiet.rms>audible.rms*0.15);assert.equal(quiet.voiceId,audible.voiceId);record('Existing effects volume scales real game output',quiet);
 await volume.press('End');
 await page.getByRole('button',{name:'Pause sound',exact:true}).click();await waitVoices(0);
 assert.equal((await status()).state,'suspended');
 record('Real device suspension still disposes native playback',{voices:0,state:'suspended'});
 await page.getByRole('button',{name:'Resume sound',exact:true}).click();
 await page.getByRole('button',{name:'Sound enabled',exact:true}).waitFor();await delay(100);
 assert.equal((await status()).activeVoices,0,'Resuming must not replay discarded sound history');
 record('Resuming the real device does not replay old cues',{voices:0});
 await page.getByRole('button',{name:'Close sound settings',exact:true}).click();
 await choose('Play sound (test harness)');await waitVoices(1);
 await choose('Stop sound (test harness)');await waitVoices(0);record('Accepted StopSound releases the exact owning sound',{voices:0});
 if(combined){
  assert.equal(await combinedCanvas.evaluate(c=>c.toDataURL()),retainedPixels,'Sound actions and foreground load cannot reset an ended pose');
  record('Sound actions and UI scheduling preserve the retained terrain-backed avatar',{sameCanvas:true});
 }
 await choose('Play sound (test harness)');await waitVoices(1);await closeActions();
 const before=fixture.stats.actions;fixture.disconnect();await page.locator('.native-lot[data-native-live="false"]').waitFor();await waitVoices(0);
 assert.equal((await status()).loadedSamples,0);record('Disconnect releases voices and imported native resource residency',{voices:0,samples:0,actions:before});
 await page.getByRole('button',{name:'Reconnect',exact:true}).click();await page.locator('.native-lot[data-native-live="true"]').waitFor();await page.waitForFunction(async()=>{const {acceptedAudioHost}=await import('/audio/source-audio.mjs');return acceptedAudioHost().snapshot().loadedSamples===1;});await delay(400);
 assert.equal((await status()).activeVoices,0);assert.equal(fixture.stats.actions,before);record('Recovery checkpoint does not replay sound or commands',{voices:0,actions:fixture.stats.actions});
 if(combined){
  await page.locator('.native-lot[data-native-avatar-models="1"]').waitFor();
  const recoveredHandle=await page.waitForFunction(presentedCanvas,'.native-lot canvas',{timeout:5000});
  const recovered=await recoveredHandle.jsonValue();await recoveredHandle.dispose();
  assert.deepEqual([recovered.width,recovered.height],[initialRaster.width,initialRaster.height],
    'Recovery comparison must use the same displayed raster dimensions');
  assert.equal(createHash('sha256').update(recovered.pixels).digest('hex'),
    createHash('sha256').update(initialPixels).digest('hex'),
    'Fresh checkpoint explicitly resets unavailable historical bones');
  record('Combined reconnect retains resources but never invents missing pose or sound history',{avatarModels:1,soundVoices:0,raster:[initialRaster.width,initialRaster.height]});
 }
 await choose('Play sound (test harness)');await waitVoices(1);const restored=await measure();assert.ok(restored.rms>0.05);record('An explicit new action plays after reconnect',restored);
 await closeActions();await page.setViewportSize({width:390,height:844});await shot('02-mobile-native-game-sound');
 await page.getByRole('button',{name:'Return to city',exact:true}).click();await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();await waitVoices(0);assert.equal((await status()).loadedSamples,0);
 record('Route exit disposes native voices and samples',{voices:0,samples:0});await shot('03-city-after-game-audio');
 assert.deepEqual(report.errors,[]);report.stats={...fixture.stats};report.passed=true;
}catch(error){report.errors.push(error.stack??String(error));if(page){report.body=await page.locator('body').innerText().catch(()=>null);report.audio=await status().catch(()=>null);await shot('failure').catch(()=>{});}throw error;
}finally{await browser?.close();await fixture?.close();gateway.kill();await writeFile(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report,null,2));}
