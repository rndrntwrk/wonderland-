// Actual optimized player + original-format fixture: an animation starts in the
// checkpoint and ends inside its supplied recovery tail. No edited wire/DOM/WASM.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdir, readFile, readdir, writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {chromium} from 'playwright';
import {createFixture} from './peer.mjs';

const output = resolve(process.env.WONDERLAND_NATIVE_QA_OUTPUT ?? '/tmp/wonderland-checkpoint-tail');
const resources = resolve(process.env.WONDERLAND_AVATAR_QA_FILES ?? 'target/native-avatar-fixtures');
const report = {passed:false, scope:'Checkpoint seed + 18 accepted tail poses; synthetic original-format geometry', checks:[], screenshots:[], errors:[], warnings:[]};
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const digest = value => createHash('sha256').update(value).digest('hex');
const record = (name, evidence) => {report.checks.push({name, evidence});console.log(name, JSON.stringify(evidence));};
let browser, fixture, gatewayLog = '';
await mkdir(output, {recursive:true});
const gateway = spawn(resolve('target/debug/examples/controlled_replay'), [], {
  env:{...process.env, WONDERLAND_REPLAY_BIND:'127.0.0.1:19887', WONDERLAND_REPLAY_BROWSER_ORIGINS:'http://127.0.0.1:19888'},
  stdio:['ignore', 'ignore', 'pipe'],
});
gateway.stderr.on('data', data => {if(gatewayLog.length<64000) gatewayLog+=data;});
async function shot(page, name) {
  const file = `${name}.png`;
  await page.screenshot({path:resolve(output,file)});
  report.screenshots.push({file,sha256:digest(await readFile(resolve(output,file)))});
}
async function painted(page, tick) {
  await page.waitForFunction(tick => document.querySelector('#native-tick')?.textContent?.match(/Tick (\d+)/)?.[1] === tick, tick);
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await page.waitForFunction(() => {
    const canvas=document.querySelector('.native-lot canvas');
    return canvas && canvas.getAttribute('aria-busy') !== 'true';
  });
}
async function enter(page) {
  await page.goto(fixture.origin);
  assert.equal(await page.title(), 'Wonderland');
  assert.equal(new URL(page.url()).origin, fixture.origin);
  await page.getByLabel('Account name',{exact:true}).fill('controlled-player');
  await page.getByLabel('Password',{exact:true}).fill('test-only');
  await page.getByRole('button',{name:'Sign in',exact:true}).click();
  await page.getByRole('heading',{name:'Choose your Sim',exact:true}).waitFor();
  await page.locator('.connected-content-details > summary').click();
  const loader=page.locator('.connected-content-details .content-loader');
  await loader.locator('input[type="file"]').first().setInputFiles(
    (await readdir(resources)).filter(f=>!f.endsWith('.json')).map(f=>resolve(resources,f)));
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
  await painted(page,(await fixture.state()).tick);
}
try {
  let healthy=false;
  for(let n=0;n<100;n++) {
    try {if((await fetch('http://127.0.0.1:19887/health')).ok){healthy=true;break;}} catch {}
    await delay(100);
  }
  assert.ok(healthy,'Controlled gateway failed to start');
  process.env.WONDERLAND_NATIVE_AVATAR_FIXTURE='1';
  process.env.WONDERLAND_NATIVE_AVATAR_BURST='1';
  process.env.WONDERLAND_NATIVE_TERRAIN_FIXTURE='1';
  browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  report.browser=browser.version();
  let livePixels, liveHash;
  for(const mode of ['live','recovery']) {
    fixture=await createFixture({dist:resolve(process.env.WONDERLAND_NATIVE_QA_DIST??'apps/web-shell/dist'),gateway:'http://127.0.0.1:19887',runtimeExecutable:resolve('target/debug/examples/native_browser_peer'),port:19888,manualTicks:true});
    const context=await browser.newContext({viewport:{width:1200,height:900}});
    const page=await context.newPage();
    page.on('pageerror', error => report.errors.push(error.message));
    page.on('console', message => {
      if(message.type()==='error')report.errors.push(message.text());
      if(message.type()==='warning' && report.warnings.length<50)report.warnings.push(message.text());
    });
    await enter(page);
    const canvas=page.locator('.native-lot canvas');
    const initial=await fixture.state();
    assert.equal(initial.avatars[0].animations.layers[0].current_frame,0);
    assert.equal(initial.avatars[0].animations.layers[0].end_reached,false);
    const initialPixels=await canvas.evaluate(c=>c.toDataURL());
    if(mode==='live') {
      await fixture.burst(18);
    } else {
      fixture.disconnect();
      await page.locator('.native-lot[data-native-live="false"]').waitFor();
      for(let n=0;n<100 && fixture.active()!==0;n++)await delay(20);
      assert.equal(fixture.active(),0);
      assert.equal(await canvas.evaluate(c=>c.toDataURL()),initialPixels,'Disconnected view must freeze');
      fixture.recoverWithTail(18);
      await page.getByRole('button',{name:'Reconnect',exact:true}).click();
      await page.locator('.native-lot[data-native-live="true"][data-native-avatar-models="1"]').waitFor();
      assert.equal(fixture.stats.recoveryTailFrames,18);
    }
    const ended=await fixture.state();
    await painted(page,ended.tick);
    assert.equal(BigInt(ended.tick),BigInt(initial.tick)+18n);
    assert.equal(ended.avatars[0].animations.layers[0].end_reached,true);
    assert.equal(ended.avatars[0].animations.layers[0].current_frame,2);
    const pixels=await canvas.evaluate(c=>c.toDataURL());
    assert.ok(pixels !== initialPixels,'RECOVERY_TAIL_ASSERTION: supplied intermediate bone transforms were discarded');
    if(mode==='live'){livePixels=pixels;liveHash=ended.hash;}
    else {assert.equal(pixels,livePixels,'Recovery must reconstruct the same supplied pose as live replay');assert.equal(ended.hash,liveHash);}
    assert.equal(fixture.stats.actions,0,'Recovery is not a player action');
    assert.equal(await page.locator('.native-action-history li').count(),0,'No replayed historical activity');
    await shot(page,`${mode}-ended-pose`);
    record('Ended pose on raised terrain matches live replay',{mode,tailTicks:18,canvasSha256:digest(pixels),authorityHash:ended.hash,actions:fixture.stats.actions,history:0});
    await painted(page,(await fixture.burst(3)).tick);
    assert.equal(await canvas.evaluate(c=>c.toDataURL()),pixels,'Idle ticks must retain recovered channels');
    await page.getByRole('button',{name:'Rotate right',exact:true}).click();await delay(100);
    assert.notEqual(await canvas.evaluate(c=>c.toDataURL()),pixels);
    await page.getByRole('button',{name:'Rotate left',exact:true}).click();await delay(100);
    assert.equal(await canvas.evaluate(c=>c.toDataURL()),pixels);
    record('Recovered pose survives idle ticks and camera redraws',{mode});
    await page.getByRole('button',{name:'Your Sim',exact:true}).click();
    await page.locator('.native-source-action').first().click();
    await page.getByText('Accepted by the server',{exact:true}).waitFor();
    assert.equal(fixture.stats.actions,1);assert.equal(fixture.stats.accepted,1);
    record('A separately selected source action still works after recovery',{mode,acceptedActions:1});
    await page.getByRole('button',{name:'Close source actions',exact:true}).click();
    if(mode==='recovery') {
      await page.setViewportSize({width:390,height:844});await delay(150);
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
      await shot(page,'recovery-mobile-390x844');
      record('Recovered native lot fits the mobile viewport',{width:390,height:844});
    }
    await page.getByRole('button',{name:'Return to city',exact:true}).click();
    await page.getByRole('heading',{name:'Controlled City',exact:true}).waitFor();
    for(let n=0;n<50&&fixture.active()!==0;n++)await delay(20);
    assert.equal(fixture.active(),0);
    assert.equal(await page.locator('.native-lot canvas').count(),0);
    await context.close();await fixture.close();fixture=undefined;
  }
  assert.deepEqual(report.errors,[]);
  record('Both sessions release sockets and views without page or console errors',{pageErrors:0,consoleErrors:0});
  report.passed=true;
} catch(error) {
  report.failure=error.stack;process.exitCode=1;console.error(error);
  if(browser)for(const context of browser.contexts())for(const page of context.pages())await shot(page,'failure').catch(()=>{});
} finally {
  await browser?.close();await fixture?.close();gateway.kill();
  await writeFile(resolve(output,'gateway.log'),gatewayLog);
  await writeFile(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');
}
