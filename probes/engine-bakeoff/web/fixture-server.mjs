// Local fixture transport for engine tests. Resolve/read before committing HTTP
// headers: a missing optional file must not reject the request listener after 200.
import {createServer} from 'node:http';
import {readFile,realpath} from 'node:fs/promises';
import {realpathSync} from 'node:fs';
import {resolve,relative,isAbsolute,extname,sep} from 'node:path';

const types={'.mjs':'text/javascript','.js':'text/javascript','.wasm':'application/wasm',
  '.css':'text/css','.html':'text/html','.json':'application/json'};
function inside(root,path){
  const name=relative(root,path);
  return name===''||(!isAbsolute(name)&&name!=='..'&&!name.startsWith('..'+sep));
}
export function createFixtureHandler(directory){
  const root=realpathSync(directory);
  return async(request,response)=>{
    function finish(status,body,headers={}){
      if(response.destroyed||response.writableEnded)return;
      if(response.headersSent){response.destroy();return;}
      response.writeHead(status,headers);response.end(body);
    }
    try{
      if(!['GET','HEAD'].includes(request.method))return finish(405,undefined,{Allow:'GET, HEAD'});
      let pathname;
      try{pathname=decodeURIComponent(new URL(request.url,'http://local').pathname);}
      catch{return finish(400);}
      if(pathname.includes('\0')||pathname.includes('\\'))return finish(400);
      if(pathname==='/favicon.ico')return finish(204);
      const candidate=resolve(root,'.'+pathname);
      if(!inside(root,candidate))return finish(403);
      const path=await realpath(candidate);
      if(!inside(root,path))return finish(403);
      const bytes=await readFile(path);
      finish(200,request.method==='HEAD'?undefined:bytes,{
        'Content-Type':types[extname(path)]||'application/octet-stream',
        'Content-Length':bytes.length,'Cache-Control':'no-store'
      });
    }catch(error){
      finish(['ENOENT','ENOTDIR','EISDIR'].includes(error.code)?404:500);
    }
  };
}
export function createFixtureServer(root){return createServer(createFixtureHandler(root));}
