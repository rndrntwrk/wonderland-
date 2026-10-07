// TEST ONLY: real accepted non-looping animation, original-format synthetic mesh.
// The product build/timers are unchanged. The authority owns end-of-animation.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdir,readdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {chromium} from 'playwright';
import {createFixture} from './peer.mjs';

const output=resolve(process.env.WONDERLAND_NATIVE_QA_OUTPUT??'/tmp/wonderland-native-pose-evidence');
const resources=resolve(process.env.WONDERLAND_AVATAR_QA_FILES??'target/native-avatar-fixtures');
const report={passed:false,scope:'Presented bone-channel retention, real Rust authority, synthetic Vitaboy resources',checks:[],screenshots:[],errors:[]};
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
const record=(name,evidence)=>{report.checks.push({name,...evidence});console.log(name,JSON.stringify(evidence));};
await mkdir(output,{recursive:true});
const gateway=spawn(resolve('target/debug/examples/controlled_replay'),[],{
 env:{...process.env,WONDERLAND_REPLAY_BIND:'127.0.0.1:19387',WONDERLAND_REPLAY_BROWSER_ORIGINS:'http://127.0.0.1:19388'},stdio:['ignore','ignore','pipe']});
let gatewayLog='',browser,fixture;
gateway.stderr.on('data',data=>{if(gatewayLog.length<64000)gatewayLog+=data;});
async function shot(page,name){const file=resolve(output,name+'.png');await page.screenshot({path:file});report.screenshots.push({file:name+'.png',sha256:digest(await readFile(file))});}
async function accepted(page,tick){
 await page.waitForFunction(tick=>{
  const value=document.querySelector('#native-tick')?.textContent?.match(/Tick (\d+)/)?.[1];
  return value!==undefined&&BigInt(value)>=BigInt(tick);
 },tick);
}
async function untilState(predicate){
 for(let i=0;i<180;i++){const state=await fixture.state();if(predicate(state))return state;await delay(100);}
 throw Error('The controlled authority did not reach the required animation state');
}
const animation=state=>state.avatars[0].animations.layers[0];
try{
 let healthy=false;
 for(let i=0;i<100;i++){try{if((await fetch('http://127.0.0.1:19387/health')).ok){healthy=true;break;}}catch{}await delay(100);}
 assert.ok(healthy,'Controlled gateway failed to start');
 process.env.WONDERLAND_NATIVE_AVATAR_FIXTURE='1';
 process.env.WONDERLAND_NATIVE_AVATAR_ONCE='1';
 fixture=await createFixture({dist:resolve(process.env.WONDERLAND_NATIVE_QA_DIST??'apps/web-shell/dist'),gateway:'http://127.0.0.1:19387',runtimeExecutable:resolve('target/debug/examples/native_browser_peer'),port:19388});
 browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 report.browser=browser.version();
 const page=await browser.newPage({viewport:{width:1200,height:900}});
 page.on('pageerror',error=>report.errors.push(error.message));
 page.on('console',message=>{if(message.type()==='error')report.errors.push(message.text());});
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
 const canvas=page.locator('.native-lot canvas');await canvas.waitFor();
 const moving=new Set();
 for(let i=0;i<5;i++){moving.add(await canvas.evaluate(c=>c.toDataURL()));await delay(120);}
 assert.ok(moving.size>1,'The witness must first observe an actually moving non-bind avatar');
 record('Real content and accepted one-shot animation are visible',{models:1,distinctFrames:moving.size});
 const plateau=await untilState(s=>animation(s).current_frame>=1.125&&!animation(s).end_reached);
 assert.equal(animation(plateau).looping,false);await accepted(page,plateau.tick);await delay(150);
 const beforeEnd=await canvas.evaluate(c=>c.toDataURL());
 await shot(page,'01-before-animation-end');
 const end=await untilState(s=>animation(s).end_reached);
 await accepted(page,(BigInt(end.tick)+2n).toString());await delay(120);
 const afterEnd=await canvas.evaluate(c=>c.toDataURL());
 assert.ok(afterEnd===beforeEnd,`POSE_RETENTION_ASSERTION: ended clip reset its sampled skeleton (${digest(beforeEnd)} -> ${digest(afterEnd)})`);
 const frames=new Set([afterEnd]);
 for(let i=0;i<8;i++){await delay(100);frames.add(await canvas.evaluate(c=>c.toDataURL()));}
 assert.equal(frames.size,1,'Later accepted idle ticks must not drift retained channels');
 record('EndReached and subsequent live ticks retain exactly the prior canvas pixels',{
  activeTick:plateau.tick,endedTick:end.tick,lastSourceFrame:animation(end).current_frame,
  canvasSha256:digest(afterEnd),laterFrames:frames.size,
 });
 await shot(page,'02-retained-ended-pose');
 // Ordinary camera changes use the retained mesh; returning to the same view is
 // an additional same-sample re-render, not another blend into the skeleton.
 await page.getByRole('button',{name:'Rotate right',exact:true}).click();await delay(150);
 assert.notEqual(await canvas.evaluate(c=>c.toDataURL()),afterEnd);
 await page.getByRole('button',{name:'Rotate left',exact:true}).click();await delay(150);
 assert.equal(await canvas.evaluate(c=>c.toDataURL()),afterEnd);
 record('Camera re-render preserves retained geometry without accumulated blending',{});
 await page.getByRole('button',{name:'Your Sim',exact:true}).click();
 await page.locator('.native-source-action').first().click();
 await page.getByText('Accepted by the server',{exact:true}).waitFor();
 assert.equal(fixture.stats.accepted,1);
 record('An ended visual clip does not block real source actions or receipts',{acceptedActions:1});
 await page.getByRole('button',{name:'Close source actions',exact:true}).click();
 await page.setViewportSize({width:390,height:844});await delay(150);await shot(page,'03-retained-mobile');
 fixture.disconnect();await page.locator('.native-lot[data-native-live="false"]').waitFor();
 const frozen=await canvas.evaluate(c=>c.toDataURL());await delay(250);assert.equal(await canvas.evaluate(c=>c.toDataURL()),frozen);
 await page.getByRole('button',{name:'Reconnect',exact:true}).click();
 await page.locator('.native-lot[data-native-live="true"][data-native-avatar-models="1"]').waitFor();await delay(250);
 assert.notEqual(await canvas.evaluate(c=>c.toDataURL()),frozen,'New checkpoint has no serialized visual bone history; old channels must not leak across it');
 assert.equal(fixture.stats.actions,1);
 record('Disconnect freezes history and a fresh checkpoint resets unavailable prior bones',{modelsAfterRecovery:1,automaticRetries:0});
 await shot(page,'04-checkpoint-reset');
 await page.getByRole('button',{name:'Return to city',exact:true}).click();
 await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();
 for(let i=0;i<30&&fixture.active()!==0;i++)await delay(50);
 assert.equal(fixture.active(),0);assert.equal(await page.locator('.native-lot canvas').count(),0);
 assert.deepEqual(report.errors,[]);
 record('Route exit releases the retained avatar lifetime',{activeNativeSockets:0,pageErrors:0,consoleErrors:0});
 report.passed=true;
}catch(error){report.failure=error.stack;process.exitCode=1;console.error(error);if(browser)for(const context of browser.contexts())for(const page of context.pages())await shot(page,'failure').catch(()=>{});}
finally{
 await browser?.close();await fixture?.close();gateway.kill();
 await writeFile(resolve(output,'gateway.log'),gatewayLog);
 await writeFile(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');
}
