// TEST ONLY. Model admission precedes asynchronous painting; do not capture the
// default 300x150 backing buffer as the baseline for a resized native viewport.
// Keep this sampling formula aligned with world_renderer.rs::surface_size.
export function presentedCanvas(selector) {
  const canvas = document.querySelector(selector);
  const viewport = canvas?.closest('.world-viewport');
  if (!canvas || !viewport || viewport.classList.contains('world-busy')) return null;
  if (canvas.dataset.renderer === 'source-webgl2') {
    // Observe the actual post-draw frame; WebGL may discard its default buffer
    // after presentation. Never force a product redraw just for this check.
    const frame = globalThis.__wonderlandCanvasEvidence?.read(canvas);
    return frame ? {width: frame.width, height: frame.height, pixels: frame.png} : null;
  }
  if (canvas.dataset.renderer !== 'source-software-3d') return null;
  const rect = canvas.getBoundingClientRect();
  if (!Number.isFinite(rect.width) || !Number.isFinite(rect.height) ||
      rect.width <= 0 || rect.height <= 0) return null;
  const scale = Math.min(Math.sqrt(393216 / (rect.width * rect.height)),
    1, 960 / rect.width, 720 / rect.height);
  const width = Math.max(1, Math.round(rect.width * scale));
  const height = Math.max(1, Math.round(rect.height * scale));
  if (canvas.width !== width || canvas.height !== height) return null;
  // Dimensions and pixel bytes are read in the same synchronous browser task.
  return {width, height, pixels: canvas.toDataURL()};
}
