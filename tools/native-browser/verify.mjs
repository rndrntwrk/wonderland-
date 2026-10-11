// Runs the built application with real browser clicks and a real Rust authority.
// Runs locally with supplied browser binaries or in the read-only hosted workflow.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {chromium} from 'playwright';
import {createFixture} from './peer.mjs';

const output=resolve(process.env.WONDERLAND_NATIVE_QA_OUTPUT??'/tmp/wonderland-native-browser-evidence');
const dist=resolve(process.env.WONDERLAND_NATIVE_QA_DIST??'apps/web-shell/dist');
await mkdir(output,{recursive:true});
const gateway=spawn(resolve('target/debug/examples/controlled_replay'),[],{
  env:{...process.env,WONDERLAND_REPLAY_BIND:'127.0.0.1:18787',WONDERLAND_REPLAY_BROWSER_ORIGINS:'http://127.0.0.1:18888'},stdio:['ignore','ignore','pipe']});
let gatewayLog='';gateway.stderr.on('data',data=>{if(gatewayLog.length<64000)gatewayLog+=data;});
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
let fixture,browser;const results={passed:false,environment:{runner:'Playwright 1.56.1',scope:'Controlled original gateway + real native runtime; declared original-BHAV fixtures'},checks:[],screenshots:[],errors:[],warnings:[]};
const record=(name,evidence)=>{results.checks.push({name,evidence});console.log(name,JSON.stringify(evidence));};
async function capture(page,name){const path=resolve(output,name+'.png');await page.screenshot({path,animations:'disabled'});results.screenshots.push({file:name+'.png',sha256:createHash('sha256').update(await readFile(path)).digest('hex')});}
async function enter(page){
  await page.goto(fixture.origin);
  await page.getByLabel('Account name',{exact:true}).fill('controlled-player');
  await page.getByLabel('Password',{exact:true}).fill('test-only');
  await page.getByRole('button',{name:'Sign in',exact:true}).click();
  await page.getByRole('heading',{name:'Choose your Sim',exact:true}).waitFor();
  await page.getByRole('button',{name:'Play as Controlled Alice',exact:true}).click();
  await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();
  await page.getByRole('button',{name:'Select Controlled Source Lot',exact:true}).click();
  await page.getByRole('heading',{name:'Property',exact:true}).waitFor();
  await page.getByRole('button',{name:'Visit',exact:true}).click();
  await page.locator('.native-lot[data-native-live="true"]').waitFor({timeout:30000});
  await page.locator('.native-lot canvas').waitFor();
}
function watch(page){
  page.on('pageerror',error=>results.errors.push({type:'pageerror',message:error.message}));
  page.on('console',message=>{if(message.type()==='error')results.errors.push({type:'console',message:message.text()});else if(message.type()==='warning')results.warnings.push(message.text());});
}
async function contained(page,selector){
  const box=await page.locator(selector).boundingBox(),size=page.viewportSize();
  assert.ok(box&&box.x>=-1&&box.y>=-1&&box.x+box.width<=size.width+1&&box.y+box.height<=size.height+1,`${selector} outside viewport: ${JSON.stringify(box)}`);
  return box;
}
async function usableControls(page){
  const controls=await page.locator('.native-lot .source-world-tools').evaluate(toolbar=>{
    const bounds=toolbar.getBoundingClientRect();
    return {toolbar:{x:bounds.x,y:bounds.y,width:bounds.width,height:bounds.height},buttons:[...toolbar.querySelectorAll('button')].map(button=>{
      const r=button.getBoundingClientRect();
      return {name:button.getAttribute('aria-label')||button.textContent.trim(),x:r.x,y:r.y,width:r.width,height:r.height};
    })};
  });
  const b=controls.toolbar;
  assert.equal(controls.buttons.length,13,'Every original native view/action control stays reachable');
  for(const r of controls.buttons){
    assert.ok(r.name.length>0,'Property button needs an accessible label');
    assert.ok(r.width>=30&&r.height>=30,`Unusable property control ${r.name}: ${JSON.stringify(r)}`);
    assert.ok(r.x>=b.x-1&&r.y>=b.y-1&&r.x+r.width<=b.x+b.width+1&&r.y+r.height<=b.y+b.height+1,`Property control outside toolbar: ${JSON.stringify(r)}`);
  }
  for(let i=0;i<controls.buttons.length;i++)for(let j=i+1;j<controls.buttons.length;j++){
    const a=controls.buttons[i],c=controls.buttons[j];
    const width=Math.min(a.x+a.width,c.x+c.width)-Math.max(a.x,c.x);
    const height=Math.min(a.y+a.height,c.y+c.height)-Math.max(a.y,c.y);
    assert.ok(width<=1||height<=1,`Property controls overlap: ${a.name} / ${c.name}`);
  }
  return controls.buttons.map(({name,width,height})=>({name,width,height}));
}
async function usableQueueControls(page){
  const rows=await page.locator('.native-lot .connected-world-action').evaluateAll(rows=>rows.map(row=>{
    const label=row.querySelector(':scope > span'),button=row.querySelector('button');
    const rect=r=>({x:r.x,y:r.y,width:r.width,height:r.height});
    const range=document.createRange();range.selectNodeContents(label);
    return {id:row.dataset.actionId,label:label.textContent,card:rect(row.getBoundingClientRect()),
      cancel:rect(button.getBoundingClientRect()),text:[...range.getClientRects()].map(rect),
      style:{whiteSpace:getComputedStyle(label).whiteSpace,paddingRight:getComputedStyle(row).paddingRight,overflowWrap:getComputedStyle(label).overflowWrap}};
  }));
  assert.equal(rows.length,1,'Only the running action is left after queue cancellation');
  for(const row of rows){
    assert.ok(row.cancel.width>=28&&row.cancel.height>=28,'Cancel target must retain its existing minimum size');
    for(const text of row.text){
      const width=Math.min(text.x+text.width,row.cancel.x+row.cancel.width)-Math.max(text.x,row.cancel.x);
      const height=Math.min(text.y+text.height,row.cancel.y+row.cancel.height)-Math.max(text.y,row.cancel.y);
      assert.ok(width<=0||height<=0,`Queue label overlaps cancel button: ${JSON.stringify(row)}`);
      assert.ok(text.x>=row.card.x&&text.x+text.width<=row.card.x+row.card.width,'Queue text must stay within its card');
    }
  }
  return rows;
}
async function waitNeed(page,name,value){
  await page.waitForFunction(({name,value})=>[...document.querySelectorAll('.connected-source-needs label')].some(label=>label.querySelector('span')?.textContent===name&&label.querySelector('progress')?.value===value),{name,value});
}
const sourceNeedNames=['Energy','Hunger','Hygiene','Bladder','Social','Fun'];
async function resetSourceNeeds(fixture,pages){
  await fixture.reset();
  for(const page of pages)for(const name of sourceNeedNames)await waitNeed(page,name,50);
}
async function waitSourceNeeds(page){
  // Unchanged source BHAV4107 chooses one of two groups via RandomNumber(3).
  // Accept exactly those two complete results, not any arbitrary need increase.
  const handle=await page.waitForFunction(names=>{
    const values=names.map(name=>[...document.querySelectorAll('.connected-source-needs label')]
      .find(label=>label.querySelector('span')?.textContent===name)?.querySelector('progress')?.value);
    const valid=[[100,100,100,50,50,50],[50,50,50,100,100,100]]
      .some(expected=>expected.every((value,i)=>values[i]===value));
    return valid?values:false;
  },sourceNeedNames);
  try{return await handle.jsonValue();}finally{await handle.dispose();}
}
async function setNeeds(page){if(!await page.getByRole('button',{name:'Close needs',exact:true}).isVisible())await page.getByRole('button',{name:'Needs',exact:true}).click();}
async function sourceMenu(page){if(!await page.locator('.native-source-action').first().isVisible())await page.getByRole('button',{name:'Your Sim',exact:true}).click();await page.locator('.native-source-action').first().waitFor();}
async function closeMenus(page){for(const name of ['Close needs','Close source actions']){const button=page.getByRole('button',{name,exact:true});if(await button.isVisible())await button.click();}}

