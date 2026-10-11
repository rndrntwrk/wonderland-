// Presentation only. No socket, admission ticket, URL or gameplay acknowledgement.
import {acceptedAudioHost} from './audio/source-audio.mjs';
let active = null, generation = 0n;
const MAX_U64 = (1n << 64n) - 1n;
export function createNativeAudioSession(player) {
  active?.dispose();
  const previous = player.lastGeneration ?? 0n;
  generation = (generation > previous ? generation : previous) + 1n;
  if (generation > MAX_U64) throw Error('Native sound lifetime exhausted');
  player.resetVoices();
  const owned = new Map(); let bytes = 0, closed = false;
  const live = () => { if (closed || active !== session || player.disposed) throw Error('Obsolete native sound session'); };
  const session = Object.freeze({
    generation: generation.toString(),
    register(key, sampleRate, channels, input) {
      live();
      if (!/^[0-9a-f]{64}$/.test(key) || !Number.isInteger(sampleRate) || sampleRate < 1 || sampleRate > 384000 || ![1,2].includes(channels)
          || !(input instanceof Int16Array) || input.length === 0 || input.length % channels !== 0 || input.byteLength > 8*1024*1024) throw Error('Invalid native PCM resource');
      if (owned.has(key)) throw Error('Duplicate native PCM resource');
      const cost = input.length * 4;
      if (owned.size >= 128 || bytes + cost > 32*1024*1024) throw Error('Native sound residency limit');
      // Copy before the WASM memory view can be reused/grown. Resource names never
      // become URLs and PCM is decoded by the existing source codec, not eval.
      const resource = {pcm:{sampleRate,channels,samples:new Int16Array(input)}};
      const previous = player.resources.get(key);
      player.registerResource(key,resource);
      owned.set(key,{previous,resource}); bytes += cost;
    },
    deliver(json) { live(); player.applyAll(JSON.parse(json)); },
    finished() { live(); return JSON.stringify(player.takeFinished()); },
    playable() { return !closed && active === session && !player.disposed && player.snapshot().state === 'running'; },
    stop() { live(); player.resetVoices(); },
    dispose() {
      if (closed) return;
      closed = true;
      if (active !== session) return;
      active = null;
      if (!player.disposed) {
        player.resetVoices();
        for (const [key,{previous,resource}] of owned) {
          if (player.resources.get(key) !== resource) continue;
          if (previous === undefined) player.resources.delete(key); else player.resources.set(key,previous);
        }
      }
      owned.clear(); bytes = 0;
    }
  });
  active = session;
  return session;
}
export function openNativeAudio() { return createNativeAudioSession(acceptedAudioHost()); }
