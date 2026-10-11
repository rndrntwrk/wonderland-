// Real native browser journey with adversarial legacy gateway callback bursts.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {chromium} from 'playwright';
import {createFixture} from './peer.mjs';
const output=resolve(process.env.WONDERLAND_NATIVE_QA_OUTPUT??'/tmp/wonderland-legacy-isolation');
await mkdir(output,{recursive:true});
const gateway=spawn(resolve('target/debug/examples/controlled_replay'),[],{env:{...process.env,WONDERLAND_REPLAY_BIND:'127.0.0.1:19487',WONDERLAND_REPLAY_BROWSER_ORIGINS:'http://127.0.0.1:19488'},stdio:['ignore','ignore','pipe']});
let fixture,browser;const report={passed:false,checks:[],errors:[]};
try{
 for(let i=0;i<100;i++){try{if((await fetch('http://127.0.0.1:19487/health')).ok)break;}catch{}await new Promise(r=>setTimeout(r,100));}
 fixture=await createFixture({dist:resolve(process.env.WONDERLAND_NATIVE_QA_DIST??'apps/web-shell/dist'),gateway:'http://127.0.0.1:19487',runtimeExecutable:resolve('target/debug/examples/native_browser_peer'),port:19488});
 browser=await chromium.launch({executablePath:process.env.WONDERLAND_QA_CHROMIUM,headless:true,args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 for(const viewport of [{width:1440,height:1000},{width:390,height:844}]){
 const context=await browser.newContext({viewport});const page=await context.newPage();page.on('pageerror',e=>report.errors.push(e.message));
 await context.addInitScript(()=>{
  const Native=window.WebSocket,descriptor=Object.getOwnPropertyDescriptor(Native.prototype,'onmessage');
  window.__legacyBurst=null;
  window.WebSocket=class extends Native{
   set onmessage(callback){descriptor.set.call(this,event=>{
    let value;try{value=JSON.parse(event.data)}catch{}
    if(value?.event?.type==='vm_frame')window.__legacyBurst=()=>{for(let i=0;i<300;i++)callback?.call(this,new MessageEvent('message',{data:event.data}));return 300};
    callback?.call(this,event);
   })}
   get onmessage(){return descriptor.get.call(this)}
  };
 });
 await page.goto(fixture.origin);await page.getByLabel('Account name',{exact:true}).fill('controlled-player');await page.getByLabel('Password',{exact:true}).fill('test-only');await page.getByRole('button',{name:'Sign in',exact:true}).click();
 await page.getByRole('button',{name:'Play as Controlled Alice',exact:true}).click();await page.getByRole('button',{name:'Select Controlled Source Lot',exact:true}).click();await page.getByRole('button',{name:'Visit',exact:true}).click();
 await page.locator('.native-lot canvas[data-renderer]').waitFor();await page.waitForFunction(()=>typeof window.__legacyBurst==='function');
 const before=await page.locator('#native-tick').textContent();assert.equal(await page.evaluate(()=>window.__legacyBurst()),300);
 await page.waitForFunction(previous=>document.querySelector('#native-tick')?.textContent!==previous,before);
 assert.equal(await page.locator('.native-lot canvas').count(),1);assert.equal(await page.getByRole('alert').filter({hasText:'World updates could not be kept in order'}).count(),0);
 await page.screenshot({path:resolve(output,`native-survives-${viewport.width}.png`)});
 report.checks.push({viewport,legacyCallbacks:300,before,after:await page.locator('#native-tick').textContent()});await context.close();
 }
 assert.deepEqual(report.errors,[]);report.passed=true;
}catch(error){report.failure=String(error.stack||error);throw error}
finally{await writeFile(resolve(output,'result.json'),JSON.stringify(report,null,2));await browser?.close();await fixture?.close();gateway.kill('SIGTERM');}
console.log(JSON.stringify(report));
