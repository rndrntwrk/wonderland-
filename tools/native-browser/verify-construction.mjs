// TEST ONLY: the actual built UI, controlled authenticated gateway and real
// Rust build/replica engines. Test prices and RAM receipts are not a database.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {chromium} from 'playwright';
import {createFixture} from './peer.mjs';
import {presentedCanvas} from './canvas-readiness.mjs';
import {observeNativeCanvas} from './canvas-observation.mjs';
const output=resolve(process.env.WONDERLAND_NATIVE_QA_OUTPUT??'/tmp/wonderland-native-construction');
const dist=resolve(process.env.WONDERLAND_NATIVE_QA_DIST??'apps/web-shell/dist');
await mkdir(output,{recursive:true});
const results={passed:false,environment:{runner:'Playwright 1.56.1',scope:'Actual native Buy/Build UI; authenticated controlled gateway; authored catalogue and RAM-only completion fixture'},checks:[],screenshots:[],errors:[],warnings:[]};
const record=(name,evidence)=>{results.checks.push({name,evidence});console.log(name,JSON.stringify(evidence));};
const delay=ms=>new Promise(r=>setTimeout(r,ms));
const gateway=spawn(resolve('target/debug/examples/controlled_replay'),[],{env:{...process.env,WONDERLAND_REPLAY_BIND:'127.0.0.1:18787',WONDERLAND_REPLAY_BROWSER_ORIGINS:'http://127.0.0.1:18888'},stdio:['ignore','ignore','pipe']});
let gatewayLog='';gateway.stderr.on('data',d=>{if(gatewayLog.length<64000)gatewayLog+=d;});
let fixture,browser,page,expectedNetworkFailure=false;
async function capture(name){const path=resolve(output,name+'.png');await page.screenshot({path,animations:'disabled'});results.screenshots.push({file:name+'.png',sha256:createHash('sha256').update(await readFile(path)).digest('hex')});}
async function enter(){
 await page.goto(fixture.origin);
 await page.getByLabel('Account name',{exact:true}).fill('controlled-player');await page.getByLabel('Password',{exact:true}).fill('test-only');
 await page.getByRole('button',{name:'Sign in',exact:true}).click();await page.getByRole('heading',{name:'Choose your Sim',exact:true}).waitFor();
 await page.getByRole('button',{name:'Play as Controlled Alice',exact:true}).click();await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();
 await page.getByRole('button',{name:'Select Controlled Source Lot',exact:true}).click();await page.getByRole('heading',{name:'Property',exact:true}).waitFor();
 await page.getByRole('button',{name:'Visit',exact:true}).click();await page.locator('.native-lot[data-native-live="true"]').waitFor({timeout:30000});
 await page.waitForFunction(presentedCanvas,'.native-lot canvas');
}
async function stage(name){await page.locator(`.native-build-panel[data-build-stage="${name}"]`).waitFor({timeout:15000});}
async function chooseTile(index=0,touch=false){
 await page.waitForFunction(presentedCanvas,'.native-lot canvas');
 const canvas=page.locator('.native-lot canvas');const bounds=await canvas.boundingBox();
 // Real canvas clicks only. Probe several ground regions without injecting
 // a WorldPick or modifying game state; the UI reports the tile it selected.
 const probes=index===0?[[.45,.56],[.40,.56],[.50,.63],[.35,.60],[.48,.5]]:[[.30,.45],[.36,.46],[.40,.42],[.46,.42],[.27,.52]];
 for(const [x,y] of probes){
  if(touch)await page.touchscreen.tap(bounds.x+bounds.width*x,bounds.y+bounds.height*y);else await page.mouse.click(bounds.x+bounds.width*x,bounds.y+bounds.height*y);
  try{
   // GPU tile selection is asynchronous. Waiting only for the already-visible
   // disabled button cancels each pending pick with the next probe.
   await page.waitForFunction(()=>[...document.querySelectorAll('.native-build-panel button')].some(b=>b.textContent.trim()==='Review placement & price'&&!b.disabled),null,{timeout:800});
   return;
  }catch{}
 }
 throw Error('No real selectable property tile found');
}
async function refresh(){await page.getByRole('button',{name:'Refresh catalogue and balance',exact:true}).click();await page.waitForFunction(()=>!document.querySelector('.native-build-notice')?.textContent.includes('Loading'));await page.getByRole('button',{name:'Refresh catalogue and balance',exact:true}).waitFor();}
async function quote(item,price,index=0,rotate=false){
 if(!await page.getByRole('button',{name:`Select ${item}`,exact:true}).isVisible())await page.getByRole('button',{name:'Change item',exact:true}).click();
 await page.getByRole('button',{name:`Select ${item}`,exact:true}).click();await chooseTile(index);
 if(rotate)await page.getByRole('button',{name:'Rotate item',exact:true}).click();
 const placement=await page.locator('.native-build-selection p').textContent();
 await page.getByRole('button',{name:'Review placement & price',exact:true}).click();await stage('Reviewing');
 await page.getByRole('button',{name:`Confirm $${price}`,exact:true}).waitFor();
 return placement;
}
function purchases(state){return state.world.objects.filter(o=>o.reference.object_id!==1);}
try{
 for(let n=0;n<100;n++){try{if((await fetch('http://127.0.0.1:18787/health')).ok)break;}catch{}if(n===99)throw Error('Gateway unavailable');await delay(50);}
 fixture=await createFixture({dist,gateway:'http://127.0.0.1:18787',runtimeExecutable:resolve('target/debug/examples/native_browser_peer'),construction:true});
 browser=await chromium.launch({headless:true,args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 results.environment.chromium=browser.version();
 page=await browser.newPage({viewport:{width:1440,height:1000},reducedMotion:'reduce',hasTouch:true});
 page.on('pageerror',e=>results.errors.push({type:'pageerror',message:e.message}));
 page.on('console',m=>{if(m.type()==='warning')results.warnings.push(m.text());else if(m.type()==='error'){if(expectedNetworkFailure&&/net::ERR_EMPTY_RESPONSE/.test(m.text()))results.warnings.push('Expected deliberately lost construction response');else results.errors.push({type:'console',message:m.text()});}});
 await observeNativeCanvas(page);
 await enter();await page.getByRole('button',{name:'Buy / Build',exact:true}).click();
 await page.getByRole('button',{name:'Select Construction test plinth',exact:true}).waitFor();
 assert.equal(await page.locator('.native-build-item').count(),8);assert.equal(await page.getByRole('button',{name:'Next',exact:true}).textContent(),'Next');
 assert.match(await page.locator('.native-build-balance').textContent(),/1000/);await capture('01-catalogue-desktop');
 record('Actual catalogue mounted in native lot',{itemsOnPage:8,totalItems:13,literalCodeInNext:false});
 await page.getByRole('button',{name:'Next',exact:true}).click();assert.equal(await page.locator('.native-build-item').count(),5);
 await page.getByRole('button',{name:'Previous',exact:true}).click();assert.equal(await page.locator('.native-build-item').count(),8);
 await page.waitForFunction(()=>[...document.querySelectorAll('.native-build-item img')].slice(0,2).every(i=>i.complete&&i.naturalWidth>0));
 record('Real pagination and same-origin catalogue thumbnails',{pages:2,secondPage:5,loadedThumbnails:true});
 await page.getByLabel('Search catalogue',{exact:true}).fill('not-present');await page.getByText('No matching catalogue items are available.',{exact:true}).waitFor();
 assert.equal(await page.locator('.native-build-item').count(),0);await page.getByLabel('Search catalogue',{exact:true}).fill('');
 await page.getByRole('button',{name:'Floors',exact:true}).click();assert.equal(await page.locator('.native-build-item').count(),1);
 await page.getByRole('button',{name:'All',exact:true}).click();record('Search and categories keep real unavailable results',{emptyResults:true,floorCount:1});
 const before=await fixture.state();assert.equal(before.world.objects.length,0);record('Initial controlled world',{objects:0,builds:0});
 const placement=await quote('Construction test plinth',200,0,true);assert.equal(fixture.stats.constructionBuilds,0);await capture('02-quote-desktop');
 fixture.holdNextConstruction();await page.getByRole('button',{name:'Confirm $200',exact:true}).click();await stage('Pending');
 assert.equal((await fixture.state()).world.objects.length,0);assert.equal(fixture.stats.constructionBuilds,1);
 assert.equal(await page.getByRole('button',{name:'Confirm $200',exact:true}).count(),0);
 await capture('03-pending-without-object');record('Admission alone does not place an object',{stage:'Pending',objects:0,admissions:1});
 await fixture.completeConstruction();await stage('Committed');
 await page.waitForFunction(()=>document.querySelector('.native-build-balance')?.textContent.includes('$800'));
 const committed=await fixture.state();assert.equal(committed.construction_builds,1);await capture('03-committed-desktop');
 record('Exact-price confirmation commits one native object',{builds:committed.construction_builds,world:committed.world});
 const [_,chosenX,chosenY]=placement.match(/Selected tile (\d+), (\d+)/);
 assert.equal(committed.world.objects[0].position.x,(Number(chosenX)-1)*16+8);assert.equal(committed.world.objects[0].position.y,(Number(chosenY)-1)*16+8);
 assert.equal(committed.world.objects[0].facing,2,'Rotate item preserves the native EAST facing');
 assert.equal(committed.world.objects.length,1);assert.equal(committed.world.objects[0].guid,123);assert.ok(committed.world.objects[0].persistent_id>0);assert.equal(committed.construction_balance,800);
 // Checkpoint restoration observes the same accepted object, without admission.
 const purchased=committed.world.objects[0];await page.getByRole('button',{name:'Reconnect',exact:true}).click();
 await page.locator('.native-lot[data-native-live="true"]').waitFor();await page.waitForFunction(presentedCanvas,'.native-lot canvas');
 assert.deepEqual((await fixture.state()).world.objects[0],purchased);assert.equal(fixture.stats.constructionBuilds,1);
 await refresh();await page.waitForFunction(()=>!document.querySelector('.native-build-item')?.disabled);
 record('Purchased object survives native reconnect',{objects:1,admissions:1,persistentId:purchased.persistent_id});
 // The quote remains a pure operation until the player confirms it.
 await quote('Construction test floor',7,1);await page.getByRole('button',{name:'Cancel quote',exact:true}).click();await stage('Cancelled');
 assert.equal(fixture.stats.constructionBuilds,1);assert.equal((await fixture.state()).construction_balance,800);
 record('Explicit cancellation abandons only the unsubmitted quote',{admissions:1,balance:800});
 await page.getByRole('button',{name:'Review placement & price',exact:true}).click();await stage('Reviewing');
 await page.getByRole('button',{name:'Confirm $7',exact:true}).click();await stage('Committed');
 await page.waitForFunction(()=>document.querySelector('.native-build-balance')?.textContent.includes('$793'));
 assert.equal(fixture.stats.constructionBuilds,2);const floor=await fixture.state();assert.ok(floor.world.architecture_revision>0);
 await capture('04-floor-committed');record('Floor painting uses actual native commitment',{architecture:floor.world.architecture_revision,balance:floor.construction_balance});
 // Lose both the HTTP confirmation and tick delivery after the authority commits.
 await quote('Construction test block',350,1);fixture.dropNextConstructionReply();expectedNetworkFailure=true;
 await page.getByRole('button',{name:'Confirm $350',exact:true}).click();await stage('Unknown');
 const calls=fixture.stats.constructionCalls;assert.equal(fixture.stats.constructionBuilds,3);
 await page.getByRole('button',{name:'Reconnect',exact:true}).click();await page.locator('.native-lot[data-native-live="true"]').waitFor();
 assert.equal(fixture.stats.constructionCalls,calls,'Reconnect must not resend the confirmation');
 await page.getByRole('button',{name:'Recheck operation status',exact:true}).click();await stage('Committed');
 await page.waitForFunction(()=>document.querySelector('.native-build-balance')?.textContent.includes('$443'));
 assert.equal(fixture.stats.constructionCalls,calls+1,'Only one read-only status call is dispatched');
 const recovered=await fixture.state();assert.equal(recovered.world.objects.length,2);assert.equal(recovered.construction_balance,443);
 assert.equal(recovered.world.objects.filter(o=>o.guid===124).length,1);assert.equal(fixture.stats.constructionBuilds,3);
 expectedNetworkFailure=false;await capture('05-recovered-confirmation');
 record('Unknown confirmation recovers without double debit or duplicate object',{admissions:3,objects:2,balance:443,statusCalls:1});
 // Independent panels remain reachable; they may not obscure confirmation.
 await page.getByRole('button',{name:'Needs',exact:true}).click();assert.equal(await page.locator('.native-build-panel').count(),0);
 await page.getByRole('button',{name:'Close needs',exact:true}).click();await page.getByRole('button',{name:'Buy / Build',exact:true}).click();
 record('Needs and construction inspectors do not overlap',{needsClosesBuild:true});
 for(const [width,height] of [[390,844],[320,600],[844,390],[1440,1000]]){
  await page.setViewportSize({width,height});await page.waitForFunction(presentedCanvas,'.native-lot canvas');
  await page.locator('.native-build-panel').evaluate(p=>p.scrollTop=0);
  const b=await page.locator('.native-build-panel').boundingBox();assert.ok(b&&b.x>=0&&b.y>=0&&b.x+b.width<=width+1&&b.y+b.height<=height+1,JSON.stringify(b));
  assert.ok(!await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),'No page-wide horizontal overflow');
  const map=await page.locator('.native-lot canvas').boundingBox();
  const overlapX=Math.min(map.x+map.width,b.x+b.width)-Math.max(map.x,b.x),overlapY=Math.min(map.y+map.height,b.y+b.height)-Math.max(map.y,b.y);
  assert.ok(overlapX<=1||overlapY<=1,'The placement viewport is obscured by the construction drawer');
  const button=await page.getByRole('button',{name:'Close Buy and Build',exact:true}).boundingBox();assert.ok(button.width>=32&&button.height>=32);
  await capture(`06-catalogue-${width}x${height}`);record('Responsive construction panel',{width,height,bounds:b,closeTarget:button.width});
 }
 // A real touchscreen event selects a free tile; no synthetic WorldPick is injected.
 await page.setViewportSize({width:390,height:844});await page.waitForFunction(presentedCanvas,'.native-lot canvas');
 await page.getByRole('button',{name:'Select Construction test floor',exact:true}).tap();await chooseTile(0,true);
 await page.getByRole('button',{name:'Review placement & price',exact:true}).tap();await stage('Reviewing');
 await page.getByRole('button',{name:'Confirm $7',exact:true}).waitFor();await capture('07-touch-exact-quote');
 await page.getByRole('button',{name:'Cancel quote',exact:true}).tap();await stage('Cancelled');assert.equal(fixture.stats.constructionBuilds,3);
 record('Touch selection reaches exact quote without accidental confirmation',{touchscreen:true,quote:7,cancelled:true,admissions:3});
 // Leaving the lot disposes its construction UI; no further calls occur.
 await page.getByRole('button',{name:'Return to city',exact:true}).click();await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();
 assert.equal(await page.locator('.native-build-panel').count(),0);const finalCalls=fixture.stats.constructionCalls;await delay(250);assert.equal(fixture.stats.constructionCalls,finalCalls);
 record('Construction resources leave with the lot',{panelRemoved:true,noLateCalls:true});

 assert.deepEqual(results.errors,[]);results.passed=true;
}catch(error){results.errors.push({type:'assertion',message:error.stack});try{if(page)await capture('failure');}catch{}console.error(error);process.exitCode=1;}
finally{
 if(browser)await browser.close();if(fixture){results.stats={...fixture.stats};await fixture.close();}gateway.kill();
 await writeFile(resolve(output,'report.json'),JSON.stringify(results,null,2)+'\n');await writeFile(resolve(output,'gateway.log'),gatewayLog);
}
