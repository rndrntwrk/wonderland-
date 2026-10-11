// Test-side synchronization only. Never resizes the canvas or advances the VM.
import {setTimeout as delay} from 'node:timers/promises';

export function drawableViewportReady(snapshot,{width,height,dpr}){
  const v=snapshot?.viewport;
  return !!v&&snapshot.devicePixelRatio===dpr&&
    [v.cssWidth,v.cssHeight,v.width,v.height].every(Number.isFinite)&&
    width>0&&height>0&&dpr>0&&
    Math.abs(v.cssWidth-width)<0.01&&Math.abs(v.cssHeight-height)<0.01&&
    Math.abs(v.width-v.cssWidth*dpr)<=1&&Math.abs(v.height-v.cssHeight*dpr)<=1;
}

export async function waitForDrawableViewport(read,expected,{timeout=20000,interval=50,now=()=>performance.now(),sleep=delay}={}){
  if(typeof read!=='function'||!Number.isFinite(timeout)||timeout<=0||timeout>30000||!Number.isFinite(interval)||interval<=0||interval>1000||![expected?.width,expected?.height,expected?.dpr].every(v=>Number.isFinite(v)&&v>0))throw new Error('Invalid drawable readiness request');
  const deadline=now()+timeout;
  let observed;
  do{
    observed=await read();
    if(drawableViewportReady(observed,expected))return observed;
    if(now()>=deadline)break;
    await sleep(Math.min(interval,Math.max(0,deadline-now())));
  }while(now()<=deadline);
  throw new Error('Drawable resize did not settle: '+JSON.stringify({expected,viewport:observed?.viewport,dpr:observed?.devicePixelRatio}));
}
