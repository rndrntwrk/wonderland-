import {readFile,stat} from 'node:fs/promises';
import {createHash} from 'node:crypto';

const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const digest=value=>typeof value==='string'&&/^[0-9a-f]{64}$/.test(value);
const name=value=>typeof value==='string'&&value.length>0&&value.length<=256
  &&!value.includes('\\')&&!value.startsWith('/')
  &&value.split('/').every(part=>part&&part!=='.'&&part!=='..');

// Operator-supplied, release-pinned local inputs. No browser-supplied filesystem
// paths; only opaque hashes leave this module. Never registers public assets.
export async function createPrivateContentProvider({release,manifestPath}) {
  if(!digest(release))throw Error('Invalid private content release');
  const input=JSON.parse(await readFile(manifestPath,'utf8'));
  if(input.schema_version!==1||!name(input.skeleton_name)||!Array.isArray(input.files)
    ||!input.files.length||input.files.length>256)throw Error('Invalid private content manifest');
  for(const key of ['head_collections','body_collections'])
    if(!Array.isArray(input[key])||!input[key].length||input[key].length>16||!input[key].every(name))
      throw Error('Invalid private collection selection');
  const byHash=new Map(),names=new Set();let total=0;
  for(const row of input.files){
    const identity=row.input_name+'\0'+row.sha256;
    if(!name(row.input_name)||names.has(identity)||!digest(row.sha256)
      ||typeof row.path!=='string'||!Number.isSafeInteger(row.size)||row.size<1||row.size>128*1024*1024)
      throw Error('Invalid private resource');
    // Distinct original archives intentionally repeat names such as textures.dat.
    names.add(identity);total+=row.size;
    if(total>128*1024*1024)throw Error('Private resource pack exceeds browser budget');
    const info=await stat(row.path);
    if(!info.isFile()||info.size!==row.size)throw Error('Private resource size mismatch');
    if(byHash.has(row.sha256)&&byHash.get(row.sha256).size!==row.size)
      throw Error('Private resource identity mismatch');
    byHash.set(row.sha256,{path:row.path,size:row.size});
  }
  if(total!==input.total_input_bytes)throw Error('Private resource total mismatch');
  const manifest=Object.freeze({schema_version:1,content_release:release,
    skeleton_name:input.skeleton_name,head_collections:input.head_collections.slice(),
    body_collections:input.body_collections.slice(),total_input_bytes:total,
    files:input.files.map(row=>({input_name:row.input_name,size:row.size,sha256:row.sha256}))});
  return {
    release,
    manifest(){return structuredClone(manifest);},
    async read(sha256){
      const row=byHash.get(sha256);
      if(!row)throw Error('Unknown private resource');
      const bytes=await readFile(row.path);
      if(bytes.length!==row.size||hash(bytes)!==sha256)throw Error('Private resource hash mismatch');
      return bytes;
    },
  };
}
