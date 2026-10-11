import test from 'node:test';
import assert from 'node:assert/strict';
import {materialPixel} from './material-pixels.mjs';
const image=pixel=>({width:1,height:1,pixels:Uint8Array.from([...pixel,255])});

test('material selection rejects the observed brown-terrain false positive',()=>{
  // Actual failed application capture: first 5x5 interior was terrain at (377,211).
  assert.equal(materialPixel(image([144,112,62]),0,0,0),false);
});
test('blue material selection cannot mistake cyan scenery for the portal marker',()=>{
  assert.equal(materialPixel(image([10,160,180]),0,0,2),false);
});
test('known visible material colors and the original minimum intensity are retained',()=>{
  assert.equal(materialPixel(image([211,19,19]),0,0,0),true);
  assert.equal(materialPixel(image([19,37,211]),0,0,2),true);
  assert.equal(materialPixel(image([100,10,10]),0,0,0),false);
  assert.equal(materialPixel(image([101,10,10]),0,0,0),true);
  for(const pixel of [[229,235,222],[90,130,60],[200,200,200]]){
    assert.equal(materialPixel(image(pixel),0,0,0),false);
    assert.equal(materialPixel(image(pixel),0,0,2),false);
  }
});

test('the actual application gate uses the tested material selector',async()=>{
  const {readFile}=await import('node:fs/promises');
  const source=await readFile(new URL('./application.mjs',import.meta.url),'utf8');
  assert.match(source,/import\s*\{materialPixel\}\s*from ['"]\.\/material-pixels\.mjs['"]/);
  assert.match(source,/const colored=\(x,y,channel\)=>materialPixel\(image,x,y,channel\)/);
});
