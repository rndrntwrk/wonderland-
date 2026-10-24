import assert from 'node:assert/strict';
import {Miniflare} from 'miniflare';
import {build} from 'esbuild';
import {readFile,mkdtemp,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
const root=await mkdtemp(join(tmpdir(),'wonderland-owned-assets-'));
const bundle=await build({entryPoints:['tests/account-assets-driver.ts'],bundle:true,format:'esm',platform:'neutral',external:['cloudflare:workers'],write:false,
 plugins:[{name:'wasm',setup(b){b.onResolve({filter:/account_crypto_bg\.wasm$/},()=>({path:'./account_crypto_bg.wasm',external:true}));}}]});
const wasm=await readFile('generated/account_crypto_bg.wasm');let mf;
async function start(){mf=new Miniflare({modulesRoot:root,modules:[{type:'ESModule',path:join(root,'worker.mjs'),contents:bundle.outputFiles[0].text},{type:'CompiledWasm',path:join(root,'account_crypto_bg.wasm'),contents:wasm}],compatibilityDate:'2026-07-01',durableObjects:{ACCOUNTS:{className:'AssetFixtureAccounts',useSQLite:true}},durableObjectsPersist:join(root,'do'),bindings:{NATIVE_ACCOUNT_HTTP:'staged-v1',NATIVE_ACCOUNT_ORIGIN:'https://test',NATIVE_ACCOUNT_REALM:'assets-test'}});await mf.ready;}
const request=async(path,data,token,extra={})=>{const r=await mf.dispatchFetch('https://test'+path,{method:'POST',headers:{'content-type':'application/json',origin:'https://test','cf-connecting-ip':'127.0.0.1',...(token?{authorization:'Bearer '+token}:{}),...extra},body:JSON.stringify(data)});return {status:r.status,body:await r.json()};};
const checks=[];let result={passed:false,checks};
try{await start();const owners=[];
 for(const [username,name]of [['asset_owner_a','First Traveller'],['asset_owner_b','Second Traveller']]){
  assert.equal((await request('/test-setup',{username,name})).status,200);
  const login=await request('/native-account/login',{username,password:'native-account-test-passphrase'});assert.equal(login.status,200);
  const roster=await request('/native-account/roster',{after:'0'},login.body.token);owners.push({token:login.body.token,id:roster.body.avatars[0].avatar_id});
 }
 const [a,b]=owners,query={avatar_id:a.id,after:'0'};
 const first=await request('/native-account/assets',query,a.token);assert.equal(first.status,200,'authenticated owned assets page must be available');
 assert.equal(first.body.state,'owned_assets');assert.equal(first.body.balance,'0');assert.equal(first.body.avatar_id,a.id);assert.equal(first.body.outfits.length,3);assert.equal(first.body.next,null);
 assert.deepEqual(first.body.outfits.map(o=>o.suit_type),[0,5,2]);assert.equal(new Set(first.body.outfits.map(o=>o.id)).size,3);
 assert.ok(first.body.outfits.every(o=>o.avatar_id===a.id&&o.source==='cas'&&o.purchase_price==='0'&&o.sale_price==='0'));
 checks.push('actual SQLite source CAS ownership returns three unique instances and zero saved balance; no catalog eligibility is treated as ownership');
 assert.deepEqual((await request('/native-account/assets',query,a.token)).body,first.body);
 assert.notEqual((await request('/native-account/assets',query,b.token)).status,200);
 assert.equal((await request('/native-account/assets',query)).status,401);
 assert.equal((await request('/native-account/assets',query,a.token,{origin:'https://other.invalid'})).status,403);
 for(const data of [{...query,account_id:'2'},{avatar_id:'0',after:'0'},{avatar_id:a.id,after:'01'},{...query,amount:'999'}])assert.equal((await request('/native-account/assets',data,a.token)).status,400);
 checks.push('repeat reads preserve identities/balance; foreign owner, missing token, foreign Origin and injected/malformed query fields reject');
 assert.equal((await request('/test-assets-fixture',{id:a.id,mode:'pages'})).body.outfits,78);
 const many=(await request('/native-account/assets',query,a.token)).body;assert.equal(many.outfits.length,64);assert.equal(many.next,'9007199254741053');
 const restQuery={...query,after:many.next},rest=(await request('/native-account/assets',restQuery,a.token)).body;
 assert.equal(rest.outfits.length,14);assert.equal(rest.outfits[0].id,'9007199254741054');assert.equal(rest.outfits.at(-1).id,'9007199254741067');assert.equal(rest.next,null);
 assert.equal(new Set([...many.outfits,...rest.outfits].map(o=>o.id)).size,78);
 checks.push('64-instance transport pages preserve complete numeric ordering and exact identities above 2^53 without imposing a game inventory cap; rows are test-only pagination inputs, not purchases');
 await mf.dispose();await start();assert.deepEqual((await request('/native-account/assets',query,a.token)).body,many);
 assert.deepEqual((await request('/native-account/assets',restQuery,a.token)).body,rest);
 const ignored=await request('/native-account/assets',query,a.token);assert.equal(ignored.status,200);assert.deepEqual((await request('/native-account/assets',query,a.token)).body,many);
 checks.push('real workerd/SQLite cold restart and a discarded read response preserve exact saved budget and owned instances without grants/debits');
 assert.equal((await request('/native-account/logout',{all:false},a.token)).status,200);assert.equal((await request('/native-account/assets',query,a.token)).status,401);
 assert.equal((await request('/native-account/assets',{avatar_id:b.id,after:'0'},b.token)).status,200);
 checks.push('revoked session cannot inspect possessions while the independent owner retains access');
 const legacy=await request('/test-assets-fixture',{id:b.id,mode:'legacy'});assert.notEqual((await request('/native-account/assets',{avatar_id:b.id,after:'0'},b.token)).status,200);
 assert.deepEqual((await request('/test-assets-fixture',{id:b.id,mode:'inspect'})).body,legacy.body);
 checks.push('legacy ownership without an initialization receipt fails closed; reading neither recreates ownership nor changes retained outfit counts');
 result.passed=true;
}catch(e){result.failure=String(e.stack??e);process.exitCode=1;}
finally{if(mf)await mf.dispose();await writeFile(join(root,'result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify({...result,evidenceRoot:root}));}
