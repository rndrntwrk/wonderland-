import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { verifyWorkbenches } from './workbench-workflows.mjs';
import { verifyWorkbenchRegressions } from './workbench-regressions.mjs';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.CREATOR_PLAYWRIGHT_MODULE || 'playwright');
const root = fileURLToPath(new URL('..', import.meta.url));
const dist = resolve(root, process.env.CREATOR_WEB_DIST_DIR || 'dist');
const output = resolve(root, process.env.CREATOR_WEB_EVIDENCE_DIR || 'test-results/browser');
const port = 8879;
const url = `http://127.0.0.1:${port}`;
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const checks = [];
const unexpected = [];
const external = [];
let browser, page, server;
const report = { schema: 'wonderland.creator-web.browser-verification.v1', checks };

// Literal source-layout fixture: IFF 2.5, BHAV 0x8002, UTF-8 STR# -1,
// BCON, PALT1 and SLOT5. It carries no redistributed game asset.
function fixture({ unsafeLabel = false, unknownBytes = 5 } = {}) {
  const header = Buffer.alloc(64);
  header.write('IFF FILE 2.5:TYPE FOLLOWED BY SIZE\0 JAMIE DOORNBOS & MAXIS 1');
  const bhav = Buffer.from([2,128,2,0,0,1,2,0,3,0,0x12,0x34,1,0,1,255,1,2,3,4,5,6,7,8,2,0,254,255,8,7,6,5,4,3,2,1,0xab]);
  const palette = Buffer.alloc(22); palette.writeUInt32LE(1,0); palette.writeUInt32LE(2,4);
  Buffer.from([83,83,203,240,242,248]).copy(palette,16);
  const slot = Buffer.alloc(50); slot.writeUInt32LE(5,4); slot.write('TOLS',8);slot.writeUInt32LE(1,12);
  slot.writeUInt16LE(1,16);[1,2,3].forEach((v,i)=>slot.writeFloatLE(v,18+i*4));slot.writeInt32LE(1,30);slot.writeInt32LE(1,38);slot.writeInt32LE(-1,46);
  const chunks = [
    ['BHAV',4096,'Init',bhav],
    ['STR#',129,'Object strings',Buffer.concat([Buffer.from([255,255,2,0]),Buffer.from('Hello\0Open the resource\0')])],
    ['BCON',4096,'Constants',Buffer.from([2,0,10,0,20,0])],
    ['PALT',1,'Palette',palette],
    ['SLOT',128,'Routing slots',slot],
    ['ZZZZ',9,unsafeLabel?'<img src=x onerror=alert(1)>':'Opaque bytes',Buffer.alloc(unknownBytes,7)],
  ].map(([kind,id,label,data])=>{
    const envelope=Buffer.alloc(76);envelope.write(kind,0);envelope.writeUInt32BE(76+data.length,4);envelope.writeUInt16BE(id,8);envelope.write(label,12,64);return Buffer.concat([envelope,data]);
  });
  return Buffer.concat([header,...chunks]);
}
function payload(bytes, kind, id) {
  for(let cursor=64;cursor<bytes.length;){
    const size=bytes.readUInt32BE(cursor+4);
    assert.ok(size>=76&&cursor+size<=bytes.length,'valid exported IFF envelope');
    if(bytes.toString('ascii',cursor,cursor+4)===kind&&bytes.readUInt16BE(cursor+8)===id)return bytes.subarray(cursor+76,cursor+size);
    cursor+=size;
  }
  throw new Error(`Missing resource ${kind}/${id}`);
}
function spriteFixture() {
  const palette=Buffer.alloc(25);palette.writeUInt32LE(1);palette.writeUInt32LE(3,4);
  palette.fill(0xa3,8,16);Buffer.from([11,12,13,21,22,23,31,32,33]).copy(palette,16);
  // Original SPR2 command forms include opaque, alpha/depth, padding and a
  // skipped row. The literal fixture is independent of the Rust writer.
  const commands=Buffer.from([18,0,1,0xc0,1,0xe7,1,0x40,19,2,15,0xd2,1,0x20,9,1,1,0x60,1,0x80,0,0xa0,0,0]);
  const frame=Buffer.alloc(16);frame.writeUInt16LE(4);frame.writeUInt16LE(2,2);frame.writeUInt32LE(7,4);frame.writeInt16LE(-2,12);frame.writeInt16LE(5,14);
  const sprite=Buffer.alloc(20);[1001,7,1,1001,frame.length+commands.length].forEach((n,i)=>sprite.writeUInt32LE(n,i*4));
  const chunks=[['PALT',7,palette],['SPR2',8,Buffer.concat([sprite,frame,commands])]].map(([kind,id,data])=>{
    const envelope=Buffer.alloc(76);envelope.write(kind);envelope.writeUInt32BE(76+data.length,4);envelope.writeUInt16BE(id,8);envelope.write(kind==='SPR2'?'Sprite':'Sprite palette',12);
    return Buffer.concat([envelope,data]);
  });
  return Buffer.concat([fixture().subarray(0,64),...chunks]);
}
async function check(name, action) {
  try { await action(); checks.push({name,status:'passed'}); }
  catch(error){ checks.push({name,status:'failed',error:error.message});throw error; }
}
async function openFile(target, name, bytes) {
  const path=resolve(output,name);await writeFile(path,bytes);
  const chooser=target.waitForEvent('filechooser');
  await target.getByRole('button',{name:'Open IFF',exact:true}).first().click();
  await (await chooser).setFiles(path);
}
async function exportFile(target, name) {
  const pending=target.waitForEvent('download');
  await target.getByRole('button',{name:'Export IFF',exact:true}).click();
  const path=resolve(output,name);await(await pending).saveAs(path);return readFile(path);
}
async function select(kind,id){
  await page.getByLabel('Filter resources',{exact:true}).fill(kind);
  await page.getByRole('button',{name:new RegExp(`^${kind.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')} ${id} `)}).click();
}
async function waitHeading(name){await page.getByRole('heading',{name,level:1,exact:true}).waitFor();}
async function startServer(){
  server=spawn(process.execPath,[resolve(root,'scripts/serve.mjs')],{cwd:root,env:{...process.env,CREATOR_WEB_DIST_DIR:dist,CREATOR_WEB_PORT:String(port)},stdio:['ignore','pipe','pipe']});
  await new Promise((resolveReady,reject)=>{
    const timeout=setTimeout(()=>reject(new Error('Development server did not become ready.')),15000);
    server.once('error',e=>{clearTimeout(timeout);reject(e);});
    server.once('exit',code=>{clearTimeout(timeout);reject(new Error(`Development server exited ${code}.`));});
    server.stdout.on('data',chunk=>{if(String(chunk).includes('Creator web:')){clearTimeout(timeout);resolveReady();}});
  });
}

