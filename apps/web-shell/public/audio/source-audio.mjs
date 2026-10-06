/** Browser source-file binding. Presentation only; never creates gameplay cues. */
import { BrowserAudio } from './browser-audio.mjs';

const GROUPS = ['Music', 'Fx', 'Vox', 'Ambience'];
const LABELS = { Music: 'Music', Fx: 'Effects', Vox: 'Voices', Ambience: 'Ambience' };
const MAX_SOURCE_BYTES = 128 * 1024 * 1024;
const MAX_SOURCE_FILES = 512;
export const AUDIO_PREFERENCES_KEY = 'wonderland.audio-preferences.v1';
const hex = bytes => Array.from(bytes, v => v.toString(16).padStart(2, '0')).join('');

function exactId(value, allowZero = false) {
  if (typeof value === 'number' && (!Number.isSafeInteger(value) || value < 0)) throw Error('invalid audio identity');
  if (!['string', 'number', 'bigint'].includes(typeof value) || typeof value === 'string' && !/^(0|[1-9][0-9]*)$/.test(value)) throw Error('invalid audio identity');
  const n = BigInt(value);
  if (n < (allowZero ? 0n : 1n) || n > (1n << 64n) - 1n) throw Error('invalid audio identity');
  return n;
}
const voiceKey = v => `${exactId(v.generation)}:${exactId(v.serial)}`;
function sourceKey(value) {
  if (typeof value === 'string' && /^[a-f0-9]{64}$/.test(value)) return value;
  if (!Array.isArray(value) || value.length !== 32 || value.some(v => !Number.isInteger(v) || v < 0 || v > 255)) throw Error('invalid source content key');
  return hex(value);
}
function volume(group, gain) {
  if (!GROUPS.includes(group) || !Number.isFinite(gain) || gain < 0 || gain > 1) throw Error('invalid audio volume');
}
function gainPan(gain, pan) {
  if (!Number.isFinite(gain) || gain < 0 || gain > 1 || !Number.isFinite(pan) || pan < -1 || pan > 1) throw Error('invalid source gain or pan');
}
function requireGesture() {
  if (globalThis.navigator?.userActivation && !globalThis.navigator.userActivation.isActive) throw Error('Sound needs a button press to start. Try Enable sound again.');
}
function message(error) { return String(error?.message ?? error).slice(0, 1024); }

/** Device preferences have their own versioned key; no account or game save is touched. */
export class SourceAudioPreferences {
  constructor({ storage = () => globalThis.localStorage } = {}) {
    this.storage = storage;
    this.volumes = Object.fromEntries(GROUPS.map(group => [group, 1]));
    this.muted = false;
    this.lastError = null;
    try {
      const raw = this.storage()?.getItem(AUDIO_PREFERENCES_KEY);
      if (raw == null) return;
      if (raw.length > 4096) throw Error('oversized sound settings');
      const data = JSON.parse(raw);
      if (data?.version !== 1 || typeof data.muted !== 'boolean' || !data.volumes) throw Error('invalid sound settings');
      for (const group of GROUPS) volume(group, data.volumes[group]);
      this.muted = data.muted;
      this.volumes = Object.fromEntries(GROUPS.map(group => [group, data.volumes[group]]));
    } catch {
      this.lastError = 'Saved sound settings could not be restored. You can still adjust sound here.';
    }
  }
  _save() {
    try {
      const storage = this.storage();
      if (!storage) throw Error('storage unavailable');
      storage.setItem(AUDIO_PREFERENCES_KEY, JSON.stringify({ version: 1, muted: this.muted, volumes: this.volumes }));
      this.lastError = null;
    } catch {
      this.lastError = 'Sound changed, but this browser could not save your settings for next time.';
    }
  }
  setVolume(group, gain) { volume(group, gain); this.volumes[group] = gain; this._save(); }
  setMuted(value) {
    if (typeof value !== 'boolean') throw Error('invalid audio mute setting');
    this.muted = value;
    this._save();
  }
  applyTo(player) {
    for (const group of GROUPS) player.setVolume(group, this.volumes[group]);
    player.setMuted(this.muted);
  }
}