try{
  for(let n=0;n<100;n++){try{if((await fetch('http://127.0.0.1:18787/health')).ok)break;}catch{}if(n===99)throw Error('Controlled gateway failed: '+gatewayLog);await delay(50);}
  fixture=await createFixture({dist,gateway:'http://127.0.0.1:18787',runtimeExecutable:resolve('target/debug/examples/native_browser_peer'),actionDelayTicks:3});
  browser=await chromium.launch({headless:true,args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  results.environment.chromium=browser.version();
  const context=await browser.newContext({viewport:{width:1440,height:1000},reducedMotion:'reduce'});
  const page=await context.newPage();watch(page);await enter(page);
  record('Full browser admission',{url:page.url(),title:await page.title(),nativeConnections:fixture.stats.opens});
  assert.ok(!await page.locator('vite-error-overlay,nextjs-portal').count());
  const first=await page.locator('#native-tick').textContent();await delay(500);
  assert.notEqual(await page.locator('#native-tick').textContent(),first);
  await page.locator('.native-lot canvas').evaluate(canvas=>canvas.setAttribute('data-stability-probe','same-canvas'));
  await setNeeds(page);await resetSourceNeeds(fixture,[page]);
  await page.getByRole('button',{name:'Close needs',exact:true}).click();
  await sourceMenu(page);await page.locator('.native-source-action').first().click();
  await page.getByText('Accepted by the server',{exact:true}).waitFor();
  await page.getByRole('button',{name:'Close source actions',exact:true}).click();
  await setNeeds(page);const firstNeeds=await waitSourceNeeds(page);
  assert.equal(await page.locator('.native-lot canvas').getAttribute('data-stability-probe'),'same-canvas');
  await page.getByRole('button',{name:'Close needs',exact:true}).click();
  await sourceMenu(page);
  await page.locator('.native-action-feedback').filter({hasText:/Action #[0-9]+ completed/}).waitFor({timeout:3000});
  record('Runtime completion is shown separately from server acceptance',{completion:await page.locator('.native-action-feedback').textContent(),history:await page.locator('.native-action-history li').count()});
  assert.equal(await page.locator('.native-action-history li').count(),1);
  await closeMenus(page);await setNeeds(page);
  record('Accepted original source action updates needs',{accepted:fixture.stats.accepted,canvasRetained:true,names:sourceNeedNames,values:firstNeeds,serverTicksDuringAction:3});
  await capture(page,'01-desktop-native-needs');await closeMenus(page);
  const otherContext=await browser.newContext({viewport:{width:390,height:844},isMobile:true,hasTouch:true,reducedMotion:'reduce'});
  const other=await otherContext.newPage();watch(other);await enter(other);
  await setNeeds(page);await setNeeds(other);await resetSourceNeeds(fixture,[page,other]);
  await closeMenus(page);await sourceMenu(page);await page.locator('.native-source-action').first().click();await page.getByText('Accepted by the server',{exact:true}).waitFor();
  await closeMenus(page);await setNeeds(page);
  const primaryNeeds=await waitSourceNeeds(page),otherNeeds=await waitSourceNeeds(other);
  assert.deepEqual(otherNeeds,primaryNeeds,'Both browser contexts must show the same actual source-selected group');
  record('Independent browser receives the same accepted need change',{independentBrowserContexts:2,sharedAuthority:true,names:sourceNeedNames,values:otherNeeds});
  await capture(other,'02-mobile-native-needs');await closeMenus(page);
  await fixture.reset();fixture.dropNextReceipt();const count=fixture.stats.actions;
  await sourceMenu(page);await page.locator('.native-source-action').first().click();
  await page.locator('.native-lot[data-native-live="false"]').waitFor();
  await page.getByRole('button',{name:'Reconnect',exact:true}).click();
  await page.locator('.native-lot[data-native-live="true"]').waitFor();
  await sourceMenu(page);await page.getByText('Previous action result unknown · not retried',{exact:true}).waitFor();
  assert.equal(fixture.stats.actions,count+1);await delay(300);assert.equal(fixture.stats.actions,count+1);
  record('Unconfirmed action survives recovery without automatic replay',{sentOnce:true,unknownResults:fixture.stats.unknownDrops,checkpoints:fixture.stats.checkpoints});
  await capture(page,'03-reconnected-unknown-result');
  await page.getByRole('button',{name:'Dismiss unknown result without retrying',exact:true}).click();
  await page.locator('.native-source-action').first().click();await page.getByText('Accepted by the server',{exact:true}).waitFor();
  await closeMenus(page);await closeMenus(other);
  for(const [width,height] of [[1440,1000],[390,844],[320,600],[844,390]]){
    await page.setViewportSize({width,height});
    const header=await contained(page,'.native-lot .source-world-header');
    const toolbar=await contained(page,'.native-lot .source-world-tools');
    const controls=await usableControls(page);
    const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth+1);assert.equal(overflow,false);
    await setNeeds(page);await contained(page,'.native-lot .connected-world-needs');await capture(page,`04-native-${width}x${height}`);await closeMenus(page);
    await sourceMenu(page);await contained(page,'.native-lot .connected-world-authoring');await closeMenus(page);
    record('Native responsive panels',{width,height,header,toolbar,controls,pageOverflow:false});
  }
  // Explicitly authored wait behavior exercises real queue/runtime cancellation;
  // it is not presented as an original object-family qualification.
  await page.setViewportSize({width:1440,height:1000});
  await sourceMenu(page);
  const wait=page.getByRole('button',{name:'Wait (test harness)',exact:true});
  await wait.click();await page.getByText('Accepted by the server',{exact:true}).waitFor();
  await page.locator('.native-queue-state').filter({hasText:/^Running$/}).waitFor();
  await wait.click();await page.getByText('Accepted by the server',{exact:true}).waitFor();
  await page.waitForFunction(()=>document.querySelectorAll('.native-lot .connected-world-action').length===2);
  const pendingRow=page.locator('.native-lot .connected-world-action[data-active="false"]');
  const cancelledId=await pendingRow.getAttribute('data-action-id');
  assert.ok(cancelledId);
  await pendingRow.getByRole('button',{name:'Cancel this action',exact:true}).click();
  await page.waitForFunction(id=>!document.querySelector(`.native-lot .connected-world-action[data-action-id="${id}"]`),cancelledId);
  await page.locator('.native-action-feedback').filter({hasText:`Action #${cancelledId} cancelled`}).waitFor();
  assert.equal(await page.locator('.native-lot .connected-world-action').count(),1,'Cancelling one queue item must not delete its running sibling');
  await other.waitForFunction(()=>document.querySelectorAll('.native-lot .connected-world-action').length===1);
  const retainedHistory=await page.locator('.native-action-history li').allTextContents();
  const retainedMessage=await page.locator('.native-action-feedback').textContent();
  const actionCount=fixture.stats.actions;
  await page.getByRole('button',{name:'Reconnect',exact:true}).click();
  await page.locator('.native-lot[data-native-live="true"]').waitFor();await sourceMenu(page);
  assert.deepEqual(await page.locator('.native-action-history li').allTextContents(),retainedHistory,'Recovery must not fabricate or duplicate action completion');
  assert.equal(await page.locator('.native-action-feedback').textContent(),retainedMessage);
  assert.equal(fixture.stats.actions,actionCount,'Recovery must not send another cancellation');
  record('Queued cancellation and terminal history follow accepted source state',{cancelledId,remainingQueue:1,historyPreserved:true,noRetry:true});
  await capture(page,'05-native-cancelled-action');
  await closeMenus(page);
  for(const [width,height] of [[1440,1000],[390,844],[320,600],[844,390]]){
    await page.setViewportSize({width,height});
    record('Queue labels remain separate from cancellation controls',{width,height,rows:await usableQueueControls(page)});
    await capture(page,`06-native-queue-${width}x${height}`);
  }

  await page.getByRole('button',{name:'Return to city',exact:true}).click();
  await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();
  assert.equal(await page.locator('.native-lot').count(),0);
  await otherContext.close();await context.close();
  for(let i=0;i<50&&fixture.active()!==0;i++)await delay(50);
  assert.equal(fixture.active(),0);record('Native sockets disposed after route exit and browser closure',{active:0});
  // Runtime errors are never silently normalized to a successful screenshot.
  assert.deepEqual(results.errors,[]);record('Console and framework health',{pageErrors:0,consoleErrors:0});
  results.passed=true;results.stats={...fixture.stats};
}catch(error){results.failure=error.stack;console.error(error);process.exitCode=1;if(browser)for(const ctx of browser.contexts())for(const p of ctx.pages())await capture(p,'failure-'+results.screenshots.length).catch(()=>{});}
finally{
  await writeFile(resolve(output,'results.json'),JSON.stringify(results,null,2)+'\n');
  await writeFile(resolve(output,'gateway.log'),gatewayLog);
  await browser?.close();await fixture?.close();gateway.kill();
}
