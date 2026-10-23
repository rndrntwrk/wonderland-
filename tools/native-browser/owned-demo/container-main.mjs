// Linux Container PID 1. Only private bindings supply original inputs. No
// account registration, public RPC, provider credentials or local-save authority.
import {mkdtemp} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import assert from 'node:assert/strict';
import {createOwnedDemo} from '../owned-demo.mjs';
import {createPrivateContentProvider} from './private-content.mjs';
import {boundedResource,stageContainerBootstrap,installPreviewExpiry} from './container-bootstrap.mjs';

const origin=process.env.WONDERLAND_PUBLIC_ORIGIN,expires=Number(process.env.WONDERLAND_PREVIEW_EXPIRES_MS);
const expectedSha256=process.env.WONDERLAND_BOOTSTRAP_SHA256,mode=process.env.WONDERLAND_START_MODE;
assert.ok(['initialize','recover'].includes(mode));
let demo;
// Process exit is independent of request activity, socket lifetime and DO alarm
// delivery. Existing per-commit durable acknowledgements/recovery stay unchanged;
// expiry never invents a final snapshot or receipt.
const clearExpiry=installPreviewExpiry({expires,expire:()=>{void demo?.close();process.exit(0);}});
try{
 const parent=await mkdtemp(join(tmpdir(),'wonderland-container-'));
 const response=await fetch('http://owned-private.invalid/bootstrap',{redirect:'error',signal:AbortSignal.timeout(30000)});
 const bytes=await boundedResource(response,2*1024*1024);
 const staged=await stageContainerBootstrap({root:join(parent,'inputs'),bytes,expectedSha256,origin,
  readBlob:async(hash,size)=>boundedResource(await fetch('http://owned-private.invalid/blob/'+hash,
   {redirect:'error',signal:AbortSignal.timeout(30000)}),size)});
 assert.ok(Date.now()<expires,'Preview expired during bootstrap');
 const contentProvider=await createPrivateContentProvider({release:staged.manifest.content_release,manifestPath:staged.contentManifest});
 demo=await createOwnedDemo({manifest:staged.manifest,contentProvider,dist:'/app/web',hostExecutable:'/app/bin/wonderland-native-lot-host',
  storageRoot:join(parent,'runtime'),resume:mode==='recover',port:8080,hosted:{publicOrigin:origin}});
 for(const signal of ['SIGINT','SIGTERM'])process.on(signal,()=>{void demo.close().finally(()=>process.exit(0));});
}catch(error){clearExpiry();await demo?.close();console.error('Native preview startup failed');process.exitCode=1;}