export function waveResource(encoded, { maxPcmBytes = 64 * 1024 * 1024, maxEncodedBytes = 4 * 1024 * 1024 } = {}) {
  if (!(encoded instanceof ArrayBuffer) || encoded.byteLength < 12) throw Error('invalid RIFF WAVE');
  const v = new DataView(encoded), text = (at, n) => String.fromCharCode(...new Uint8Array(encoded, at, n));
  if (text(0, 4) !== 'RIFF' || text(8, 4) !== 'WAVE' || v.getUint32(4, true) + 8 !== encoded.byteLength) throw Error('invalid RIFF WAVE size');
  let format = null, data = null, at = 12;
  while (at < encoded.byteLength) {
    if (at + 8 > encoded.byteLength) throw Error('truncated RIFF chunk');
    const kind = text(at, 4), size = v.getUint32(at + 4, true), end = at + 8 + size;
    if (end + size % 2 > encoded.byteLength) throw Error('truncated RIFF chunk');
    if (kind === 'fmt ') {
      if (format || size < 16) throw Error('invalid RIFF format');
      format = { tag: v.getUint16(at + 8, true), channels: v.getUint16(at + 10, true), sampleRate: v.getUint32(at + 12, true), rate: v.getUint32(at + 16, true), align: v.getUint16(at + 20, true), bits: v.getUint16(at + 22, true) };
    }
    if (kind === 'data') { if (data !== null) throw Error('duplicate RIFF data'); data = size; }
    at = end + size % 2;
  }
  if (!format || data === null || format.tag !== 1 || ![1, 2].includes(format.channels) || ![8, 16, 24, 32].includes(format.bits) || format.sampleRate < 1 || format.sampleRate > 384000 || format.align !== format.channels * format.bits / 8 || format.rate !== format.sampleRate * format.align || data === 0 || data % format.align) throw Error('invalid PCM WAVE fields');
  const frames = data / format.align, decodedBytes = frames * format.channels * 4;
  return { encoded, format: 'wav', sampleRate: format.sampleRate, channels: format.channels, frames, decodedBytes, streaming: decodedBytes > maxPcmBytes || encoded.byteLength > maxEncodedBytes };
}

function sourceBytes(resource) {
  const encoded = resource.encoded?.byteLength ?? 0, blob = resource.blob?.size ?? 0, pcm = resource.pcm?.samples?.byteLength ?? 0;
  const bytes = Math.max(encoded, blob) + pcm;
  if (!Number.isSafeInteger(bytes) || bytes < 0) throw Error('invalid source resource size');
  return bytes;
}

