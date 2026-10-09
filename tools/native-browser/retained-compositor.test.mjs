import test from 'node:test';
import assert from 'node:assert/strict';
import {deflateSync} from 'node:zlib';
import {canvasPixels} from './canvas-evidence.mjs';

function image(redX=0, hud=0) {
 const rgba=Buffer.from([hud,80,90,255, hud,80,90,255, 50,60,70,255, 50,60,70,255]);
 rgba.set([255,0,0,255],redX*4);
 const chunk=(name,data)=>{const b=Buffer.alloc(data.length+12);b.writeUInt32BE(data.length);b.write(name,4);data.copy(b,8);return b;};
 const header=Buffer.alloc(13);header.writeUInt32BE(2);header.writeUInt32BE(2,4);header[8]=8;header[9]=6;
 return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',deflateSync(Buffer.concat([Buffer.from([0]),rgba.subarray(0,8),Buffer.from([0]),rgba.subarray(8)]))),chunk('IEND',Buffer.alloc(0))]);
}
function observedCanvas(frame,composited) {
 return {
  async screenshot(){return composited;},
  async evaluate(){return {generation:'7',width:2,height:2,png:'data:image/png;base64,'+frame.toString('base64')};}
 };
}
// PR50 R1: every native retention comparison must read the compositor too.
// The observed generation remains correct and unchanged in these faults.
test('retention comparison rejects a blank compositor despite a cached valid framebuffer',async()=>{
 const frame=image();
 const chunk=(name,data)=>{const b=Buffer.alloc(data.length+12);b.writeUInt32BE(data.length);b.write(name,4);data.copy(b,8);return b;};
 const header=Buffer.alloc(13);header.writeUInt32BE(2);header.writeUInt32BE(2,4);header[8]=8;header[9]=6;
 const cleared=Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',deflateSync(Buffer.alloc(18))),chunk('IEND',Buffer.alloc(0))]);
 await assert.rejects(canvasPixels(observedCanvas(frame,cleared)),/No synthetic avatar/);
});
test('retention equality includes the current visible pose, not just the cached generation',async()=>{
 const frame=image(0);
 const correct=await canvasPixels(observedCanvas(frame,image(0,100)));
 const stale=await canvasPixels(observedCanvas(frame,image(1,100)));
 assert.notEqual(stale,correct,'A wrong/stale visible pose cannot pass retained framebuffer equality');
});
test('every pose comparison takes a new compositor screenshot, including unchanged generations',async()=>{
 let screenshots=0;
 const canvas=observedCanvas(image(),image());
 canvas.screenshot=async()=>{screenshots++;return image();};
 const first=await canvasPixels(canvas);
 assert.equal(await canvasPixels(canvas),first);
 assert.equal(screenshots,2,'Disconnect/idle comparisons cannot reuse the preceding page observation');
});
