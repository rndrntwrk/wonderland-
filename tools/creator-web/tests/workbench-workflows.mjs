import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {upgrades,neighborhoods,purchase,cityBmp,patchSource,patch} from './workbench-fixtures.mjs';

export async function verifyWorkbenches({page,output,check,payload}) {
  const go=async(name,id)=>{
    await page.getByRole('navigation',{name:'Creator tools',exact:true}).getByRole('button',{name,exact:true}).click();
    const panel=page.locator(`#workbench-${id}`);await panel.waitFor({state:'visible'});await page.waitForFunction(id=>document.activeElement?.id===id,`workbench-${id}`);return panel;
  };
  const open=async(panel,name,bytes,label='Open source')=>{
    const path=resolve(output,name);await writeFile(path,bytes);const choosing=page.waitForEvent('filechooser');choosing.catch(()=>{});
    await panel.getByRole('button',{name:label,exact:true}).and(panel.locator('button')).click();await(await choosing).setFiles(path);
    await panel.locator('.error-banner, .workbench-layout').first().waitFor();
    await page.waitForFunction(id=>document.querySelector(id)?.getAttribute('aria-busy')==='false',`#${await panel.getAttribute('id')}`);
  };
  const download=async(panel,name,label='Export source')=>{
    const pending=page.waitForEvent('download');pending.catch(()=>{});await panel.getByRole('button',{name:label,exact:true}).and(panel.locator('button')).click();
    const path=resolve(output,name);await(await pending).saveAs(path);return readFile(path);
  };
  const edit=async(panel,edits)=>{
    const details=panel.locator('details').filter({hasText:'Advanced field edits'});
    if((await details.getAttribute('open'))===null)await panel.getByText('Advanced field edits',{exact:true}).click();
    await panel.getByLabel('Field edit JSON',{exact:true}).fill(JSON.stringify(edits));
    await panel.getByRole('button',{name:'Apply field edits',exact:true}).click();
  };
  let editedUpgrades;
  await check('upgrade panel preserves no-op bytes and applies typed price changes',async()=>{
    const panel=await go('Upgrades','upgrades');await open(panel,'upgrades.json',upgrades);
    assert.deepEqual(await download(panel,'upgrades-no-op.json'),upgrades);
    await panel.getByLabel('Price ($literal, Rrelative or object GUID)',{exact:true}).fill('$75');
    await panel.getByRole('button',{name:'Apply level fields',exact:true}).click();
    editedUpgrades=await download(panel,'upgrades-edited.json');const json=JSON.parse(editedUpgrades);
    assert.equal(json.Files[0].Upgrades[0].Price,'$75');assert.equal(json.Files[0].Future.retain,7);
    assert.equal(await panel.getAttribute('data-dirty'),'true');
    await panel.getByRole('button',{name:'Undo',exact:true}).click();assert.deepEqual(await download(panel,'upgrades-undo.json'),upgrades);
    await panel.getByRole('button',{name:'Redo',exact:true}).click();assert.deepEqual(await download(panel,'upgrades-redo.json'),editedUpgrades);
    await page.screenshot({path:resolve(output,'creator-upgrades-desktop.png'),fullPage:true});
  });
  await check('neighborhood panel enforces identities and keeps source-order nearest ties',async()=>{
    const panel=await go('Neighborhoods','neighborhood');await open(panel,'neighborhoods.json',neighborhoods);
    assert.deepEqual(await download(panel,'neighborhood-no-op.json'),neighborhoods);
    await panel.getByLabel('Query X',{exact:true}).fill('11');await panel.getByLabel('Query Y',{exact:true}).fill('10');
    await panel.getByRole('button',{name:'Find nearest',exact:true}).click();await panel.getByRole('status').filter({hasText:'Nearest source index: 0'}).waitFor();
    await panel.getByLabel('Explicit GUID',{exact:true}).fill('b');await panel.getByRole('button',{name:'Apply neighborhood',exact:true}).click();
    await panel.getByRole('alert').waitFor();assert.deepEqual(await download(panel,'neighborhood-rejected.json'),neighborhoods);
    await panel.getByLabel('Explicit GUID',{exact:true}).fill('a');await panel.getByLabel('Neighborhood name',{exact:true}).fill('Garden Path');
    await panel.getByRole('button',{name:'Apply neighborhood',exact:true}).click();const changed=JSON.parse(await download(panel,'neighborhood-edited.json'));
    assert.equal(changed[0].Name,'Garden Path');assert.equal(changed[0].Future,7);assert.equal(changed[1].Name,'Harbor');
  });
  await check('asset panel edits real binary references without rounding 64-bit identities',async()=>{
    const panel=await go('Assets','assets');await panel.getByLabel('Asset source format',{exact:true}).selectOption('purchasable-outfit');
    const source=purchase();await open(panel,'outfit.po',source);assert.deepEqual(await download(panel,'outfit-no-op.po'),source);
    await edit(panel,[{path:['gender'],remove:false,value:1}]);const changed=await download(panel,'outfit-edited.po');
    assert.equal(changed.readUInt32BE(4),1);assert.equal(changed.readBigUInt64BE(16),0xfedcba9876543210n);
    assert.deepEqual(changed.subarray(8),source.subarray(8));
    const metadata=await download(panel,'outfit-inspection.json','Download inspection JSON');assert.match(metadata.toString(),/18364758544493064720/);
    await edit(panel,[{path:['unexpected'],remove:false,value:7}]);await panel.getByRole('alert').waitFor();assert.deepEqual(await download(panel,'outfit-rejected.po'),changed);
    await panel.getByRole('button',{name:'Undo',exact:true}).click();assert.deepEqual(await download(panel,'outfit-undo.po'),source);
  });
  await check('city painter modifies real 512px map pixels and rejects invalid categorical colors',async()=>{
    const panel=await go('City painter','city');const source=cityBmp();await open(panel,'terrain.bmp',source);
    assert.deepEqual(await download(panel,'city-no-op.bmp'),source);
    await panel.getByLabel('Pixel X',{exact:true}).fill('10');await panel.getByLabel('Pixel Y',{exact:true}).fill('10');
    await panel.getByLabel('Terrain palette preset',{exact:true}).selectOption('rock');await panel.getByRole('button',{name:'Apply paint',exact:true}).click();
    await panel.getByRole('button',{name:'Inspect pixel',exact:true}).click();await panel.getByRole('status').filter({hasText:'RGBA: 255, 0, 0, 255'}).waitFor();
    const changed=await download(panel,'city-edited.bmp','Export BMP');const row=changed.readInt32LE(22)<0?10:511-10;const at=changed.readUInt32LE(10)+(row*512+10)*(changed.readUInt16LE(28)/8);
    assert.deepEqual([...changed.subarray(at,at+3)],[0,0,255]);
    await panel.getByLabel('Red',{exact:true}).fill('13');await panel.getByRole('button',{name:'Apply paint',exact:true}).click();await panel.getByRole('alert').waitFor();
    assert.deepEqual(await download(panel,'city-rejected.bmp','Export BMP'),changed);
    await panel.getByRole('button',{name:'Undo',exact:true}).click();assert.deepEqual(await download(panel,'city-undo.bmp'),source);
    await panel.getByLabel('Pixel X',{exact:true}).fill('3');await panel.getByLabel('Pixel Y',{exact:true}).fill('3');await panel.getByRole('button',{name:'Draw road',exact:true}).click();
    await panel.getByRole('button',{name:'Inspect pixel',exact:true}).click();await panel.getByRole('status').filter({hasText:'RGBA: 8, 8, 8, 255'}).waitFor();
    const png=await download(panel,'road.png','Export PNG');assert.equal(png.subarray(1,4).toString(),'PNG');
    await page.screenshot({path:resolve(output,'creator-city-desktop.png'),fullPage:true});
  });
  await check('patch panel resolves ordered PIFF bytes, suppression and exact source names',async()=>{
    const panel=await go('Patches','patches');await open(panel,'Source.iff',patchSource);
    assert.deepEqual(await download(panel,'patch-original.iff'),patchSource);
    await open(panel,'official.piff',patch('official'),'Add official PIFF');
    assert.equal(payload(await download(panel,'official-effective.iff','Export effective IFF'),'ZZZZ',1).toString(),'official');
    await open(panel,'user.piff',patch('user-authored'),'Add user PIFF');
    const effective=await download(panel,'user-effective.iff','Export effective IFF');assert.equal(payload(effective,'ZZZZ',1).toString(),'user-authored');assert.equal(payload(effective,'ZZZZ',2).toString(),'untouched');
    const metadata=JSON.parse(await download(panel,'patch-provenance.json','Download inspection JSON'));
    assert.equal(metadata.applied[0].name,'user.piff');assert.equal(metadata.suppressed[0].name,'official.piff');
    await panel.getByLabel('Exact source IFF name',{exact:true}).fill('source.iff');await panel.getByRole('button',{name:'Apply source name',exact:true}).click();
    assert.deepEqual(await download(panel,'case-mismatch.iff','Export effective IFF'),patchSource);
    await panel.getByLabel('Exact source IFF name',{exact:true}).fill('Source.iff');await panel.getByRole('button',{name:'Apply source name',exact:true}).click();
    await panel.locator('.patch-inputs li').filter({hasText:'user.piff'}).getByRole('button',{name:'Remove',exact:true}).click();
    assert.equal(payload(await download(panel,'official-restored.iff','Export effective IFF'),'ZZZZ',1).toString(),'official');
    assert.deepEqual(await download(panel,'source-preserved.iff'),patchSource);
  });
  await check('workbench navigation preserves independent documents, undo history and dirty state',async()=>{
    const panel=await go('Upgrades','upgrades');assert.deepEqual(await download(panel,'upgrades-after-navigation.json'),editedUpgrades);
    assert.equal(await panel.getByRole('button',{name:'Undo',exact:true}).isEnabled(),true);assert.equal(await panel.getAttribute('data-dirty'),'true');
    await page.getByRole('navigation',{name:'Creator tools',exact:true}).getByRole('button',{name:'IFF resources',exact:true}).click();
    assert.equal(await page.getByRole('heading',{name:'Sprite',exact:true}).count(),1);
    assert.equal(await page.locator('[data-dirty="true"]').count()>0,true);
  });
  await check('all workbench panels stay usable at 390px with keyboard focus and no page overflow',async()=>{
    await page.setViewportSize({width:390,height:844});
    for(const [name,id] of [['Upgrades','upgrades'],['Neighborhoods','neighborhood'],['Assets','assets'],['City painter','city'],['Patches','patches']]){
      const panel=await go(name,id);assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,`${name} page width`);
      assert.equal(await panel.evaluate(node=>document.activeElement===node),true,`${name} focus`);
      assert.equal(await panel.getByRole('button',{name:'Export source',exact:true}).isEnabled(),true);
    }
    await go('Neighborhoods','neighborhood');await page.screenshot({path:resolve(output,'creator-neighborhood-mobile.png'),fullPage:true});
    await page.setViewportSize({width:1536,height:1024});
  });
}