export class SourceAudioPlayer {
  constructor({ backend, contextFactory, mediaFactory = () => new Audio(), urlApi = globalThis.URL, maxSourceBytes = MAX_SOURCE_BYTES, maxSourceFiles = MAX_SOURCE_FILES } = {}) {
    if (!Number.isSafeInteger(maxSourceBytes) || maxSourceBytes < 1 || !Number.isSafeInteger(maxSourceFiles) || maxSourceFiles < 1) throw Error('invalid source resource budget');
    this.resources = new Map();
    this.voices = new Map();
    this.streams = new Map();
    this.completed = [];
    this.volumes = Object.fromEntries(GROUPS.map(group => [group, 1]));
    this.muted = false;
    this.disposed = false;
    this.serial = 0n;
    this.generation = 1n;
    this.mediaFactory = mediaFactory;
    this.urlApi = urlApi;
    this.state = 'locked';
    this.lastNotice = null;
    this.lastGeneration = null;
    this.lastSerial = 0n;
    this.auditionVoice = null;
    this._operation = 0;
    this._loading = false;
    this._maxSourceBytes = maxSourceBytes;
    this._maxSourceFiles = maxSourceFiles;
    this.backend = backend ?? new BrowserAudio({ contextFactory, loadSample: async (key, { signal }) => {
      if (signal.aborted) throw Error('audio load cancelled');
      const resource = this.resources.get(sourceKey(key));
      if (!resource) throw Error('Source sample is unavailable. Load the original audio resource.');
      if (resource.streaming) throw Error('streamed sample cannot be decoded as an effect');
      return resource;
    } });
  }
  _live() { if (this.disposed) throw Error('source audio player disposed'); }
  _gain(voice) { return voice.gain * (this.muted ? 0 : this.volumes[voice.group]); }
  _notice(error) { this.lastNotice = message(error); }
  _clearVoices() {
    this.backend.reset();
    for (const id of [...this.streams.keys()]) this._removeStream(id);
    this.voices.clear();
    this.completed = [];
    this.auditionVoice = null;
  }
  applyAll(items) {
    this._live();
    if (!Array.isArray(items) || items.length > 1024) throw Error('source audio intent batch budget');
    // Forward each intent in order. Holding decoded Starts until after a later
    // streaming generation reset would resurrect the previous scene's voices.
    for (const item of items) {
      if (!item || typeof item !== 'object' || Object.keys(item).length !== 1) throw Error('invalid source mixer intent');
      const [kind] = Object.keys(item), data = item[kind];
      if (!data?.voice) throw Error('voice identity required');
      const id = voiceKey(data.voice);
      if (kind === 'Start') { this._startVoice(data, id); continue; }
      if (!['SetGainPan', 'Pause', 'Resume', 'Stop', 'Release'].includes(kind)) throw Error('unknown source mixer intent');
      const voice = this.voices.get(id), stream = this.streams.get(id);
      if (kind === 'SetGainPan') {
        gainPan(data.gain, data.pan);
        if (stream && data.pan !== 0) throw Error('Streamed music does not support positional pan');
        if (voice) { voice.gain = data.gain; voice.pan = data.pan; }
      }
      if (stream) {
        if (kind === 'SetGainPan') stream.media.volume = this._gain(voice);
        else if (kind === 'Pause') { stream.paused = true; this._pauseStream(stream); }
        else if (kind === 'Resume') { stream.paused = false; if (this.state === 'running') this._playStream(stream); }
        else this._removeStream(id);
      } else {
        this.backend.applyAll([kind === 'SetGainPan' && voice ? { SetGainPan: { ...data, gain: this._gain(voice) } } : item]);
      }
      if (kind === 'Stop' || kind === 'Release') {
        this.voices.delete(id);
        this.completed = this.completed.filter(v => voiceKey(v) !== id);
        if (this.auditionVoice && voiceKey(this.auditionVoice) === id) this.auditionVoice = null;
      }
    }
  }
  _startVoice(data, id) {
    const generation = exactId(data.voice.generation), serial = exactId(data.voice.serial), seek = exactId(data.seek_frame ?? '0', true);
    if (this.lastGeneration !== null && (generation < this.lastGeneration || generation === this.lastGeneration && serial <= this.lastSerial)) throw Error('stale or replayed audio identity');
    gainPan(data.gain, data.pan);
    if (!GROUPS.includes(data.group) || typeof data.looped !== 'boolean') throw Error('invalid source voice parameters');
    const voice = { ...data, sample: sourceKey(data.sample), voice: { generation: generation.toString(), serial: serial.toString() }, seek_frame: seek.toString() };
    const resource = this.resources.get(voice.sample);
    const streaming = resource?.streaming || resource?.blob && this.backend.requiresStreaming?.(resource);
    if (streaming && voice.group !== 'Music') throw Error('Long source samples support Music streaming only');
    if (streaming && voice.pan !== 0) throw Error('Streamed music does not support positional pan');
    if (streaming && seek > 0n && (!Number.isSafeInteger(resource.sampleRate) || resource.sampleRate < 1 || seek > BigInt(Number.MAX_SAFE_INTEGER))) throw Error('Source sample rate is required for a safe stream seek');
    const newGeneration = this.lastGeneration !== null && generation > this.lastGeneration;
    if (!newGeneration && this.voices.size + this.completed.length >= 128) throw Error('source voice budget exceeded');
    const stream = streaming ? this._createStream(voice, resource) : null;
    try {
      if (newGeneration) this._clearVoices();
      if (!stream) this.backend.applyAll([{ Start: { ...voice, gain: this._gain(voice) } }]);
      this.voices.set(id, voice);
      this.lastGeneration = generation;
      this.lastSerial = serial;
      if (stream) {
        this.streams.set(id, stream);
        if (this.state === 'running' || this.state === 'activating') this._playStream(stream);
      }
    } catch (error) {
      if (stream) this._releaseStream(stream);
      throw error;
    }
  }
  _createStream(voice, resource) {
    const id = voiceKey(voice.voice), media = this.mediaFactory();
    const stream = { id, media, url: null, voice: voice.voice, paused: false, attempt: 0, playPromise: null, listeners: [] };
    const listen = (name, callback) => { media.addEventListener(name, callback); stream.listeners.push([name, callback]); };
    const current = () => !this.disposed && this.streams.get(id) === stream;
    try {
      stream.url = this.urlApi.createObjectURL(resource.blob);
      media.preload = 'metadata';
      media.loop = voice.looped;
      media.volume = this._gain(voice);
      listen('loadedmetadata', () => {
        if (!current() || voice.seek_frame === '0') return;
        try {
          let frame = BigInt(voice.seek_frame);
          if (resource.frames && voice.looped) frame %= BigInt(resource.frames);
          if (resource.frames && frame >= BigInt(resource.frames)) throw Error('seek beyond source sample');
          media.currentTime = Number(frame) / resource.sampleRate;
        } catch (error) { this._notice(error); this._completeStream(stream); }
      });
      listen('ended', () => { if (current()) this._completeStream(stream); });
      listen('error', () => {
        if (current()) { this._notice('The browser could not decode the selected source audio. Choose another MP3 or PCM WAVE file.'); this._completeStream(stream); }
      });
      media.src = stream.url;
      return stream;
    } catch (error) { this._releaseStream(stream); throw error; }
  }
  _completeStream(stream) {
    if (this.streams.get(stream.id) !== stream) return;
    this.completed.push(stream.voice);
    this._removeStream(stream.id);
    this.voices.delete(stream.id);
  }
  _playStream(stream) {
    if (stream.paused || this.disposed || this.streams.get(stream.id) !== stream) return Promise.resolve();
    stream.playOperation = this._operation;
    if (stream.playPromise) return stream.playPromise;
    const attempt = ++stream.attempt;
    let played;
    try { played = stream.media.play(); } catch (error) { played = Promise.reject(error); }
    const pending = Promise.resolve(played).then(() => {
      if (this.disposed || this.streams.get(stream.id) !== stream || stream.paused || this.state === 'suspended') stream.media.pause();
    }).catch(error => {
      if (!this.disposed && this.streams.get(stream.id) === stream && attempt === stream.attempt && stream.playOperation === this._operation && !stream.paused && this.state !== 'suspended') {
        this.state = 'interrupted';
        this._notice(error);
        throw error;
      }
    }).finally(() => { if (stream.playPromise === pending) stream.playPromise = null; });
    stream.playPromise = pending;
    // Runtime mixer delivery is synchronous. Gesture callers additionally await
    // this same promise; delivery-only callers still receive snapshot errors.
    pending.catch(() => {});
    return pending;
  }
  _pauseStream(stream) { stream.attempt++; stream.playPromise = null; stream.media.pause(); }
  _releaseStream(stream) {
    this._pauseStream(stream);
    for (const [name, callback] of stream.listeners) stream.media.removeEventListener?.(name, callback);
    try { stream.media.removeAttribute('src'); stream.media.load?.(); }
    finally { if (stream.url !== null) this.urlApi.revokeObjectURL(stream.url); }
  }
  _removeStream(id) {
    const stream = this.streams.get(id);
    if (!stream) return;
    this.streams.delete(id);
    this._releaseStream(stream);
  }
  _refresh() {
    const intents = [];
    for (const [id, voice] of this.voices) {
      const stream = this.streams.get(id);
      if (stream) stream.media.volume = this._gain(voice);
      else intents.push({ SetGainPan: { voice: voice.voice, gain: this._gain(voice), pan: voice.pan } });
    }
    this.backend.applyAll(intents);
  }
  setVolume(group, gain) { this._live(); volume(group, gain); this.volumes[group] = gain; this._refresh(); }
  setMuted(value) { this._live(); if (typeof value !== 'boolean') throw Error('invalid audio mute setting'); this.muted = value; this._refresh(); }
  async unlockFromGesture() {
    this._live();
    requireGesture();
    const operation = ++this._operation;
    this.state = 'activating';
    this.lastNotice = null;
    // Both Web Audio resume and media play are invoked before yielding the
    // gesture. Awaiting context.resume first can lose media user activation.
    const attempts = [this.backend.unlockFromGesture()];
    for (const stream of this.streams.values()) if (!stream.paused) attempts.push(this._playStream(stream));
    const results = await Promise.allSettled(attempts);
    this._live();
    if (operation !== this._operation) throw Error('sound activation cancelled');
    const failure = results.find(result => result.status === 'rejected');
    if (failure || this.lastNotice) {
      const error = failure?.reason ?? Error(this.lastNotice);
      this.state = 'interrupted'; this._notice(error); throw error;
    }
    this.state = 'running';
  }
  async suspend() {
    this._live();
    const operation = ++this._operation;
    this.state = 'suspended';
    for (const stream of this.streams.values()) this._pauseStream(stream);
    await this.backend.suspend();
    if (!this.disposed && operation === this._operation) this.state = 'suspended';
  }
  takeFinished() {
    const result = [...this.backend.takeFinished(), ...this.completed.splice(0)];
    for (const voice of result) {
      const id = voiceKey(voice);
      this.voices.delete(id);
      if (this.auditionVoice && voiceKey(this.auditionVoice) === id) this.auditionVoice = null;
    }
    return result;
  }
  snapshot() {
    const backend = this.backend.snapshot();
    return { ...backend, state: this.disposed ? 'disposed' : this.state === 'running' && backend.state ? backend.state : this.state,
      activeVoices: (backend.activeVoices ?? 0) + this.streams.size,
      completedVoices: (backend.completedVoices ?? 0) + this.completed.length,
      streamingVoices: this.streams.size, loadedSamples: this.resources.size,
      muted: this.muted, volumes: { ...this.volumes }, lastNotice: this.lastNotice };
  }
  _resourceBudget(resources) {
    if (resources.size > this._maxSourceFiles || [...resources.values()].reduce((bytes, resource) => bytes + sourceBytes(resource), 0) > this._maxSourceBytes) throw Error('Loaded source audio exceeds this browser memory budget. Leave this session to release its loaded files.');
  }
  registerResource(key, resource) {
    this._live();
    if (sourceKey(key) !== key || !resource || typeof resource !== 'object') throw Error('invalid source content resource');
    const next = new Map(this.resources).set(key, resource);
    this._resourceBudget(next);
    this.resources.set(key, resource);
  }
  async loadFiles(files) {
    this._live();
    if (this._loading) throw Error('Another source audio selection is still loading.');
    if (!Array.isArray(files) || files.length > this._maxSourceFiles || files.some(file => !Number.isSafeInteger(file.size) || file.size < 1 || typeof file.arrayBuffer !== 'function') || files.reduce((bytes, file) => bytes + file.size, 0) > this._maxSourceBytes) throw Error('Source selection exceeds this browser memory budget.');
    for (const file of files) if (!/\.(mp3|wav)$/i.test(file.name)) throw Error('Select original MP3 or PCM WAVE files. XA and UTK need the source decoder.');
    this._loading = true;
    try {
      const staged = new Map();
      for (const file of files) {
        const encoded = await file.arrayBuffer();
        this._live();
        if (!(encoded instanceof ArrayBuffer) || encoded.byteLength !== file.size) throw Error('Source audio file could not be read completely.');
        const key = hex(new Uint8Array(await crypto.subtle.digest('SHA-256', encoded)));
        this._live();
        if (staged.has(key)) continue;
        const resource = /\.mp3$/i.test(file.name) ? { streaming: true, format: 'mp3' } : waveResource(encoded);
        resource.blob = file;
        resource.name = file.webkitRelativePath || file.name;
        if (resource.streaming) delete resource.encoded;
        staged.set(key, resource);
      }
      const next = new Map([...this.resources, ...staged]);
      this._resourceBudget(next);
      // A malformed later file never leaves hidden, partially loaded resources.
      for (const [key, resource] of staged) this.resources.set(key, resource);
      return [...staged].map(([key, resource]) => ({ key, name: resource.name, streaming: resource.streaming }));
    } finally { this._loading = false; }
  }
  audition(key, { looped = false } = {}) {
    this._live();
    this.takeFinished();
    if (!this.resources.has(key)) throw Error('Choose a loaded music file first.');
    this.stopAudition();
    const voice = { generation: this.generation.toString(), serial: (++this.serial).toString() };
    this.applyAll([{ Start: { voice, sample: key, group: 'Music', gain: 1, pan: 0, looped, seek_frame: '0' } }]);
    this.auditionVoice = voice;
  }
  async auditionFromGesture(key, options) {
    this._live();
    requireGesture();
    if (!this.resources.has(key)) throw Error('Choose a loaded music file first.');
    this.stopAudition();
    // Creating the context happens synchronously, so admission can choose media
    // streaming when WAVE resampling would exceed this device's decode budget.
    const activation = this.unlockFromGesture();
    const operation = this._operation;
    let playback;
    try {
      this.audition(key, options);
      playback = this.streams.get(voiceKey(this.auditionVoice))?.playPromise;
    } catch (error) { activation.catch(() => {}); throw error; }
    const results = await Promise.allSettled([activation, playback]);
    const failure = results.find(result => result.status === 'rejected');
    if (failure) {
      if (!this.disposed && operation === this._operation) { this.state = 'interrupted'; this._notice(failure.reason); }
      throw failure.reason;
    }
    await this.backend.settled();
    this._live();
    const error = this.snapshot().lastNotice || this.snapshot().lastError;
    if (error) throw Error(error);
  }
  stopAudition() {
    if (this.auditionVoice) { const voice = this.auditionVoice; this.auditionVoice = null; this.applyAll([{ Stop: { voice } }]); }
  }
  async dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this._operation++;
    this.state = 'disposed';
    for (const id of [...this.streams.keys()]) this._removeStream(id);
    this.voices.clear();
    this.resources.clear();
    this.completed = [];
    this.auditionVoice = null;
    await this.backend.dispose();
  }
}

