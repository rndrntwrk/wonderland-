// Prepare code-only input for an approved local Docker build. No registry,
// Docker daemon, provider, original-content upload or account action is called.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {mkdir,writeFile,readFile} from 'node:fs/promises';
import {resolve,dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
const repo=resolve(dirname(fileURLToPath(import.meta.url)),'../../..');
const [receiptPath,destination]=process.argv.slice(2);assert.ok(receiptPath&&destination&&process.argv.length===4);
const out=resolve(destination),sha=b=>createHash('sha256').update(b).digest('hex');
const git=(...args)=>execFileSync('git',['-C',repo,...args],{encoding:'utf8'});
assert.equal(git('diff','HEAD','--name-only').trim(),'','Commit tracked changes first');
const receipt=JSON.parse(await readFile(receiptPath));assert.equal(receipt.status,'success');
const changed=git('diff',receipt.sourceCommit,'HEAD','--name-only').trim().split('\n').filter(Boolean);
assert.ok(!changed.some(p=>p.startsWith('crates/')||p.startsWith('apps/web-shell/')||p==='Cargo.toml'||p==='Cargo.lock'),
 'Browser/native compilation inputs changed; supply matching artifacts');
for(const f of receipt.artifacts)assert.equal(sha(await readFile(join(receipt.dist,f.path))),f.sha256);
await mkdir(out);const files=[];
async function put(name,data){const path=join(out,name);await mkdir(dirname(path),{recursive:true});await writeFile(path,data,{flag:'wx'});
 files.push({path:name,bytes:data.length,sha256:sha(data)});}
const tracked=git('ls-tree','-r','--name-only','-z','HEAD').split('\0').filter(Boolean);
for(const name of tracked){
 const code=(name==='Cargo.toml'||name==='Cargo.lock'||/^(crates|services|apps|tools)\/.*(?:\.rs|Cargo\.toml|\.wgsl|\.glsl)$/.test(name));
 const supervisor=name==='tools/native-browser/owned-demo.mjs'||(name.startsWith('tools/native-browser/owned-demo/')&&name.endsWith('.mjs')&&!name.endsWith('.test.mjs'));
 if(code||supervisor)await put(name,execFileSync('git',['-C',repo,'show','HEAD:'+name]));
}
const ws=JSON.parse(await readFile(join(repo,'services/native-edge/package-lock.json'))).packages['node_modules/ws'];
assert.equal(ws.version,'8.21.0');assert.ok(ws.integrity.startsWith('sha512-'));
const pkg={name:'wonderland-container-supervisor',private:true,type:'module',dependencies:{ws:ws.version}};
await put('services/native-edge/package.json',Buffer.from(JSON.stringify(pkg)));
const {dev,...pinnedWs}=ws;
await put('services/native-edge/package-lock.json',Buffer.from(JSON.stringify({name:pkg.name,lockfileVersion:3,requires:true,
 packages:{'':{name:pkg.name,dependencies:pkg.dependencies},'node_modules/ws':pinnedWs}})));
await put('web/index.html',await readFile(join(receipt.dist,'index.html')));
await put('Dockerfile',execFileSync('git',['-C',repo,'show','HEAD:tools/native-browser/owned-demo/Dockerfile']));
assert.ok(!files.some(f=>/\.(dat|anim|skel|iff|wlp|bin)$/.test(f.path)),'Original resources are forbidden in image context');
await writeFile(join(out,'context-manifest.json'),JSON.stringify({sourceCommit:git('rev-parse','HEAD').trim(),
 sourceTree:git('rev-parse','HEAD^{tree}').trim(),browserCompileSource:receipt.sourceCommit,files,
 originalGameInputsIncluded:false,dockerInvoked:false},null,2),{flag:'wx'});
console.log(JSON.stringify({context:out,files:files.length,dockerInvoked:false}));
