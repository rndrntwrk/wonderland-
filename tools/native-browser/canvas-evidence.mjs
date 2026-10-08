// TEST ONLY. Keep complete framebuffer identity separate from ordinary page
// screenshots: the latter also include DOM chrome covering the canvas.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
export {observeNativeCanvas} from './canvas-observation.mjs';
import {setTimeout as sleep} from 'node:timers/promises';
import {png} from '../../apps/web-shell/tests/gpu/png.mjs';
export async function canvasImage(canvas) {
  const bytes=await canvas.screenshot({type:'png',animations:'allow',caret:'initial',timeout:10000});
  return png(bytes);
}
export async function canvasPixels(canvas) {
 const deadline=Date.now()+5000;
 // Let an already-requested Rust animation frame reach the renderer. We never
 // request a product redraw. The read itself still validates the exact marker.
 let first=true;
 while(Date.now()<deadline){
  const sample=await canvas.evaluate(async(element,waitForPaint)=>{
   if(!globalThis.__wonderlandCanvasEvidence)throw new Error('Install native canvas observer before navigation');
   if(waitForPaint)await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
   if(element.closest('.world-viewport')?.classList.contains('world-busy'))return null;
   return globalThis.__wonderlandCanvasEvidence.read(element);
  },first);
  first=false;
  if(sample){
   assert.ok(Number.isInteger(sample.width)&&Number.isInteger(sample.height)&&
    sample.width>0&&sample.height>0&&sample.width<=4096&&sample.height<=4096&&
    sample.width*sample.height<=1024*1024,'Invalid observed framebuffer dimensions');
   assert.ok(typeof sample.png==='string'&&sample.png.length<=8*1024*1024&&
    sample.png.startsWith('data:image/png;base64,'),'Invalid observed framebuffer PNG');
   const image=png(Buffer.from(sample.png.slice(22),'base64'));
   assert.equal(image.width,sample.width);assert.equal(image.height,sample.height);
   assert.ok(redAvatarPoint(image,{x:0,y:0,width:image.width,height:image.height}),
    'Complete pose observation must contain actual synthetic avatar pixels, not a cleared buffer');
   return `${image.width}x${image.height}:sha256:`+createHash('sha256').update(image.pixels).digest('hex');
  }
  await sleep(20);
 }
 throw new Error('No current, completed native color framebuffer observation');
}
export function redAvatarSignature(image) {
 assert.ok(image.width>0&&image.height>0&&image.width<=4096&&image.height<=4096&&
  image.pixels.length===image.width*image.height*4);
 // The synthetic red artwork is the explicitly declared browser motion witness.
 // HUD text cannot count as motion. Full, unmasked pixel equality is separately
 // checked by canvasPixels; this is only ordinary-compositor visibility proof.
 const red=Buffer.alloc(image.pixels.length);let count=0;
 for(let i=0;i<image.pixels.length;i+=4){
  if(image.pixels[i]>150&&image.pixels[i+1]<30&&image.pixels[i+2]<30&&image.pixels[i+3]>0){
   red.set(image.pixels.subarray(i,i+4),i);count++;
  }
 }
 assert.ok(count>0,'No synthetic avatar visible in the ordinary compositor screenshot');
 return `${image.width}x${image.height}:`+createHash('sha256').update(red).digest('hex');
}
export async function visibleAvatarPixels(canvas) {
 return redAvatarSignature(await canvasImage(canvas));
}
export function redAvatarPoint(image,bounds) {
  assert.ok(image.width>0&&image.height>0&&image.pixels.length===image.width*image.height*4);
  assert.ok(bounds&&[bounds.x,bounds.y,bounds.width,bounds.height].every(Number.isFinite)&&bounds.width>0&&bounds.height>0);
  const points=[];
  for(let y=0;y<image.height;y++)for(let x=0;x<image.width;x++){
    const i=(y*image.width+x)*4;
    if(image.pixels[i]>150&&image.pixels[i+1]<30&&image.pixels[i+2]<30&&image.pixels[i+3]>0)points.push([x,y]);
  }
  if(!points.length)return null;
  const [x,y]=points[Math.floor(points.length/2)];
  return {x:bounds.x+(x+.5)*bounds.width/image.width,y:bounds.y+(y+.5)*bounds.height/image.height,pixels:points.length};
}
export async function avatarPoint(canvas) {
  return redAvatarPoint(await canvasImage(canvas),await canvas.boundingBox());
}
export async function waitHiddenAvatar(page,canvas) {
  const deadline=Date.now()+5000;
  while(Date.now()<deadline){
    if(await page.locator('.native-lot[data-native-avatar-models="0"]').count()){
      const image=await canvasImage(canvas);
      // An empty drawable cannot satisfy a hidden-entity test. Require actual
      // opaque terrain/background pixels and no synthetic avatar pixels.
      const opaque=image.pixels.some((v,i)=>i%4===3&&v===255);
      if(opaque&&!redAvatarPoint(image,{x:0,y:0,width:image.width,height:image.height}))return 0;
    }
    await sleep(50);
  }
  throw new Error('Accepted hidden avatar did not disappear from the actual page image');
}
