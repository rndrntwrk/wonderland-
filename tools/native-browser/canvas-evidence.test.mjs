import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';

test('native visual journeys capture compositor images, never open Canvas2D on the live WebGL canvas',async()=>{
 for(const file of ['verify-avatars.mjs','verify-audio.mjs','verify-pose-batches.mjs','verify-pose-retention.mjs']){
  const source=await readFile(new URL(file,import.meta.url),'utf8');
  assert.match(source,/from ['"]\.\/canvas-evidence\.mjs['"]/);
  assert.doesNotMatch(source,/\.evaluate\(c=>c\.toDataURL\(\)\)/);
  assert.doesNotMatch(source,/c\.getContext\('2d'\)\.getImageData/);
  assert.doesNotMatch(source,/const ctx=c\.getContext\('2d'\),rgba/);
 }
});

import {redAvatarPoint,canvasPixels} from './canvas-evidence.mjs';
import {deflateSync} from 'node:zlib';
const image=()=>({width:2,height:2,pixels:new Uint8Array([255,0,0,255,144,112,62,255, 255,0,0,0, 255,0,0,255])});
test('red witness excludes brown terrain and transparent pixels and maps screenshot centers to CSS',()=>{
 assert.deepEqual(redAvatarPoint(image(),{x:100,y:200,width:20,height:40}),{x:115,y:230,pixels:2});
 const absent=image();for(let i=0;i<absent.pixels.length;i+=4)absent.pixels.set([144,112,62,255],i);
 assert.equal(redAvatarPoint(absent,{x:0,y:0,width:2,height:2}),null);
});
test('pixel mapper rejects missing bounds or malformed images',()=>{
 assert.throws(()=>redAvatarPoint(image(),null));assert.throws(()=>redAvatarPoint({...image(),width:10},{x:0,y:0,width:2,height:2}));
});
test('compositor capture does not evaluate or alter the product canvas',async()=>{
 const chunk=(name,bytes)=>{const b=Buffer.alloc(bytes.length+12);b.writeUInt32BE(bytes.length);b.write(name,4);bytes.copy(b,8);return b;};
 const header=Buffer.alloc(13);header.writeUInt32BE(1);header.writeUInt32BE(1,4);header[8]=8;header[9]=6;
 // The existing bounded evidence decoder (not an import parser) does not use CRC.
 const data=Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',deflateSync(Buffer.from([0,255,0,0,255]))),chunk('IEND',Buffer.alloc(0))]);
 let calls=0;
 const result=await canvasPixels({async screenshot(options){calls++;assert.equal(options.animations,'allow');return data;},evaluate(){throw Error('Product canvas must not be changed');}});
 assert.equal(calls,1);assert.equal(result,'1x1:/wAA/w==');
});
