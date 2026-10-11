import test from 'node:test';
import assert from 'node:assert/strict';
import { presentedCanvas } from '../../../tools/native-browser/canvas-readiness.mjs';

function setup(t, changes = {}) {
  const canvas = {
    width: 724, height: 543, dataset: { renderer: 'source-software-3d' },
    getBoundingClientRect: () => ({ width: 1200, height: 900 }),
    closest: () => ({ classList: { contains: () => false } }),
    toDataURL: () => 'painted', ...changes,
  };
  globalThis.document = { querySelector: () => canvas };
  t.after(() => { delete globalThis.document; });
  return canvas;
}

test('default canvas dimensions cannot become a visual baseline', t => {
  setup(t, {width: 300, height: 150});
  assert.equal(presentedCanvas('canvas'), null);
});
test('a ready model does not imply its pending world paint has completed', t => {
  setup(t, {closest: () => ({classList: {contains: () => true}})});
  assert.equal(presentedCanvas('canvas'), null);
});
test('unpainted canvas cannot provide evidence even at the right resolution', t => {
  setup(t, {dataset: {}});
  assert.equal(presentedCanvas('canvas'), null);
});
test('completed native paint returns exact pixels and actual dimensions', t => {
  setup(t);
  assert.deepEqual(presentedCanvas('canvas'), {width:724,height:543,pixels:'painted'});
});
test('hidden or detached surfaces cannot seed a baseline', t => {
  setup(t, {getBoundingClientRect: () => ({width:0,height:0})});
  assert.equal(presentedCanvas('canvas'), null);
});
test('narrow and landscape surfaces follow the native renderer sampling budget', t => {
  const canvas = setup(t);
  for(const [w,h] of [[390,844],[320,600],[844,390],[1920,1080],[100,100]]) {
    const scale=Math.min(Math.sqrt(393216/(w*h)),1,960/w,720/h);
    canvas.width=Math.max(1,Math.round(w*scale));canvas.height=Math.max(1,Math.round(h*scale));
    canvas.getBoundingClientRect=()=>({width:w,height:h});
    assert.ok(presentedCanvas('canvas'));
    canvas.width++;assert.equal(presentedCanvas('canvas'),null);
  }
});
