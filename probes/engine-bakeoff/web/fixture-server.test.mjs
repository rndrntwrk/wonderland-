import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,rm,mkdir,symlink} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {request} from 'node:http';
import {createFixtureHandler,createFixtureServer} from './fixture-server.mjs';

async function fixture(t){
  const parent=await mkdtemp(join(tmpdir(),'wonderland-http-'));
  const root=join(parent,'public');await mkdir(root);
  await writeFile(join(root,'engine.wasm'),Buffer.from([0,97,115,109,1,0,0,0]));
  await writeFile(join(root,'host.mjs'),'export const ready = true;');
  await writeFile(join(root,'with space.txt'),'space');
  await writeFile(join(parent,'outside.txt'),'must not be served');
  t.after(()=>rm(parent,{recursive:true,force:true}));
  return {parent,root};
}
function responseRecorder(){
  return {headersSent:false,writableEnded:false,destroyed:false,codes:[],headers:{},body:null,
    writeHead(status,headers={}){
      if(this.headersSent)throw Object.assign(new Error('Cannot write headers after they are sent to the client'),{code:'ERR_HTTP_HEADERS_SENT'});
      this.headersSent=true;this.codes.push(status);this.headers=headers;return this;
    },
    end(body){this.body=body;this.writableEnded=true;},
    destroy(){this.destroyed=true;}
  };
}
test('missing files produce one 404, never an already-committed 200 or rejection',async t=>{
  const {root}=await fixture(t),response=responseRecorder();
  await assert.doesNotReject(createFixtureHandler(root)({url:'/missing.wasm',method:'GET'},response));
  assert.deepEqual(response.codes,[404]);assert.equal(response.writableEnded,true);
});
test('optional favicon returns one empty 204',async t=>{
  const {root}=await fixture(t),response=responseRecorder();
  await createFixtureHandler(root)({url:'/favicon.ico',method:'GET'},response);
  assert.deepEqual(response.codes,[204]);assert.equal(response.body,undefined);
});
test('already-closed response is not written after a read fails',async t=>{
  const {root}=await fixture(t),response=responseRecorder();response.destroyed=true;
  await assert.doesNotReject(createFixtureHandler(root)({url:'/missing.wasm',method:'GET'},response));
  assert.deepEqual(response.codes,[]);
});
function get(port,path,method='GET'){
  return new Promise((resolve,reject)=>{
    const req=request({hostname:'127.0.0.1',port,path,method},response=>{
      const chunks=[];response.on('data',chunk=>chunks.push(chunk));
      response.on('end',()=>resolve({status:response.statusCode,headers:response.headers,body:Buffer.concat(chunks)}));
      response.on('error',reject);
    });
    req.setTimeout(3000,()=>req.destroy(new Error('fixture HTTP request timed out')));
    req.on('error',reject);req.end();
  });
}
async function server(t){
  const data=await fixture(t),server=createFixtureServer(data.root);
  await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});
  t.after(()=>new Promise(resolve=>{server.closeAllConnections();server.close(resolve);}));
  return {...data,port:server.address().port};
}
test('real HTTP: WASM/module MIME and bytes survive concurrent requests',async t=>{
  const {port}=await server(t);
  const results=await Promise.all(Array.from({length:8},(_,i)=>get(port,i%2?'/host.mjs':'/engine.wasm')));
  for(const [i,result] of results.entries()){
    assert.equal(result.status,200);
    assert.equal(result.headers['content-type'],i%2?'text/javascript':'application/wasm');
    assert.deepEqual(result.body,i%2?Buffer.from('export const ready = true;'):Buffer.from([0,97,115,109,1,0,0,0]));
  }
});
test('real HTTP: missing files, directories and favicon do not poison later loads',async t=>{
  const {port}=await server(t);
  const results=await Promise.all(['/missing.wasm','/','/favicon.ico'].map(path=>get(port,path)));
  assert.deepEqual(results.map(result=>result.status),[404,404,204]);
  assert.equal((await get(port,'/engine.wasm')).status,200);
});
test('real HTTP: decode valid paths, refuse malformed encoding and escaped paths',async t=>{
  const {port,root,parent}=await server(t);
  await symlink(join(parent,'outside.txt'),join(root,'escaped.txt'));
  assert.equal((await get(port,'/with%20space.txt')).body.toString(),'space');
  assert.equal((await get(port,'/%ZZ')).status,400);
  assert.equal((await get(port,'/..%2Foutside.txt')).status,403);
  assert.equal((await get(port,'/escaped.txt')).status,403);
});
test('real HTTP: HEAD retains MIME and length, writes no body; other methods fail',async t=>{
  const {port}=await server(t),head=await get(port,'/engine.wasm','HEAD');
  assert.equal(head.status,200);assert.equal(head.headers['content-type'],'application/wasm');
  assert.equal(head.headers['content-length'],'8');assert.equal(head.body.length,0);
  assert.equal((await get(port,'/engine.wasm','POST')).status,405);
});
