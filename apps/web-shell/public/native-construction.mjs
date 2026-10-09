// Byte-only bridge for construction::exchange. It never parses u64 values,
// manufactures acceptance, retries a call, or changes the game's local state.
export const MAX_CALL_BYTES = 128 * 1024 + 128;
export const MAX_REPLY_BYTES = MAX_CALL_BYTES + 128 * 1024;
export const MAX_REPLY_CHUNKS = 4096;
const REQUEST_TYPE = 'application/vnd.wonderland.construction-call';
const REPLY_TYPE = 'application/vnd.wonderland.construction-reply';

function failure(code, message) {
  const error = new Error(message);
  error.code = code;
  return error;
}
function cancelBody(response) {
  try { Promise.resolve(response?.body?.cancel()).catch(() => {}); } catch { /* Preserve primary failure. */ }
}
/**
 * Endpoint and allowedOrigin must come from trusted application configuration.
 * csrfToken is a server-issued anti-CSRF token, not a grant or an operation ID.
 * The server must independently enforce origin, auth, CSRF, rate and pre-buffer
 * limits. This helper's streamed cap cannot constrain the browser's network heap.
 *
 * Any failure after dispatch makes delivery UNKNOWN and closes this channel.
 * Reconnect through a new channel and query the same operation. Never resend the
 * confirmation just because the promise rejected. Dispose does not undo a debit.
 */
export function createConstructionTransport(endpoint, {
  allowedOrigin, csrfToken, timeoutMs = 15_000, fetchImpl = globalThis.fetch,
} = {}) {
  let url;
  let origin;
  try { url = new URL(endpoint); origin = new URL(allowedOrigin); }
  catch { throw failure('INVALID_CONFIG', 'A configured absolute construction endpoint is required'); }
  const loopback = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if (origin.origin !== allowedOrigin || url.origin !== allowedOrigin
      || url.username || url.password || url.search || url.hash
      || !(url.protocol === 'https:' || (url.protocol === 'http:' && loopback))) {
    throw failure('INVALID_CONFIG', 'Construction endpoint is outside its configured origin');
  }
  if (typeof csrfToken !== 'string' || csrfToken.length === 0 || csrfToken.length > 4096
      || /[^\x21-\x7e]/.test(csrfToken) || typeof fetchImpl !== 'function'
      || !Number.isSafeInteger(timeoutMs) || timeoutMs <= 0 || timeoutMs > 300_000) {
    throw failure('INVALID_CONFIG', 'Construction transport configuration is invalid');
  }
  const target = url.href;
  let csrf = csrfToken;
  let disposed = false;
  let active = null;
  function dispose() {
    disposed = true;
    csrf = '';
    active?.abort();
  }
  async function send(bytes, { signal } = {}) {
    if (disposed) throw failure('CLOSED', 'Construction channel is closed');
    if (active) throw failure('BUSY', 'A construction request is already in flight');
    if (!(bytes instanceof Uint8Array) || bytes.byteLength < 16 || bytes.byteLength > MAX_CALL_BYTES
        || (typeof SharedArrayBuffer !== 'undefined' && bytes.buffer instanceof SharedArrayBuffer)) {
      throw failure('NOT_SENT', 'Construction request has invalid byte bounds');
    }
    if (signal?.aborted) throw failure('NOT_SENT', 'Construction request was cancelled before dispatch');
    // Snapshot WASM-owned memory synchronously before any await/growth/reuse.
    const body = bytes.slice();
    const controller = new AbortController();
    active = controller;
    const onParentAbort = () => controller.abort();
    signal?.addEventListener('abort', onParentAbort, { once: true });
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    let abortListener;
    const interrupted = new Promise((_, reject) => {
      abortListener = () => reject(failure('DELIVERY_UNKNOWN', 'Construction delivery was interrupted'));
      controller.signal.addEventListener('abort', abortListener, { once: true });
    });
    // fetchImpl may throw before the first race is attached. Observe abort
    // rejection immediately; the original promise still rejects every race.
    interrupted.catch(() => {});
    let response;
    let reader;
    try {
      if (signal?.aborted) controller.abort();
      const fetching = Promise.resolve(fetchImpl(target, {
        method: 'POST', credentials: 'include', mode: 'cors', redirect: 'error',
        cache: 'no-store', referrerPolicy: 'no-referrer', signal: controller.signal,
        headers: { 'Content-Type': REQUEST_TYPE, 'Accept': REPLY_TYPE, 'X-Wonderland-CSRF': csrf },
        body,
      }));
      // A test host or unusual adapter may ignore AbortSignal. Cancel any late
      // body, and never return its bytes into a replacement client generation.
      fetching.then(value => {
        if (disposed || controller.signal.aborted) cancelBody(value);
      }, () => {});
      response = await Promise.race([fetching, interrupted]);
      if (disposed || controller.signal.aborted || response.status !== 200 || response.redirected
          || response.url !== target
          || response.headers.get('content-type')?.split(';')[0].trim().toLowerCase() !== REPLY_TYPE) {
        throw failure('DELIVERY_UNKNOWN', 'Construction reply was not admitted');
      }
      const declared = response.headers.get('content-length');
      if (declared !== null && (!/^[0-9]+$/.test(declared) || Number(declared) > MAX_REPLY_BYTES)) {
        throw failure('DELIVERY_UNKNOWN', 'Construction reply exceeded its byte limit');
      }
      if (!response.body?.getReader) throw failure('DELIVERY_UNKNOWN', 'A bounded construction response stream is required');
      reader = response.body.getReader();
      const chunks = [];
      let total = 0;
      let reads = 0;
      for (;;) {
        if (++reads > MAX_REPLY_CHUNKS) throw failure('DELIVERY_UNKNOWN', 'Construction response exceeded its work limit');
        const { value, done } = await Promise.race([reader.read(), interrupted]);
        if (disposed || controller.signal.aborted) throw failure('DELIVERY_UNKNOWN', 'Construction reply is obsolete');
        if (done) break;
        if (!(value instanceof Uint8Array) || value.byteLength > MAX_REPLY_BYTES - total) {
          throw failure('DELIVERY_UNKNOWN', 'Construction reply exceeded its byte limit');
        }
        total += value.byteLength;
        if (value.byteLength) chunks.push(value.slice());
      }
      if (total < 16) throw failure('DELIVERY_UNKNOWN', 'Construction reply is truncated');
      const result = new Uint8Array(total);
      let offset = 0;
      for (const chunk of chunks) { result.set(chunk, offset); offset += chunk.byteLength; }
      return result; // Rust still validates envelope, exact echo, phase and scope.
    } catch {
      disposed = true;
      csrf = '';
      controller.abort();
      if (reader) { try { Promise.resolve(reader.cancel()).catch(() => {}); } catch { /* Preserve uncertainty. */ } }
      else cancelBody(response);
      throw failure('DELIVERY_UNKNOWN', 'Construction delivery is unknown; query its status before any new write');
    } finally {
      clearTimeout(timer);
      signal?.removeEventListener('abort', onParentAbort);
      controller.signal.removeEventListener('abort', abortListener);
      try { reader?.releaseLock(); } catch { /* Disposed reader. */ }
      if (active === controller) active = null;
    }
  }
  return Object.freeze({ send, dispose });
}
