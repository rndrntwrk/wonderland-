// TEST ONLY. Observe the real composited canvas, including WebGL's non-preserved
// default buffer. Do not acquire a different graphics context, redraw the game,
// inject pixels, disable animations or substitute private GPU readback results.
import assert from 'node:assert/strict';
import {setTimeout as sleep} from 'node:timers/promises';
import {png} from '../../apps/web-shell/tests/gpu/png.mjs';
export async function canvasImage(canvas) {
  const bytes=await canvas.screenshot({type:'png',animations:'allow',caret:'initial',timeout:10000});
  return png(bytes);
}
export async function canvasPixels(canvas) {
  const image=await canvasImage(canvas);
  // Compare decoded pixels, not encoder settings or volatile screenshot metadata.
  return `${image.width}x${image.height}:`+Buffer.from(image.pixels).toString('base64');
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
