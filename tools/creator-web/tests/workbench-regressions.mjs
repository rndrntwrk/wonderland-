import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {neighborhoods,purchase} from './workbench-fixtures.mjs';

export async function verifyWorkbenchRegressions({page,output,check}) {
  const go=async(name,id)=>{await page.getByRole('navigation',{name:'Creator tools'}).getByRole('button',{name,exact:true}).click();const panel=page.locator(`#workbench-${id}`);await panel.waitFor({state:'visible'});return panel;};
  const open=async(panel,name,bytes,label='Open source')=>{
    const path=resolve(output,name);await writeFile(path,bytes);const pending=page.waitForEvent('filechooser');pending.catch(()=>{});
    await panel.getByRole('button',{name:label,exact:true}).and(panel.locator('button')).click();await(await pending).setFiles(path);
    await panel.locator('.workbench-layout').waitFor();await page.waitForFunction(id=>document.querySelector(id)?.getAttribute('aria-busy')==='false',`#${await panel.getAttribute('id')}`);
  };
  const download=async(panel,name,label='Export source')=>{const pending=page.waitForEvent('download');pending.catch(()=>{});await panel.getByRole('button',{name:label,exact:true}).click();const path=resolve(output,name);await(await pending).saveAs(path);return readFile(path);};
  const failures=[];
  const regression=async(name,run)=>{try{await check(name,run);}catch(e){failures.push(e);}};
  await regression('advanced draft cannot silently apply to a replacement source revision',async()=>{
    const panel=await go('Assets','assets');await panel.getByLabel('Asset source format').selectOption('purchasable-outfit');
    const source=purchase();await open(panel,'draft-a.po',source);
    const advanced=panel.locator('details').filter({hasText:'Advanced field edits'});if((await advanced.getAttribute('open'))===null)await panel.getByText('Advanced field edits',{exact:true}).click();
    await panel.getByLabel('Field edit JSON').fill('[{"path":["gender"],"remove":false,"value":1}]');
    const replacement=Buffer.from(source);replacement.writeUInt32BE(2,4);await open(panel,'draft-b.po',replacement);
    await panel.getByRole('button',{name:'Apply field edits',exact:true}).click();
    assert.deepEqual(await download(panel,'draft-replacement-preserved.po'),replacement,'old draft must be cleared or rejected, never rebound');
  });
  await regression('unchanged neighborhood form preserves absent optional source fields byte-for-byte',async()=>{
    const panel=await go('Neighborhoods','neighborhood');await open(panel,'optional-neighborhood.json',neighborhoods);
    await panel.getByRole('button',{name:'Apply neighborhood',exact:true}).click();
    assert.deepEqual(await download(panel,'neighborhood-form-no-op.json'),neighborhoods);
  });
  await regression('FSOm transform preserves element selection, recomputes bounds and roundtrips source-bound GLB',async()=>{
    const panel=await go('Assets','assets');await panel.getByLabel('Asset source format').selectOption('fsom');
    const source=await readFile(new URL('../../../tests/tools/fixtures/creator-workbench-triangle.fsom',import.meta.url));
    await open(panel,'triangle.fsom',source);assert.deepEqual(await download(panel,'triangle-no-op.fsom'),source);
    await panel.getByLabel('Element index',{exact:true}).fill('1');await panel.getByLabel('X / U',{exact:true}).fill('2');
    await panel.getByRole('button',{name:'Apply transform',exact:true}).click();
    assert.equal(await panel.getByLabel('Element index',{exact:true}).inputValue(),'1','nonzero element selection survives edit');
    const model=JSON.parse(await download(panel,'triangle-edited.inspection.json','Download inspection JSON'));
    assert.equal(model.groups[0][0].vertices[1].position[0],1073741824);assert.equal(model.bounds[1][0],1073741824);
    const changed=await download(panel,'triangle-edited.fsom');
    const glb=await download(panel,'triangle.glb','Export GLB');assert.equal(glb.subarray(0,4).toString(),'glTF');
    await open(panel,'triangle-reimport.glb',glb,'Import edited GLB');assert.deepEqual(await download(panel,'triangle-roundtrip.fsom'),changed);
    const obj=await download(panel,'triangle.obj','Export OBJ');await open(panel,'triangle-reimport.obj',obj,'Import edited OBJ');assert.deepEqual(await download(panel,'triangle-obj-roundtrip.fsom'),changed);
    await panel.getByRole('button',{name:'Undo',exact:true}).click();assert.deepEqual(await download(panel,'triangle-undo.fsom'),source);
  });
  if(failures.length)throw new Error(`${failures.length} review regression(s) failed: ${failures.map(e=>e.message).join('; ')}`);
}
