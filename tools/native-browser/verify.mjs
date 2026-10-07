// Runs the built application with real browser clicks and a real Rust authority.
// Browser plugin is absent in this environment; hosted Playwright is the runner.
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
async function waitNeed(page,name,value){
  await page.waitForFunction(({name,value})=>[...document.querySelectorAll('.connected-source-needs label')].some(label=>label.querySelector('span')?.textContent===name&&label.querySelector('progress')?.value===value),{name,value});
}
async function setNeeds(page){if(!await page.getByRole('button',{name:'Close needs',exact:true}).isVisible())await page.getByRole('button',{name:'Needs',exact:true}).click();}
async function sourceMenu(page){if(!await page.locator('.native-source-action').isVisible())await page.getByRole('button',{name:'Your Sim',exact:true}).click();await page.locator('.native-source-action').waitFor();}
async function closeMenus(page){for(const name of ['Close needs','Close source actions']){const button=page.getByRole('button',{name,exact:true});if(await button.isVisible())await button.click();}}

try{
  for(let n=0;n<100;n++){try{if((await fetch('http://127.0.0.1:18787/health')).ok)break;}catch{}if(n===99)throw Error('Controlled gateway failed: '+gatewayLog);await delay(50);}
  fixture=await createFixture({dist,gateway:'http://127.0.0.1:18787',runtimeExecutable:resolve('target/debug/examples/native_browser_peer')});
  browser=await chromium.launch({headless:true,args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  results.environment.chromium=browser.version();
  const context=await browser.newContext({viewport:{width:1440,height:1000},reducedMotion:'reduce'});
  const page=await context.newPage();watch(page);await enter(page);
  record('Full browser admission',{url:page.url(),title:await page.title(),nativeConnections:fixture.stats.opens});
  assert.ok(!await page.locator('vite-error-overlay,nextjs-portal').count());
  const first=await page.locator('#native-tick').textContent();await delay(500);
  assert.notEqual(await page.locator('#native-tick').textContent(),first);
  await page.locator('.native-lot canvas').evaluate(canvas=>canvas.setAttribute('data-stability-probe','same-canvas'));
  await setNeeds(page);await fixture.reset();await waitNeed(page,'Hunger',50);
  await page.getByRole('button',{name:'Close needs',exact:true}).click();
  await sourceMenu(page);await page.locator('.native-source-action').first().click();
  await page.getByText('Accepted by the server',{exact:true}).waitFor();
  await page.getByRole('button',{name:'Close source actions',exact:true}).click();
  await setNeeds(page);await waitNeed(page,'Hunger',100);await waitNeed(page,'Energy',100);await waitNeed(page,'Hygiene',100);
  assert.equal(await page.locator('.native-lot canvas').getAttribute('data-stability-probe'),'same-canvas');
  record('Accepted original source action updates needs',{accepted:fixture.stats.accepted,canvasRetained:true,hunger:100});
  await capture(page,'01-desktop-native-needs');await closeMenus(page);
  const otherContext=await browser.newContext({viewport:{width:390,height:844},isMobile:true,hasTouch:true,reducedMotion:'reduce'});
  const other=await otherContext.newPage();watch(other);await enter(other);await fixture.reset();
  await setNeeds(page);await setNeeds(other);await waitNeed(page,'Hunger',50);await waitNeed(other,'Hunger',50);
  await closeMenus(page);await sourceMenu(page);await page.locator('.native-source-action').first().click();await page.getByText('Accepted by the server',{exact:true}).waitFor();
  await waitNeed(other,'Hunger',100);record('Independent browser receives the same accepted need change',{independentBrowserContexts:2,sharedAuthority:true});
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
    const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth+1);assert.equal(overflow,false);
    await setNeeds(page);await contained(page,'.native-lot .connected-world-needs');await capture(page,`04-native-${width}x${height}`);await closeMenus(page);
    await sourceMenu(page);await contained(page,'.native-lot .connected-world-authoring');await closeMenus(page);
    record('Native responsive panels',{width,height,header,toolbar,pageOverflow:false});
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
