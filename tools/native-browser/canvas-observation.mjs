// TEST ONLY: install before navigation. Observe the real color framebuffer at
// its existing post-draw generation notification, before the browser may discard
// a non-preserved drawing buffer. Never create a context, call draw/readPixels,
// dispatch resize, alter CSS/timers, or substitute an offscreen ID attachment.
// Ordinary compositor screenshots remain an independent acceptance dimension.
export function installCanvasPaintObserver(scope=globalThis) {
 if(scope.__wonderlandCanvasEvidence)throw new Error('Canvas evidence observer already installed');
 const maxPixels=1024*1024,maxDimension=4096,maxUrl=8*1024*1024;
 let current=null,observations=0;
 const valid=canvas=>canvas?.tagName==='CANVAS'&&canvas.isConnected&&canvas.closest('.native-lot')&&
  Number.isInteger(canvas.width)&&canvas.width>0&&canvas.width<=maxDimension&&
  Number.isInteger(canvas.height)&&canvas.height>0&&canvas.height<=maxDimension&&
  canvas.width*canvas.height<=maxPixels;
 const read=canvas=>{
  if(!current||current.canvas!==canvas||!valid(canvas)||
    canvas.getAttribute('data-gpu-state')!=='ready'||
    canvas.getAttribute('data-frame-generation')!==current.generation||
    canvas.width!==current.width||canvas.height!==current.height)return null;
  const {generation,width,height,png,observation}=current;
  return {generation,width,height,png,observation};
 };
 const observer=new scope.MutationObserver(records=>{
  if(current&&!current.canvas.isConnected)current=null;
  const painted=new Set();
  for(const record of records){
   const canvas=record.target;
   if(record.type!=='attributes'||canvas?.tagName!=='CANVAS')continue;
   if(current?.canvas===canvas&&canvas.getAttribute('data-gpu-state')!=='ready')current=null;
   if(record.attributeName==='data-frame-generation'&&canvas.closest('.native-lot'))painted.add(canvas);
  }
  for(const canvas of painted){
   // Forget the prior frame before attempting capture: an error cannot leave a
   // successful-looking stale snapshot. Only one native framebuffer is retained.
   current=null;
   if(!valid(canvas)||canvas.getAttribute('data-gpu-state')!=='ready')continue;
   const generation=canvas.getAttribute('data-frame-generation');
   if(!/^(0|[1-9][0-9]{0,19})$/.test(generation??'')||BigInt(generation)>18446744073709551615n)continue;
   try{
    const png=canvas.toDataURL('image/png');
    if(typeof png!=='string'||!png.startsWith('data:image/png;base64,')||png.length>maxUrl)continue;
    current={canvas,generation,width:canvas.width,height:canvas.height,png,observation:++observations};
   }catch{current=null;}
  }
 });
 observer.observe(scope.document,{subtree:true,childList:true,attributes:true,
  attributeFilter:['data-frame-generation','data-gpu-state']});
 Object.defineProperty(scope,'__wonderlandCanvasEvidence',{value:Object.freeze({read}),configurable:false});
}
export async function observeNativeCanvas(page) {
 await page.addInitScript(installCanvasPaintObserver);
}
