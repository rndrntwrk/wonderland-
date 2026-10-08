import {observeNativeCanvas,visibleAvatarPixels,canvasPixels,avatarPoint,waitHiddenAvatar} from './canvas-evidence.mjs';
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
const terrain = process.env.WONDERLAND_NATIVE_TERRAIN_FIXTURE === '1';
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
 await observeNativeCanvas(page);
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
 const frames=new Set(),visibleFrames=new Set();
 for(let i=0;i<14;i++){frames.add(await canvasPixels(canvas));visibleFrames.add(await visibleAvatarPixels(canvas));await delay(120);}
 assert.ok(frames.size>=2,'Accepted changing animation frames must alter the actual canvas');
 assert.ok(visibleFrames.size>=2,'Actual composited avatar must move, not only the underlying framebuffer');
 report.checks.push({name:'Accepted animation changes render real pixels',distinctCanvasFrames:frames.size,distinctVisibleAvatarFrames:visibleFrames.size});
 assert.equal(await page.locator('.world-view-error').count(),0,'No renderer admission error');
 // Find real rendered synthetic red geometry, then use ordinary pointer events.
 let picked=false,selectedPoint=null;
 for(let attempt=0;attempt<8&&!picked;attempt++){
  const point=await avatarPoint(canvas);
  assert.ok(point&&point.pixels>0,'Synthetic avatar geometry must have visible pixels');
  selectedPoint=point;
  await page.mouse.click(point.x,point.y);
  await delay(70);
  picked=await page.getByRole('heading',{name:'Actions',exact:true}).isVisible();
 }
 assert.ok(picked,'Rendered native avatar must open its source action menu when selected');
 report.checks.push({name:'Depth-tested avatar selection opens actual source actions'});
 await page.getByRole('button',{name:'Close source actions',exact:true}).click();
  if (terrain) {
   // Raised plane deliberately occludes the unfixed floor-height avatar.
   report.checks.push({name:'Resource-backed avatar remains visible and pickable on elevated sloped terrain',
    terrainCorners:'160 + 8*x + 16*y', nativePosition:[3.5,3.5], expectedHeightTiles:4.575});
   await shot(page,'native-elevated-contact');
   await fixture.setHidden(2);
   await page.locator('.native-lot[data-native-avatar-models="0"]').waitFor();
   // Model admission is synchronous, but the existing viewport paints on the
   // next animation frame. Wait for the actual completed draw, not its model
   // counter; a permanently stale visible image still fails this bounded check.
   const hiddenPixels=await waitHiddenAvatar(page,canvas);
   assert.equal(hiddenPixels,0,'Hidden=2 cannot keep rendering a selectable avatar');
   await page.mouse.click(selectedPoint.x,selectedPoint.y);await delay(100);
   assert.equal(await page.getByRole('heading',{name:'Actions',exact:true}).count(),0,
    'An old mesh coordinate cannot open source actions while the avatar is hidden');
   await fixture.setHidden(0);
   await page.locator('.native-lot[data-native-avatar-models="1"]').waitFor();
   report.checks.push({name:'Accepted non-boolean Hidden suppresses the original-format model until explicitly restored', hiddenValue:2,hiddenPixels});
  }


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
 const frozen=await canvasPixels(canvas);await delay(450);
 assert.equal(await canvasPixels(canvas),frozen,'Disconnected avatar must not advance');
 report.checks.push({name:'Disconnect freezes accepted avatar pixels'});
 await page.getByRole('button',{name:'Reconnect',exact:true}).click();
 await page.locator('.native-lot[data-native-live="true"][data-native-avatar-models="1"]').waitFor();
 report.checks.push({name:'Reconnect restores original-format avatar resources'});

 // This is the combined PR31/PR33 boundary, not two independent passing builds.
 // Leave the native animation running while its next real action receipt is lost.
 await page.getByRole('button',{name:'Walls down',exact:true}).click();
 await page.getByRole('button',{name:'Your Sim',exact:true}).click();
 await page.locator('.native-source-action').first().click();
 await page.getByText('Accepted by the server',{exact:true}).waitFor();
 const actionsBefore=fixture.stats.actions;
 const historyBefore=await page.locator('.native-action-history li').count();
 fixture.loseNextReceipt();
 const sentAt=performance.now();
 await page.locator('.native-source-action').first().click();
 await page.getByText('Sent · awaiting server acceptance',{exact:true}).waitFor();
 await page.waitForFunction(count=>document.querySelectorAll('.native-action-history li').length===count,historyBefore+1);
 assert.equal(fixture.stats.silentReceipts,1);
 await page.getByRole('button',{name:'Close source actions',exact:true}).click();
 const pendingFrames=new Set(),pendingVisibleFrames=new Set();
 for(let i=0;i<10;i++){pendingFrames.add(await canvasPixels(canvas));pendingVisibleFrames.add(await visibleAvatarPixels(canvas));await delay(120);}
 assert.ok(pendingFrames.size>=2,'Receipt uncertainty must not stop still-accepted animation frames');
 assert.ok(pendingVisibleFrames.size>=2,'Receipt uncertainty must not freeze the visible composited avatar');
 assert.equal(fixture.active(),1,'The test authority keeps this connection open');
 assert.equal(fixture.stats.unknownDrops,0,'No server close may simulate the receipt deadline');
 report.checks.push({name:'Avatar animation and source completion continue while only the receipt is withheld',
  distinctCanvasFrames:pendingFrames.size,distinctVisibleAvatarFrames:pendingVisibleFrames.size,actions:fixture.stats.actions,historyEntries:historyBefore+1});
 await shot(page,'native-avatar-pending-receipt');
 await page.getByRole('button',{name:'Your Sim',exact:true}).click();
 const timeoutMessage='The server did not confirm this action in time. Its result is unknown. Reconnect to continue; it will not be retried.';
 await page.getByText(timeoutMessage,{exact:true}).waitFor({timeout:25000});
 const elapsed=performance.now()-sentAt;
 assert.ok(elapsed>=14000&&elapsed<30000,'Unchanged real 15s receipt deadline, with scheduling allowance');
 assert.equal(await page.locator('.native-lot').getAttribute('data-native-live'),'false');
 assert.equal(await page.locator('.native-lot').getAttribute('data-native-avatar-models'),'1');
 const timeoutFrame=await canvasPixels(canvas);await delay(400);
 assert.equal(await canvasPixels(canvas),timeoutFrame,'Timeout freezes the actual last accepted avatar pose');
 assert.equal(fixture.stats.actions,actionsBefore+1,'No automatic replay from the avatar component');
 assert.equal(fixture.active(),0);
 report.checks.push({name:'Independent receipt timeout freezes the resource-backed avatar without dropping its model',
  elapsedMilliseconds:Math.round(elapsed),models:1,automaticRetries:0});
 await shot(page,'native-avatar-timeout');
 await page.getByRole('button',{name:'Reconnect',exact:true}).click();
 await page.locator('.native-lot[data-native-live="true"][data-native-avatar-models="1"]').waitFor();
 assert.equal(await page.getByRole('button',{name:'Walls down',exact:true}).getAttribute('aria-pressed'),'true',
  'Rebuilding the resource viewport cannot reset player camera/visibility controls');
 const recoveredActions=page.getByRole('button',{name:'Close source actions',exact:true});
 if(await recoveredActions.isVisible())await recoveredActions.click();
 const recoveredFrames=new Set(),recoveredVisibleFrames=new Set();
 for(let i=0;i<10;i++){recoveredFrames.add(await canvasPixels(canvas));recoveredVisibleFrames.add(await visibleAvatarPixels(canvas));await delay(120);}
 assert.ok(recoveredFrames.size>=2,'Recovered avatar animation must resume on accepted frames');
 assert.ok(recoveredVisibleFrames.size>=2,'Recovered avatar must visibly move in the ordinary compositor');
 await page.getByRole('button',{name:'Your Sim',exact:true}).click();
 await page.getByText('Previous action result unknown · not retried',{exact:true}).waitFor();
 assert.equal(await page.locator('.native-action-history li').count(),historyBefore+1,'Recovery cannot replay visual action history');
 assert.ok(await page.locator('.native-source-action').evaluateAll(buttons=>buttons.every(button=>button.disabled)));
 assert.equal(fixture.stats.actions,actionsBefore+1);
 await shot(page,'native-avatar-recovered-unknown');
 await page.getByRole('button',{name:'Dismiss unknown result without retrying',exact:true}).click();
 assert.equal(fixture.stats.actions,actionsBefore+1,'Dismissing uncertainty is not transmission');
 await page.locator('.native-source-action').first().click();
 await page.getByText('Accepted by the server',{exact:true}).waitFor();
 assert.equal(fixture.stats.actions,actionsBefore+2);
 report.checks.push({name:'Avatar resources, selected controls and unknown results survive the same reconnect',
  distinctRecoveredFrames:recoveredFrames.size,distinctVisibleAvatarFrames:recoveredVisibleFrames.size,historyReplay:false,explicitNewActionAccepted:true});
 await page.getByRole('button',{name:'Return to city',exact:true}).click();
 await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();
 for(let i=0;i<30&&fixture.active()!==0;i++)await delay(50);
 assert.equal(fixture.active(),0);
 assert.equal(await page.locator('.native-lot canvas').count(),0);
 report.checks.push({name:'Leaving the animated lot disposes its session and viewport',activeNativeSockets:0});
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
