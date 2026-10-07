// The WASM owner authenticates admission, validates binary protocols and applies
// accepted ticks synchronously. This module never parses IDs, ticks or commands.
const MAX_PACKET = 80 * 1024 * 1024;
const MAX_WRITE = 65536;
const MAX_BUFFERED = 131072;
const INVALID = 'Invalid native connection';

function address(value, origin) {
  if (typeof value !== 'string' || value.trim() !== value || /[\\?#\u0000-\u0020\u007f]/.test(value)) throw Error(INVALID);
  let url, allowed;
  try { url = new URL(value); allowed = new URL(origin); } catch { throw Error(INVALID); }
  if (url.username || url.password || !['https:', 'http:'].includes(allowed.protocol) ||
      allowed.username || allowed.password || allowed.pathname !== '/' || allowed.search || allowed.hash) throw Error(INVALID);
  const secure = url.protocol === 'wss:' && allowed.protocol === 'https:';
  const local = url.protocol === 'ws:' && allowed.protocol === 'http:' &&
    ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if ((!secure && !local) || url.host !== allowed.host) throw Error(INVALID);
  return url.href;
}

/**
 * Own a single socket. A fresh connection requires a fresh server-issued ticket.
 * onFrame is synchronous and returns null, a binary recovery request, or false
 * to fail closed. ready() is called only after WASM installs a valid checkpoint.
 * dispose() is terminal; reconnect belongs to the Rust session owner.
 * Browser WebSocket has no pre-buffer receive limit: the server MUST cap frames.
 */
export function connectNativeSocket(url, ticket, onFrame, onState, options = {}) {
  const maxPacketBytes = options.maxPacketBytes ?? MAX_PACKET;
  if (!Number.isSafeInteger(maxPacketBytes) || maxPacketBytes < 1 || maxPacketBytes > MAX_PACKET ||
      typeof ticket !== 'string' || ticket.length < 1 || ticket.length > 4096 ||
      /[\u0000-\u001f\u007f]/.test(ticket) || typeof onFrame !== 'function' || typeof onState !== 'function') throw Error(INVALID);
  const allowedOrigin = options.allowedOrigin ?? globalThis.location?.origin;
  const destination = address(url, allowedOrigin);
  const Socket = options.WebSocket ?? globalThis.WebSocket;
  if (typeof Socket !== 'function') throw Error(INVALID);
  const schedule = options.setTimeout ?? globalThis.setTimeout;
  const cancel = options.clearTimeout ?? globalThis.clearTimeout;
  const ws = new Socket(destination);
  ws.binaryType = 'arraybuffer';
  let dead = false, live = false, authenticated = false, timer = null;
  const clearTimer = () => { if (timer !== null) { cancel(timer); timer = null; } };
  const notify = value => { try { onState(value); } catch { /* Do not leak exception payloads. */ } };
  function finish(state) {
    if (dead) return;
    dead = true; live = false; ticket = ''; clearTimer();
    ws.onopen = null; ws.onmessage = null; ws.onclose = null; ws.onerror = null;
    try { ws.close(); } catch { /* Terminal even if the browser cannot send close. */ }
    if (state) notify(state);
    onFrame = null; onState = null;
  }
  function arm() { clearTimer(); timer = schedule(() => finish('failed'), 15000); }
  function write(bytes, requireLive) {
    if (dead || !authenticated || (requireLive && !live) || ws.readyState !== 1 ||
        !(bytes instanceof Uint8Array) || bytes.byteLength < 1 || bytes.byteLength > MAX_WRITE ||
        !(bytes.buffer instanceof ArrayBuffer) || !Number.isSafeInteger(ws.bufferedAmount) ||
        ws.bufferedAmount < 0 || ws.bufferedAmount + bytes.byteLength > MAX_BUFFERED) return false;
    try { ws.send(bytes); return true; } catch { finish('failed'); return false; }
  }
  ws.onopen = () => {
    if (dead) return;
    try {
      ws.send(JSON.stringify({type: 'native_auth', ticket}));
      ticket = ''; authenticated = true; notify('connected'); arm();
    } catch { finish('failed'); }
  };
  ws.onmessage = event => {
    if (dead) return;
    if (!authenticated || !(event.data instanceof ArrayBuffer) ||
        event.data.byteLength === 0 || event.data.byteLength > maxPacketBytes) {
      finish('failed'); return;
    }
    try {
      // No async Blob conversion, promise queue or reactive latest-message slot:
      // the decoder sees every arrival before the next socket task can run.
      const reply = onFrame(new Uint8Array(event.data));
      if (dead) return;
      if (reply === false || (reply != null && !write(reply, false))) { finish('failed'); return; }
      if (live) arm(); // During handshake, malformed traffic cannot extend the deadline.
    } catch { finish('failed'); }
  };
  ws.onerror = () => finish('failed');
  ws.onclose = () => finish('disconnected');
  arm(); notify('opening');
  return Object.freeze({
    ready() {
      if (dead || !authenticated || ws.readyState !== 1) return false;
      live = true; arm(); notify('live'); return true;
    },
    // Only generated protocol-control packets should use this before readiness.
    control(bytes) { return write(bytes, false); },
    send(bytes) { return write(bytes, true); },
    dispose() { finish(null); },
  });
}