let shared = null, accepted = null, preferences = null, controls = null, removeLifecycle = null;
function settings() { return preferences ??= new SourceAudioPreferences(); }
function installLifecycle() {
  if (removeLifecycle || !globalThis.window || !globalThis.document) return;
  const hidden = () => {
    if (document.visibilityState === 'hidden') Promise.allSettled([shared?.suspend(), accepted?.suspend()]);
  };
  const leave = () => { disposeSourceAudio().catch(() => {}); };
  document.addEventListener('visibilitychange', hidden);
  window.addEventListener('pagehide', leave);
  removeLifecycle = () => { document.removeEventListener('visibilitychange', hidden); window.removeEventListener('pagehide', leave); removeLifecycle = null; };
}
export function acceptedAudioHost() {
  if (!accepted) { accepted = new SourceAudioPlayer(); settings().applyTo(accepted); installLifecycle(); }
  return accepted;
}
export function sourceAudioHost() {
  if (!shared) { shared = new SourceAudioPlayer(); settings().applyTo(shared); installLifecycle(); }
  return shared;
}

export function createSourceAudioFocusReturn(dialog) {
  const document = dialog.ownerDocument;
  let opener = null;
  const triggers = () => [...document.querySelectorAll('.source-audio-control .settings-audio')];
  const external = element => element?.isConnected && element !== document.body && element !== document.documentElement
    && !dialog.contains(element) && typeof element.focus === 'function' && element.tabIndex >= 0
    && element.getClientRects().length > 0;
  return {
    capture() {
      if (dialog.open) return;
      // The reactive trigger is disabled while its module opens asynchronously,
      // so native focus may already have fallen back to body at this point.
      opener = [document.activeElement, ...triggers()].find(external) ?? null;
    },
    restore() {
      if (dialog.open || !dialog.isConnected) return false;
      // Resolve again after close: the trigger is now enabled, and a rerender
      // may have replaced the element captured when the dialog opened.
      for (const element of [opener, ...triggers()]) {
        if (!external(element) || element.disabled || element.matches?.(':disabled')) continue;
        element.focus({ preventScroll: true });
        if (document.activeElement === element) return true;
      }
      return false;
    }
  };
}

