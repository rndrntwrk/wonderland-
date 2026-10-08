// This checks transport integrity, not authority. A receiver must compare an
// independently trusted digest; the file's own receipt cannot authenticate it.
const MAX_BYTES = 16 * 1024 * 1024;
const MAX_PENDING_HASHES = 2;
let pendingHashes = 0;

export async function makeFacadeLinks(bytes, metadata, key) {
  if (!(bytes instanceof Uint8Array) || bytes.length < 9 || bytes.length > MAX_BYTES ||
      (typeof SharedArrayBuffer !== 'undefined' && bytes.buffer instanceof SharedArrayBuffer) ||
      bytes[0] !== 70 || bytes[1] !== 83 || bytes[2] !== 79 || bytes[3] !== 102 ||
      typeof key !== 'string' || !/^[a-f0-9]{64}$/.test(key) ||
      typeof metadata !== 'string' || metadata.length > 65536 ||
      new TextEncoder().encode(metadata).length > 65536) {
    throw new Error('Invalid facade export');
  }
  const details = JSON.parse(metadata);
  if (!details || details.kind !== 'presentation_facade' || details.source_hash !== key ||
      details.bytes !== bytes.length || details.not_a_game_save !== true) {
    throw new Error('Mismatched facade metadata');
  }
  if (typeof details.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(details.sha256)) {
    throw new Error('Missing or invalid facade checksum');
  }
  if (pendingHashes >= MAX_PENDING_HASHES) throw new Error('Facade checksum queue is full');
  pendingHashes++;
  try {
    // wasm-bindgen may lend a temporary WASM view. Snapshot it synchronously,
    // before the first await, and use this SAME immutable copy for the Blob.
    const copy = new Uint8Array(bytes);
    const digest = new Uint8Array(await globalThis.crypto.subtle.digest('SHA-256', copy));
    const actual = Array.from(digest, byte => byte.toString(16).padStart(2, '0')).join('');
    if (actual !== details.sha256) throw new Error('Facade checksum mismatch');
    let file;
    try {
      file = URL.createObjectURL(new Blob([copy], {type: 'application/octet-stream'}));
      const info = URL.createObjectURL(new Blob([metadata], {type: 'application/json'}));
      return [file, info];
    } catch (error) {
      if (file) URL.revokeObjectURL(file);
      throw error;
    }
  } finally {
    pendingHashes--;
  }
}
export function releaseFacadeLinks(file, metadata) {
  for (const url of [file, metadata])
    if (typeof url === 'string' && url.startsWith('blob:')) URL.revokeObjectURL(url);
}
export function yieldFacade() { return new Promise(resolve => setTimeout(resolve, 0)); }
