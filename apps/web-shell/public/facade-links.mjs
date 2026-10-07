// Self-contained wasm-bindgen module; owns only short-lived export URLs.
export function makeFacadeLinks(bytes, metadata, key) {
  if (!(bytes instanceof Uint8Array) || bytes.length < 9 || bytes.length > 16 * 1024 * 1024 ||
      bytes[0] !== 70 || bytes[1] !== 83 || bytes[2] !== 79 || bytes[3] !== 102 ||
      typeof key !== 'string' || !/^[a-f0-9]{64}$/.test(key) ||
      typeof metadata !== 'string' || new TextEncoder().encode(metadata).length > 65536) {
    throw new Error('Invalid facade export');
  }
  const details = JSON.parse(metadata);
  if (details.kind !== 'presentation_facade' || details.source_hash !== key || details.bytes !== bytes.length ||
      details.not_a_game_save !== true) throw new Error('Mismatched facade metadata');
  let file;
  try {
    file = URL.createObjectURL(new Blob([bytes], {type: 'application/octet-stream'}));
    const info = URL.createObjectURL(new Blob([metadata], {type: 'application/json'}));
    return [file, info];
  } catch (error) {
    if (file) URL.revokeObjectURL(file);
    throw error;
  }
}
export function releaseFacadeLinks(file, metadata) {
  for (const url of [file, metadata]) if (typeof url === 'string' && url.startsWith('blob:')) URL.revokeObjectURL(url);
}
export function yieldFacade() { return new Promise(resolve => setTimeout(resolve, 0)); }