export function openSourceAudioControls() {
  if (controls) {
    if (!controls.dialog.open) { controls.focus.capture(); controls.dialog.showModal(); }
    controls.refresh();
    controls.watch();
    return;
  }
  const player = sourceAudioHost(), runtime = acceptedAudioHost(), prefs = settings();
  const dialog = document.createElement('dialog');
  dialog.id = 'source-audio-dialog';
  dialog.className = 'source-audio-dialog';
  dialog.setAttribute('aria-labelledby', 'source-audio-title');
  dialog.setAttribute('aria-describedby', 'source-audio-description');
  dialog.innerHTML = `<header class="audio-heading">
    <h2 id="source-audio-title"><span class="icon" aria-hidden="true" style="--icon:url('/assets/icons/volume.svg')"></span>Sound</h2>
    <form method="dialog"><button class="chrome round audio-close" aria-label="Close sound settings"><span class="icon" aria-hidden="true" style="--icon:url('/assets/icons/x.svg')"></span></button></form>
  </header>
  <div class="audio-body">
    <p id="source-audio-description" class="audio-note">Choose original MP3 or WAVE files to listen to while you play.</p>
    <div class="audio-actions"><button class="chrome" type="button" data-enable>Enable sound</button><button class="chrome" type="button" data-pause>Pause sound</button><label class="audio-check"><input type="checkbox" data-mute> Mute all sound</label></div>
    <fieldset class="audio-volumes"><legend>Volume</legend>${GROUPS.map(group => `<label for="audio-volume-${group}">${LABELS[group]}<input id="audio-volume-${group}" type="range" min="0" max="100" step="1" value="100" data-group="${group}" aria-label="${LABELS[group]} volume"><output for="audio-volume-${group}">100%</output></label>`).join('')}</fieldset>
    <label class="audio-field audio-file-label" for="audio-source-files">Original music files<input id="audio-source-files" type="file" multiple accept=".mp3,.wav" data-files></label>
    <label class="audio-field" for="audio-selected-music">Selected music<select id="audio-selected-music" data-playlist aria-label="Selected music"><option value="">Choose a loaded file</option></select></label>
    <div class="audio-actions"><button class="chrome" type="button" aria-label="Play selected music" data-play><span class="icon" aria-hidden="true" style="--icon:url('/assets/icons/player-play.svg')"></span>Play music</button><button class="chrome" type="button" data-stop>Stop music</button><label class="audio-check"><input type="checkbox" data-loop> Repeat on playback</label></div>
    <div class="audio-status"><p data-status role="status" aria-live="polite" aria-atomic="true"></p><p data-storage role="status" aria-live="polite" hidden></p></div>
  </div>`;
  const query = selector => dialog.querySelector(selector);
  const enable = query('[data-enable]'), pause = query('[data-pause]'), play = query('[data-play]'), stop = query('[data-stop]');
  const files = query('[data-files]'), list = query('[data-playlist]'), mute = query('[data-mute]'), status = query('[data-status]'), storageStatus = query('[data-storage]');
  const focus = createSourceAudioFocusReturn(dialog);
  focus.capture();
  const session = { dialog, focus, operation: 0, busy: null, notice: null, timer: null, refresh: null, watch: null };
  controls = session;
  const live = () => controls === session;
  const refresh = () => {
    if (!live()) return;
    player.takeFinished(); // Local presentation only; runtime completions belong to its driver.
    const local = player.snapshot(), game = runtime.snapshot();
    const active = [local.state, game.state].some(state => state === 'running' || state === 'activating');
    enable.disabled = session.busy !== null || local.state === 'running' && game.state === 'running';
    enable.textContent = local.state === 'running' && game.state === 'running' ? 'Sound enabled' : [local.state, game.state].some(state => state === 'suspended' || state === 'interrupted') ? 'Resume sound' : 'Enable sound';
    pause.disabled = !active;
    play.disabled = session.busy !== null || !list.value;
    stop.disabled = !player.auditionVoice;
    files.disabled = session.busy !== null;
    list.disabled = session.busy !== null || list.options.length < 2;
    dialog.setAttribute('aria-busy', String(session.busy !== null));
    mute.checked = prefs.muted;
    for (const slider of dialog.querySelectorAll('[data-group]')) {
      slider.value = Math.round(prefs.volumes[slider.dataset.group] * 100);
      slider.setAttribute('aria-valuetext', `${slider.value}%`);
      slider.nextElementSibling.value = `${slider.value}%`;
    }
    const error = local.lastNotice || local.lastError || game.lastNotice || game.lastError;
    const count = `${local.loadedSamples} source file${local.loadedSamples === 1 ? '' : 's'} loaded.`;
    const state = local.state === 'running' ? (player.auditionVoice ? 'Music is playing.' : 'Sound is enabled.') : local.state === 'suspended' ? 'Sound is paused. Resume sound to continue.' : local.state === 'interrupted' ? 'Sound needs activation. Try Resume sound.' : 'Sound is waiting for activation.';
    status.textContent = session.notice || error || `${state} ${count}`;
    storageStatus.hidden = !prefs.lastError;
    storageStatus.textContent = prefs.lastError ?? '';
  };
  session.refresh = refresh;
  const run = async (action, pending, exclusive = true) => {
    if (!live() || exclusive && session.busy !== null) return;
    const operation = ++session.operation;
    if (exclusive) session.busy = operation;
    session.notice = pending;
    refresh();
    try {
      await action();
      if (live() && operation === session.operation) session.notice = null;
    } catch (error) {
      if (live() && operation === session.operation) session.notice = message(error);
    } finally {
      if (session.busy === operation) session.busy = null;
      refresh();
    }
  };
  const both = async action => {
    const results = await Promise.allSettled([action(player), action(runtime)]);
    const failure = results.find(result => result.status === 'rejected');
    if (failure) throw failure.reason;
  };
  enable.onclick = () => run(() => both(host => host.unlockFromGesture()), 'Enabling sound…');
  pause.onclick = () => run(() => both(host => host.suspend()), 'Pausing sound…', false);
  mute.onchange = () => { prefs.setMuted(mute.checked); prefs.applyTo(player); prefs.applyTo(runtime); refresh(); };
  for (const slider of dialog.querySelectorAll('[data-group]')) slider.oninput = () => { prefs.setVolume(slider.dataset.group, Number(slider.value) / 100); prefs.applyTo(player); prefs.applyTo(runtime); refresh(); };
  list.onchange = refresh;
  files.onchange = () => run(async () => {
    try {
      const loaded = await player.loadFiles(Array.from(files.files));
      if (!live()) return;
      for (const file of loaded) {
        runtime.registerResource(file.key, player.resources.get(file.key));
        let option = Array.from(list.options).find(item => item.value === file.key);
        if (!option) { option = document.createElement('option'); option.value = file.key; list.append(option); }
        option.textContent = file.name;
      }
      if (loaded.length) list.value = loaded[0].key;
    } finally { files.value = ''; }
  }, 'Loading original music…');
  play.onclick = () => run(() => player.auditionFromGesture(list.value, { looped: query('[data-loop]').checked }), 'Starting selected music…');
  stop.onclick = () => run(() => player.stopAudition(), 'Stopping music…', false);
  const unwatch = () => { if (session.timer !== null) clearInterval(session.timer); session.timer = null; };
  session.watch = () => { unwatch(); session.timer = setInterval(refresh, 250); };
  dialog.addEventListener('close', () => {
    // A queued close can arrive after disposal or an immediate reopening.
    if (!live() || dialog.open || !dialog.isConnected) return;
    unwatch();
    focus.restore();
  });
  document.body.append(dialog);
  refresh();
  dialog.showModal();
  session.watch();
}

export async function disposeSourceAudio() {
  const old = shared, oldAccepted = accepted, oldControls = controls;
  shared = null;
  accepted = null;
  controls = null;
  preferences = null;
  removeLifecycle?.();
  if (oldControls) {
    if (oldControls.timer !== null) clearInterval(oldControls.timer);
    if (oldControls.dialog.open) oldControls.dialog.close();
    oldControls.dialog.remove();
  }
  const results = await Promise.allSettled([old?.dispose(), oldAccepted?.dispose()]);
  const failure = results.find(result => result.status === 'rejected');
  if (failure) throw failure.reason;
}