try{
  await mkdir(output,{recursive:true});
  await startServer();
  browser=await chromium.launch({headless:true});
  report.browser=await browser.version();
  report.wasm_sha256=sha(await readFile(resolve(dist,'pkg/wonderland_creator_web_bg.wasm')));
  const servedWasm=await fetch(`${url}/pkg/wonderland_creator_web_bg.wasm`);
  assert.equal(servedWasm.status,200,'configured WASM artifact is served');
  report.served_wasm_sha256=sha(Buffer.from(await servedWasm.arrayBuffer()));
  assert.equal(report.served_wasm_sha256,report.wasm_sha256,'served WASM matches the configured local artifact');
  page=await browser.newPage({viewport:{width:1536,height:1024},acceptDownloads:true});
  page.on('pageerror',error=>unexpected.push(error.message));
  page.on('request',request=>{if(!request.url().startsWith(url)&&!request.url().startsWith('blob:'))external.push(request.url());});
  await page.goto(url);
  const input=fixture();report.input_sha256=sha(input);
  await check('native file chooser imports actual IFF bytes',async()=>{
    await openFile(page,'workbench.iff',input);await waitHeading('Init');
    assert.equal(await page.getByRole('heading',{name:'Instructions',exact:true}).count(),1);
    assert.equal(await page.locator('.resource-row').count(),6);
    assert.equal(await page.getByRole('table').locator('tbody tr').count(),2);
    assert.equal(await page.locator('.error-banner').count(),0);
  });
  await page.screenshot({path:resolve(output,'creator-desktop.png'),fullPage:true});
  await check('no-op export is byte-identical',async()=>{assert.deepEqual(await exportFile(page,'no-op.iff'),input);});
  await check('filter, typed tuning edit and exact binary export',async()=>{
    await select('BCON',4096);await waitHeading('Constants');await page.getByRole('tab',{name:'Edit',exact:true}).click();
    assert.equal(await page.getByLabel('Value (0–65535)',{exact:true}).inputValue(),'10');
    await page.getByLabel('Value (0–65535)',{exact:true}).fill('99');await page.getByRole('button',{name:'Apply edit',exact:true}).click();
    await page.getByRole('status').filter({hasText:'Edit resource'}).waitFor();
    const result=await exportFile(page,'tuning.iff');assert.equal(payload(result,'BCON',4096).readUInt16LE(2),99);assert.deepEqual(payload(result,'BHAV',4096),payload(input,'BHAV',4096));
  });
  let beforeInvalid;
  await check('tab keyboard navigation and invalid branch atomicity',async()=>{
    await select('BHAV',4096);await waitHeading('Init');
    await page.getByRole('tab',{name:'Inspect',exact:true}).focus();await page.getByRole('tab',{name:'Inspect',exact:true}).press('ArrowRight');
    assert.equal(await page.getByRole('tab',{name:'Edit',exact:true}).getAttribute('aria-selected'),'true');
    beforeInvalid=await exportFile(page,'before-invalid.iff');
    await page.getByLabel('True destination',{exact:true}).fill('2');await page.getByRole('button',{name:'Apply edit',exact:true}).click();
    await page.getByRole('alert').waitFor();assert.match(await page.getByRole('alert').innerText(),/out-of-range branch/);
    assert.deepEqual(await exportFile(page,'after-invalid.iff'),beforeInvalid);
  });
  let branchEdit;
  await check('guarded branch edit, history undo and redo',async()=>{
    await page.getByLabel('True destination',{exact:true}).fill('254');await page.getByRole('button',{name:'Apply edit',exact:true}).click();
    await page.getByRole('status').filter({hasText:'Edit resource'}).waitFor();
    branchEdit=await exportFile(page,'branch.iff');assert.equal(payload(branchEdit,'BHAV',4096)[14],254);
    await page.getByRole('tab',{name:'History',exact:true}).click();
    assert.equal(await page.locator('.change-list li').count(),2);
    await page.getByRole('button',{name:'Undo',exact:true}).click();assert.deepEqual(await exportFile(page,'undo.iff'),beforeInvalid);
    await page.getByRole('button',{name:'Redo',exact:true}).click();assert.deepEqual(await exportFile(page,'redo.iff'),branchEdit);
  });
  await check('UTF-8 text, emoji and markup stay literal',async()=>{
    await select('STR#',129);await page.getByRole('tab',{name:'Edit',exact:true}).click();
    const value='Bonjour 🌿 <b>literal text</b>';
    await page.getByLabel('String value',{exact:true}).fill(value);await page.getByRole('button',{name:'Apply edit',exact:true}).click();
    await page.getByRole('tab',{name:'Inspect',exact:true}).click();assert.ok((await page.getByRole('table').innerText()).includes(value));
    assert.equal(await page.locator('.tab-panel b').count(),0);
    const bytes=await exportFile(page,'utf8.iff');assert.ok(payload(bytes,'STR#',129).includes(Buffer.from(value)));
  });
  await check('palette and slot edits use the real typed codecs',async()=>{
    await select('PALT',1);await page.getByRole('tab',{name:'Edit',exact:true}).click();
    await page.getByLabel('Red (0–255)',{exact:true}).fill('120');await page.getByRole('button',{name:'Apply edit',exact:true}).click();
    assert.equal(payload(await exportFile(page,'palette.iff'),'PALT',1)[16],120);
    await select('SLOT',128);await page.getByRole('tab',{name:'Edit',exact:true}).click();
    assert.equal(await page.getByLabel('X offset',{exact:true}).inputValue(),'1');
    await page.getByLabel('X offset',{exact:true}).fill('-2.5');await page.getByRole('button',{name:'Apply edit',exact:true}).click();
    assert.equal(payload(await exportFile(page,'slot.iff'),'SLOT',128).readFloatLE(18),-2.5);
  });
  await check('malformed and oversized imports preserve the active document',async()=>{
    const before=await exportFile(page,'before-bad-file.iff');
    await openFile(page,'malformed.iff',Buffer.from('bad'));await page.getByRole('alert').waitFor();
    assert.deepEqual(await exportFile(page,'after-bad-file.iff'),before);
    await openFile(page,'oversize.iff',Buffer.alloc(8*1024*1024+1));await page.getByRole('alert').filter({hasText:'8 MiB'}).waitFor();
    assert.deepEqual(await exportFile(page,'after-large-file.iff'),before);
  });
  await check('reimport validates edited output and clears bounded history',async()=>{
    const bytes=await readFile(resolve(output,'slot.iff'));await openFile(page,'reimport.iff',bytes);await waitHeading('Init');
    await page.getByRole('tab',{name:'History',exact:true}).click();assert.equal(await page.getByRole('button',{name:'Undo',exact:true}).isEnabled(),false);
    assert.deepEqual(await exportFile(page,'reimported.iff'),bytes);
  });
  await check('hostile labels are text and truncated raw previews cannot silently replace resources',async()=>{
    await openFile(page,'unrecognized.iff',fixture({unsafeLabel:true,unknownBytes:4096}));await waitHeading('Init');
    await select('ZZZZ',9);await page.getByRole('tab',{name:'Edit',exact:true}).click();
    assert.equal(await page.locator('img').count(),0);assert.equal(await page.getByLabel('Resource bytes (hex)',{exact:true}).inputValue(),'');
    const before=await exportFile(page,'opaque-before.iff');await page.getByRole('button',{name:'Apply edit',exact:true}).click();
    await page.getByRole('alert').filter({hasText:'complete replacement bytes'}).waitFor();assert.deepEqual(await exportFile(page,'opaque-after.iff'),before);
  });
  await check('390px viewport retains usable resource and edit controls without page overflow',async()=>{
    await openFile(page,'mobile.iff',input);await waitHeading('Init');await page.setViewportSize({width:390,height:844});
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth),true);
    await select('BCON',4096);await page.getByRole('tab',{name:'Edit',exact:true}).click();
    await page.getByLabel('Value (0–65535)',{exact:true}).fill('77');await page.getByRole('button',{name:'Apply edit',exact:true}).click();
    assert.equal(payload(await exportFile(page,'mobile-edit.iff'),'BCON',4096).readUInt16LE(2),77);
    await page.screenshot({path:resolve(output,'creator-mobile.png'),fullPage:true});
  });
  await check('metadata, resource addition and removal preserve exact unaffected bytes',async()=>{
    await page.setViewportSize({width:1536,height:1024});await openFile(page,'metadata.iff',input);await waitHeading('Init');
    await select('BCON',4096);await page.getByRole('tab',{name:'Edit',exact:true}).click();
    await page.getByText('Resource metadata',{exact:true}).click();
    await page.getByLabel('Resource ID',{exact:true}).fill('4097');await page.getByLabel('Resource flags',{exact:true}).fill('4660');
    const label=Buffer.alloc(64);label.write('Edited constants');await page.getByLabel('Label bytes (128 hex digits)',{exact:true}).fill(label.toString('hex'));
    await page.getByRole('button',{name:'Apply metadata',exact:true}).click();
    const renamed=await exportFile(page,'metadata-renamed.iff');assert.deepEqual(payload(renamed,'BCON',4097),payload(input,'BCON',4096));assert.throws(()=>payload(renamed,'BCON',4096));assert.deepEqual(payload(renamed,'BHAV',4096),payload(input,'BHAV',4096));
    await select('BCON',4097);await waitHeading('Edited constants');await page.getByRole('tab',{name:'Edit',exact:true}).click();
    await page.getByText('Add a resource',{exact:true}).click();await page.getByLabel('New resource type',{exact:true}).fill('ZZZZ');await page.getByLabel('New resource ID',{exact:true}).fill('10');await page.getByLabel('New resource payload (hex)',{exact:true}).fill('aabb00ff');await page.getByRole('button',{name:'Add resource',exact:true}).click();
    const added=await exportFile(page,'metadata-added.iff');assert.deepEqual(payload(added,'ZZZZ',10),Buffer.from([0xaa,0xbb,0,255]));
    await select('ZZZZ',10);await page.getByRole('tab',{name:'Edit',exact:true}).click();await page.getByText('Resource metadata',{exact:true}).click();await page.getByRole('button',{name:'Remove resource',exact:true}).click();
    assert.deepEqual(await exportFile(page,'metadata-removed.iff'),renamed);
    await page.getByRole('tab',{name:'History',exact:true}).click();await page.getByRole('button',{name:'Undo',exact:true}).click();assert.deepEqual(await exportFile(page,'metadata-restored.iff'),added);
  });
  await check('empty IFF can acquire its first valid resource',async()=>{
    const empty=input.subarray(0,64);await openFile(page,'empty.iff',empty);await waitHeading('Resource file is empty');assert.deepEqual(await exportFile(page,'empty-no-op.iff'),empty);
    await page.getByText('Add a resource',{exact:true}).click();await page.getByLabel('New resource type',{exact:true}).fill('BCON');await page.getByLabel('New resource ID',{exact:true}).fill('1');await page.getByLabel('New resource payload (hex)',{exact:true}).fill('01002a00');await page.getByRole('button',{name:'Add resource',exact:true}).click();
    await waitHeading('Untitled resource');assert.deepEqual(payload(await exportFile(page,'first-resource.iff'),'BCON',1),Buffer.from([1,0,42,0]));
    await page.getByRole('tab',{name:'History',exact:true}).click();await page.getByRole('button',{name:'Undo',exact:true}).click();await waitHeading('Resource file is empty');assert.deepEqual(await exportFile(page,'empty-restored.iff'),empty);
    await page.getByRole('button',{name:'Redo',exact:true}).click();await waitHeading('Untitled resource');assert.equal(payload(await exportFile(page,'first-resource-redo.iff'),'BCON',1).readUInt16LE(2),42);
  });
  await check('source-bound sprite package export, edit, stale rejection and undo',async()=>{
    const original=spriteFixture();await openFile(page,'sprite.iff',original);await waitHeading('Sprite palette');await select('SPR2',8);
    const pending=page.waitForEvent('download');await page.getByRole('button',{name:'Export sprite package',exact:true}).click();const jsonPath=resolve(output,'sprite-edit.json');await(await pending).saveAs(jsonPath);
    const exported=JSON.parse(await readFile(jsonPath,'utf8'));assert.equal(exported.source_sha256,sha(original));assert.equal(exported.sprite.frames.length,1);assert.equal(exported.sprite.frames[0].indices_hex.length,16);
    exported.sprite.frames[0].position=[11,-8];await page.getByRole('tab',{name:'Edit',exact:true}).click();await page.getByLabel('Sprite package JSON',{exact:true}).fill(JSON.stringify(exported));await page.getByRole('button',{name:'Apply sprite edit',exact:true}).click();
    await page.getByRole('status').filter({hasText:'Sprite edit applied'}).waitFor();const changed=await exportFile(page,'sprite-changed.iff');assert.notDeepEqual(payload(changed,'SPR2',8),payload(original,'SPR2',8));assert.deepEqual(payload(changed,'PALT',7),payload(original,'PALT',7));
    await page.getByLabel('Sprite package JSON',{exact:true}).fill(JSON.stringify(exported));await page.getByRole('button',{name:'Apply sprite edit',exact:true}).click();await page.getByRole('alert').waitFor();assert.deepEqual(await exportFile(page,'sprite-stale.iff'),changed);
    await page.getByRole('tab',{name:'History',exact:true}).click();await page.getByRole('button',{name:'Undo',exact:true}).click();assert.deepEqual(await exportFile(page,'sprite-restored.iff'),original);
  });
  await verifyWorkbenches({page,output,check,payload});
  await verifyWorkbenchRegressions({page,output,check});
  await check('missing application module produces an actionable startup error',async()=>{
    const broken=await browser.newPage();try{
      await broken.route('**/pkg/wonderland_creator_web.js',route=>route.abort());await broken.goto(url);
      await broken.getByRole('alert').filter({hasText:'could not start'}).waitFor();
    }finally{await broken.close();}
  });
  assert.deepEqual(unexpected,[],'no uncaught errors during working editor flows');assert.deepEqual(external,[],'no external resource requests');
  report.external_requests=external;report.page_errors=unexpected;report.status='passed';
}catch(error){
  report.status='failed';report.error=error.stack;
  if(page){try{await page.screenshot({path:resolve(output,'failure.png'),fullPage:true});report.last_page_text=await page.locator('body').innerText();}catch{}}
  process.exitCode=1;
}finally{
  if(browser)await browser.close();
  if(server)server.kill('SIGTERM');
  report.completed_at=new Date().toISOString();await writeFile(resolve(output,'report.json'),JSON.stringify(report,null,2)+'\n');
  console.log(JSON.stringify(report,null,2));
}
