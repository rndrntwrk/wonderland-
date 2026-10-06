// wasm-bindgen emits a local JS snippet, not an arbitrary transitive module tree.
import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,copyFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('the shipped source GPU snippet imports without unpublished sibling modules',async()=>{
  const dir=await mkdtemp(join(tmpdir(),'wonderland-snippet-'));
  try {
    const path=join(dir,'world-gpu.mjs');
    await copyFile(new URL('../../public/world-gpu.mjs',import.meta.url),path);
    const api=await import(pathToFileURL(path).href);
    assert.equal(typeof api.captureSourceWorld,'function');
    assert.equal(typeof api.WorldPngCapture,'function');
    assert.equal(typeof api.paintSourceWorld,'function');
  } finally {await rm(dir,{recursive:true,force:true});}
});
