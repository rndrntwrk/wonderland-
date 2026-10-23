import assert from 'node:assert/strict';
import test from 'node:test';
import {previewSettings,assertPreviewActive,isNativeRequest} from '../src/hosted/preview-policy.ts';
import {retainPreviewWindow,stopExpiredPreview} from '../src/hosted/preview-lifecycle.ts';
const env=()=>({NATIVE_PREVIEW_APPROVAL:'approved-one-hour-v1',NATIVE_PREVIEW_ID:'fixture-preview-0001',
 NATIVE_PREVIEW_ORIGIN:'https://fixture.invalid',NATIVE_PREVIEW_START_MS:'1792790000000',NATIVE_PREVIEW_EXPIRES_MS:'1792793600000',
 NATIVE_BOOTSTRAP_SHA256:'a'.repeat(64),LOT_NAME:'41',ACCOUNT_REALM:'preview-fixture'});
function hooks(){let saved;const calls=[];return {calls,load:async()=>saved,save:async x=>{saved=structuredClone(x);calls.push('save');},
 alarm:async t=>calls.push(['alarm',t]),stop:async()=>calls.push('stop')};}
test('configuration is inactive by default and cannot exceed one hour',()=>{
 for(const change of [{NATIVE_PREVIEW_APPROVAL:undefined},{NATIVE_PREVIEW_EXPIRES_MS:'1792793600001'},
  {NATIVE_PREVIEW_ORIGIN:'http://fixture.invalid'},{NATIVE_PREVIEW_ID:'../outside'},
  {NATIVE_BOOTSTRAP_SHA256:'not pinned'}])assert.throws(()=>previewSettings({...env(),...change}));
 const s=previewSettings(env());assert.throws(()=>assertPreviewActive(s,s.starts-1));
 assert.doesNotThrow(()=>assertPreviewActive(s,s.starts));assert.throws(()=>assertPreviewActive(s,s.expires));
});
test('window survives restart and traffic cannot extend or change its authority',async()=>{
 const s=previewSettings(env()),h=hooks();await retainPreviewWindow(s,h,s.starts);await retainPreviewWindow(s,h,s.starts+1000);
 assert.equal(h.calls.filter(x=>x==='save').length,1);
 await assert.rejects(retainPreviewWindow({...s,expires:s.expires+1},h,s.starts),/changed/);
 await assert.rejects(retainPreviewWindow({...s,lot:'42'},h,s.starts),/changed/);
 await assert.rejects(retainPreviewWindow(s,h,s.expires),/expired/);assert.equal(h.calls.at(-1),'stop');
});
test('early alarm rearms original deadline; deadline destroys without restart',async()=>{
 const s=previewSettings(env()),h=hooks();assert.equal(await stopExpiredPreview(s,h,s.expires-1),false);
 assert.deepEqual(h.calls.at(-1),['alarm',s.expires]);assert.equal(await stopExpiredPreview(s,h,s.expires),true);
 assert.equal(h.calls.at(-1),'stop');
});
test('alarm failure prevents startup permission',async()=>{
 const s=previewSettings(env()),h=hooks();h.alarm=async()=>{throw Error('durable alarm unavailable')};
 await assert.rejects(retainPreviewWindow(s,h,s.starts),/alarm unavailable/);
});
test('public route excludes private/bootstrap paths, bad origins, methods and queries',()=>{
 const s=previewSettings(env());
 const req=(path,method='POST',origin=s.origin)=>new Request(s.origin+path,{method,headers:{origin}});
 assert.equal(isNativeRequest(req('/native-account/login'),s),true);
 assert.equal(isNativeRequest(req('/native-account/create'),s),true);
 assert.equal(isNativeRequest(req('/native-content/file/'+'b'.repeat(64)),s),true);
 for(const path of ['/rpc','/bootstrap','/__native-preview/ready','/native-account/register','/native-account/login?x=1','/native-content/file/../secret'])
  assert.equal(isNativeRequest(req(path),s),false);
 assert.equal(isNativeRequest(req('/native-account/login','GET'),s),false);
 assert.equal(isNativeRequest(req('/native-account/login','POST','https://other.invalid'),s),false);
 assert.equal(isNativeRequest(req('/native-account/create','GET'),s),false);
 assert.equal(isNativeRequest(req('/native-account/create','POST','https://other.invalid'),s),false);
 assert.equal(isNativeRequest(new Request(s.origin+'/native-lot/stream',{headers:{origin:s.origin,upgrade:'websocket'}}),s),true);
});
