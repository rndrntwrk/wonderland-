// Opt-in real-engine regression. Build/package the selected artifact and run
// the reference generator first. Presentation screenshots remain a separate gate.
import test from 'node:test';
import assert from 'node:assert/strict';
import {createFixtureServer} from './fixture-server.mjs';
import {createHash} from 'node:crypto';
import {readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {colorDifference,readPpm,compareIds} from '../../../tools/swarm-c/read-png.mjs';

const root=fileURLToPath(new URL('../../../',import.meta.url));
const reference=process.env.WONDERLAND_REFERENCE_DIR||resolve(root,'tools/swarm-c/output/reference');
const variant=process.env.WONDERLAND_ENGINE_VARIANT||'bevy-webgpu';
const webgpu=variant.endsWith('webgpu');
assert.ok(['bevy-webgpu','bevy-webgl2','fyrox-webgl2'].includes(variant));

function stablePoints(ids,depth){
  const points=[],seen=new Set();
  const at=(x,y)=>({object_id:ids.readUInt32LE((y*640+x)*8),generation:ids.readUInt32LE((y*640+x)*8+4)});
  for(let y=3;y<477;y+=3)for(let x=3;x<637;x+=3){
    const id=at(x,y),kind=id.object_id?`entity-${id.object_id}`:(depth.readFloatLE((y*640+x)*4)<1?'ownerless':'background');
    if(seen.has(kind)||id.object_id&&points.filter(point=>point.object_id).length>=8)continue;
    let stable=true;
    for(let dy=-2;dy<=2&&stable;dy++)for(let dx=-2;dx<=2;dx++){
      const neighbor=at(x+dx,y+dy);
      if(id.object_id!==neighbor.object_id||id.generation!==neighbor.generation){stable=false;break;}
    }
    if(stable){points.push({x,y,...id,kind});seen.add(kind);}
  }
  assert.ok(points.some(point=>point.kind==='ownerless'),'reference needs ownerless occlusion');
  assert.ok(points.some(point=>point.kind==='background'),'reference needs background');
  assert.ok(points.filter(point=>point.object_id).length>=2,'reference needs distinct live owners');
  return points;
}

test(`${variant} selects from offscreen GPU IDs and rejects interrupted work`,{timeout:300000},async()=>{
  const {chromium}=await import(process.env.WONDERLAND_PLAYWRIGHT_MODULE||new URL('../../../tools/swarm-c/node_modules/playwright/index.mjs',import.meta.url).href);
  const manifest=JSON.parse(await readFile(resolve(reference,'manifest.json'),'utf8'));
  const server=createFixtureServer(root);
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const report={variant,observations:[],scenes:[],raw:[],console:[],presentationQualified:false};
  let browser;
  try{
    const artifact=resolve(root,`probes/engine-bakeoff/web/pkg/${variant}/engine_bg.wasm`);
    report.artifact={path:`web/pkg/${variant}/engine_bg.wasm`,
      sha256:createHash('sha256').update(await readFile(artifact)).digest('hex'),
      profile:process.env.WONDERLAND_BUILD_PROFILE||'unspecified'};
    const args=webgpu?['--enable-features=Vulkan','--use-gl=angle','--use-angle=swiftshader','--use-vulkan=swiftshader',
      '--use-webgpu-adapter=swiftshader','--disable-vulkan-surface','--enable-unsafe-webgpu']:
      ['--use-gl=angle','--use-angle=swiftshader-webgl','--enable-unsafe-swiftshader'];
    browser=await chromium.launch({headless:process.env.WONDERLAND_HEADED!=='1',channel:'chromium',args,...(process.env.WONDERLAND_CHROMIUM?{executablePath:process.env.WONDERLAND_CHROMIUM}:{})});
    report.browser=browser.version();report.args=args;report.headless=process.env.WONDERLAND_HEADED!=='1';
    for(const avatars of [32,64]){
      const context=await browser.newContext({viewport:{width:1400,height:1100}}),page=await context.newPage();
      await page.addInitScript(()=>{
        // Delay notification after actual GPU work, only in this adversarial
        // harness. Cancellation still rejects a map whose buffer was unmapped.
        const gate=window.__gpuPickTest={hold:false,mapped:0,readPixels:0,releases:[]};
        if(window.GPUBuffer){
          const map=GPUBuffer.prototype.mapAsync;
          GPUBuffer.prototype.mapAsync=function(...args){
            const pending=map.apply(this,args);
            if(this.label!=='Wonderland one-pixel GPU ID readback')return pending;
            return pending.then(async result=>{
              if(!gate.hold)return result;
              gate.mapped++;await new Promise(resolve=>gate.releases.push(resolve));
              if(this.mapState!=='mapped')throw new DOMException('Test-delayed map was cancelled','AbortError');
              return result;
            });
          };
        }
        if(window.WebGL2RenderingContext){
          const read=WebGL2RenderingContext.prototype.readPixels,wait=WebGL2RenderingContext.prototype.clientWaitSync,
            syncParameter=WebGL2RenderingContext.prototype.getSyncParameter;
          WebGL2RenderingContext.prototype.readPixels=function(...args){
            const result=read.apply(this,args);
            if(gate.hold&&args[2]===1&&args[3]===1)gate.readPixels++;
            return result;
          };
          WebGL2RenderingContext.prototype.clientWaitSync=function(...args){
            return gate.hold?this.TIMEOUT_EXPIRED:wait.apply(this,args);
          };
          WebGL2RenderingContext.prototype.getSyncParameter=function(...args){
            // wgpu's non-blocking GL poll uses getSyncParameter, while Fyrox
            // uses clientWaitSync. Hold the actual fence on both APIs.
            return gate.hold&&args[1]===this.SYNC_STATUS?this.UNSIGNALED:syncParameter.apply(this,args);
          };
        }
      });
      page.on('console',message=>{if(message.type()==='error')report.console.push(message.text());});
      page.on('pageerror',error=>report.console.push(String(error)));
      const snapshot=()=>page.evaluate(()=>window.__wonderlandProbe.snapshot());
      const command=async(method,...args)=>{
        const seq=await page.evaluate(({method,args})=>window.__wonderlandProbe[method](...args),{method,args});
        try{
          await page.waitForFunction(seq=>window.__wonderlandProbe.snapshot().lastCommand>=seq,seq,{timeout:20000});
        }catch(error){
          report.failedCommand={method,args,seq,snapshot:await snapshot()};
          throw error;
        }
        return seq;
      };
      try{
        await page.goto(`http://127.0.0.1:${server.address().port}/probes/engine-bakeoff/web/index.html?variant=${variant}&avatars=${avatars}`);
        await page.waitForFunction(()=>window.__wonderlandProbe?.snapshot().ready||window.__wonderlandProbe?.snapshot().errors.length,undefined,{timeout:90000});
        const observed=await snapshot();
        assert.deepEqual(observed.errors,[]);
        assert.equal(observed.actualBackend,webgpu?'webgpu':'webgl2');
        report.observations.push({avatars,engine:observed.engine,actualBackend:observed.actualBackend,
          adapter:observed.adapter,wasmMemoryShared:observed.wasmMemoryShared,
          crossOriginIsolated:observed.crossOriginIsolated,devicePixelRatio:observed.devicePixelRatio});
        await page.evaluate(()=>window.__wonderlandProbe.resize(642,482));
        await page.waitForFunction(()=>{const viewport=window.__wonderlandProbe.snapshot().viewport;return viewport.width===640&&viewport.height===480;});
        for(const mode of ['full2d','hybrid2d','full3d']){
          await command('setMode',mode);await command('setPass','color');
          const ids=await readFile(resolve(reference,`${mode}-${avatars}.ids`)),depth=await readFile(resolve(reference,`${mode}-${avatars}.depth`));
          const scene={mode,avatars,picks:[],interruptions:[]};report.scenes.push(scene);
          const points=stablePoints(ids,depth);
          for(const point of points){
            const seq=await command('selectAt',point.x,point.y);
            await page.waitForFunction(seq=>window.__wonderlandProbe.snapshot().pickCompletedCommand>=seq,seq,{timeout:20000});
            const selected=await snapshot();
            scene.picks.push({point,selection:selected.selection,state:selected.pickState,error:selected.pickError,source:selected.selectionSource});
            assert.equal(selected.pickState,'completed',selected.pickError||JSON.stringify(scene.picks.at(-1)));
            assert.equal(selected.selectionSource,'gpu-id-readback-generation-checked');
            assert.deepEqual(selected.selection,point.object_id?{object_id:point.object_id,generation:point.generation}:null);
            assert.equal(selected.pass,'color','interactive picking must leave the visible pass unchanged');
          }
          for(const interrupt of ['reloadFixture','suspend','simulateLoss']){
            const owner=points.find(point=>point.object_id);
            const ownerSeq=await command('selectAt',owner.x,owner.y);
            await page.waitForFunction(seq=>window.__wonderlandProbe.snapshot().pickCompletedCommand>=seq,ownerSeq);
            const before=(await snapshot()).selection;
            assert.ok(before,'late no-hit must be tested against a live selection');
            await page.evaluate(()=>{const gate=window.__gpuPickTest;gate.hold=true;gate.mapped=0;gate.readPixels=0;});
            await command('selectAt',0,0);
            await page.waitForFunction(webgpu=>webgpu?window.__gpuPickTest.mapped>0:window.__gpuPickTest.readPixels>0,webgpu);
            const seq=await command(interrupt);
            await page.waitForFunction(seq=>window.__wonderlandProbe.snapshot().lastCommand>=seq,seq);
            let state=await snapshot();
            assert.equal(state.pickState,'cancelled');assert.deepEqual(state.selection,before);
            await page.evaluate(()=>{const gate=window.__gpuPickTest;gate.hold=false;for(const release of gate.releases.splice(0))release();});
            await command('resume');
            const visits=state.renderScheduleVisits;
            await page.waitForFunction(visits=>window.__wonderlandProbe.snapshot().renderScheduleVisits>visits+2,visits);
            state=await snapshot();assert.deepEqual(state.selection,before);
            scene.interruptions.push({interrupt,state:state.pickState,selection:state.selection,actualGpuCompletionDelayed:true});
          }
          if(webgpu)for(const pass of ['color','pick']){
            await command('setPass',pass);
            const prior=await snapshot();
            await page.waitForFunction(visits=>window.__wonderlandProbe.snapshot().renderScheduleVisits>visits+2,prior.renderScheduleVisits);
            try{
              const raw=await page.evaluate(()=>window.__wonderlandProbe.readGpuFrame()),state=await snapshot();
              assert.equal(raw.width,640);assert.equal(raw.height,480);
              const image={width:raw.width,height:raw.height,pixels:Buffer.from(raw.base64,'base64')};
              const record={mode,avatars,pass,readback:raw.readback,format:raw.format,stamp:raw.stamp};report.raw.push(record);
              const expected=manifest.scenes.find(scene=>scene.mode===mode&&scene.avatars===avatars);
              assert.equal(raw.stamp.sceneHash,expected.fixtureHash);assert.equal(raw.stamp.mode,mode);assert.equal(raw.stamp.pass,pass);
              if(pass==='pick'){
                record.ids=compareIds(image,ids,depth,state.idMap,expected.idReferences.find(reference=>reference.width===640));
                assert.ok(record.ids.checked>100);assert.ok(record.ids.ownerlessOcclusionChecked>0);assert.equal(record.ids.mismatched,0);
              }else{
                record.color=colorDifference(image,readPpm(await readFile(resolve(reference,`${mode}-${avatars}.ppm`))));
                assert.ok(record.color.meanAbsoluteByteError<=4&&record.color.rootMeanSquareByteError<=12&&record.color.channelFractionOver8<=0.03,JSON.stringify(record.color));
              }
            }catch(error){report.raw.push({mode,avatars,pass,error:String(error),diagnostic:(await snapshot()).gpuReadbackDiagnostic});throw error;}
          }
        }
        await command('setPass','color');
      }finally{await context.close();}
    }
    assert.deepEqual(report.console,[]);
    console.log(JSON.stringify({variant,browser:report.browser,scenes:report.scenes.length,
      picks:report.scenes.reduce((sum,scene)=>sum+scene.picks.length,0),interruptions:report.scenes.reduce((sum,scene)=>sum+scene.interruptions.length,0),raw:report.raw.length,
      presentationQualified:false,evidence:'Actual engine offscreen asynchronous IDs; independent CPU oracle; presentation screenshots remain separate'}));
  }catch(error){report.error=String(error.stack||error);throw error;}
  finally{
    if(process.env.WONDERLAND_GPU_PICK_REPORT)await writeFile(process.env.WONDERLAND_GPU_PICK_REPORT,JSON.stringify(report,null,2));
    await browser?.close();await new Promise(resolve=>server.close(resolve));
  }
});
