# Wonderland audio runtime

W10 is a bounded, source-derived HIT/event/FSC/station runtime with ordered mixer intents, replay-safe causal cues, PCM decoding/mixing, and explicit browser/native playback adapters. It is an independent Rust 1.75 workspace under MPL-2.0. Both debug and release arithmetic overflow checks are enabled, and unsafe Rust is forbidden.

The full **97-opcode, 39-ambience, and 23-station census**, source quirks, provenance, and codec boundaries are in [audio-source-notes.md](../../docs/swarm-c/audio-source-notes.md). The shipped sample programs and compressed vectors are synthetic. An authorized game installation and a versioned B provider adapter are required for real-content qualification.

## Integration boundary

A remains authoritative for simulation. This crate receives already accepted sound outputs, and all bytecode execution, random selection, playback clocks, decode completion, interruption handling, and voice cleanup remain presentation work. Run `AudioSystem::tick` at a fixed **60 Hz audio cadence**, independent of the renderer's frame rate. Do not advance A from an audio callback.

B owns raw HIT/HSM/EVT/TRK/HLS/audio metadata parsing. C receives normalized `ResourceGroup`, `HitCatalog`, `SampleRef`, and `codec::SampleMetadata` values. Resource identity is the shared core `AssetKey`; the provider must maintain a verified, authorized key-to-bytes mapping. No raw B parser was copied, and this Rust 1.75 crate has no direct B dependency.

The main public entrypoint is `wonderland_audio_runtime::system::AudioSystem`. Construction from normalized provider values is:

```rust
use std::sync::Arc;
use wonderland_audio_runtime::{
    cue::CueLedger,
    hit::{HitCatalog, HitHost, HitLimits},
    runtime::{AudioRuntime, EventBank, ResourceGroup},
    system::{AudioContent, AudioSystem},
    Result,
};

fn make_audio(
    lot: u64,
    timeline: u64,
    voice_generation: u64,
    audio_seed: u64,
    catalog: HitCatalog,
    groups: Vec<ResourceGroup>,
    content: AudioContent,
) -> Result<AudioSystem> {
    let limits = HitLimits::default();
    let bank = EventBank::new(groups, &limits)?;
    let host = HitHost::new(Arc::new(catalog), limits, audio_seed, voice_generation)?;
    let ledger = CueLedger::new(lot, timeline, 8_192)?;
    AudioSystem::new(AudioRuntime::new(host, bank), ledger, content)
}
```

Supply complete original HIT bytes, including the header. Entrypoint and branch PCs remain absolute. `HitProgram::mode` distinguishes TSO PCs from the declared TS1 translation table. A missing/malformed program, unknown event, unavailable sample, or exceeded budget returns a typed error or a player fault; these conditions never produce fake decoded PCM.

### Cue admission and identity

`CueId` contains lot, timeline, simulation tick, source event ordinal, nested property ordinal, and optional generation-tagged owner. A transport/reconnect epoch is deliberately absent. `projection::project_request` accepts the A-view `SoundRequest` (`opcode: u16`) and a normalized `FwavProvider`. `project_avatar` preserves non-audio properties' positions when deriving nested audio ordinals.

| Method | Contract |
|---|---|
| `AudioSystem::accept(&AudioCue)` | Returns New, Duplicate, or Retired; only successful actions commit a causal identity |
| `CueLedger::check(&CueId)` | Nonmutating identity, retirement, duplicate, and capacity preflight |
| `CueLedger::admit(&CueId)` | Records a newly accepted identity for lower-level integrations |
| `ledger.retire_through(tick)` | Advances the irreversible reconciled-history watermark and frees older entries |
| `reconcile_owners(&[EntityRef])` | Removes stale entity generations from presentation ownership |
| `stop_all()` | Returns ordered cleanup intents and retains causal history |

Do not call `ledger.admit` before `AudioSystem::accept`; the latter owns its transaction. Invalid/missing events and capacity rejections leave the ledger unchanged, so the same cue can retry. Eager music replacement is staged before existing music fades or stops. The audio RNG is restored on rejected admission.

Advance retirement only after the accepted timeline is reconciled through that tick. There is no silent bounded-cache eviction that permits an old sound to replay. Replacing the lot/timeline requires a new system and a monotonically newer voice generation; a device reset on its own does not erase causal history.

### Ordered playback and ownership

`AudioSystem::tick() -> Result<Vec<MixerIntent>>` is the combined scheduler for HIT, source special station/music events, ambience loops, and FSC. It retains cross-kind insertion order, queued-start ordering, pending-music promotion, and FSC activation order. `AudioRuntime` is also available for integrations that need HIT events alone.

`MixerIntent` uses externally tagged serde variants:

- `Start { voice, sample, group, gain, pan, looped, seek_frame }`
- `SetGainPan { voice, gain, pan }`
- `Stop { voice }`, `Pause { voice }`, `Resume { voice }`, `Release { voice }`

`VoiceId { generation: u64, serial: u64 }` is ordered and monotonically admitted. Groups are `Fx`, `Music`, `Vox`, and `Ambience`. Gain/pan in an intent is already resolved; valid gain is 0–1 and pan is -1–1. `set_master` mutes/scales a group without deleting its logical cues. Owner volume submissions retain the loudest owner and the first equal-gain pan; nightclub suffix selection retains the source later-equal-gain winner.

Use `complete_voice` for backend voice completion/failure feedback and `take_faults` for bounded player diagnostics. Neither is an A acknowledgement. Completion queues in both backends retain capacity until the presentation host drains them, making a stalled consumer visible instead of dropping lifecycle events.

## Native PCM and playback

`pcm::NativeMixer::new(sample_rate, max_voices, max_pcm_bytes)` consumes the same ordered intents. Install decoded `PcmBuffer` values using `insert_sample(AssetKey, pcm)` before applying their Start intents. `render(frames)` returns interleaved **stereo PCM16** with an integer-phase zero-order hold resampler and linear pan attenuation.

At 48 kHz, one audio tick corresponds to 800 output frames. Mix/render chunk boundaries do not change the integer sample position. Each render call is bounded to at most one second. Sample cache bytes and entries, active voices, and undrained completions are bounded. Active samples cannot be evicted; `evict_sample`, `resident_bytes`, `take_finished`, and `reset` expose ownership. Reset retains the accepted voice watermark.

`codec::encode_wave(&PcmBuffer)` produces a portable PCM16 WAVE artifact. [adapters/native_player.py](adapters/native_player.py) owns an explicitly external FFplay child to play a bounded WAVE file:

```python
# Load adapters/native_player.py as a module from the host.
player = NativePlayer(executable="/usr/bin/ffplay")
print(player.capabilities())
player.start("/authorized/rendered.wav", root="/authorized", deadline=30)
# poll(), pause(), resume(), stop(), and dispose() belong to presentation.
player.dispose()
```

This is **buffered-file playback**. `capabilities()` reports executable availability, POSIX pause/resume support, the selected SDL driver, `live_device_callback: false`, and `physical_output_verified: false`. No live low-latency device callback is implemented by this Python adapter. Tests use the explicitly silent SDL `dummy` driver and do not certify physical speakers.

### Continuous native callback

The separate [native transport](native/README.md) connects the actual `NativeMixer`
to a bounded atomic stereo ring. A worker owns mixing, resampling, decoded PCM,
commands and completion delivery. The data callback copies PCM16, F32 or U16
without allocation, deallocation, decoding, locking or waiting. Its private
session/instance tokens fence reset and reopen; cancellation and event backpressure
retain their ownership until drained.

The [CPAL 0.15.3 binding](native/cpal/README.md) selects explicit stereo
rate/format/buffer bounds and owns the real native stream. It is isolated from
this Rust 1.75 package and its transport tests. The transport passed 25 debug
and 25 release tests plus 12 independent challenges. Device compilation, all four
CPAL configuration tests and the actual OS ALSA-null smoke passed at `bc529fd4`
in [native-audio job 111899190176](https://github.com/rndrntwrk/wonderland-/actions/runs/37350305093/job/111899190176).
Use that job's retained dependency lock for subsequent locked runs. Physical
output, unplug/reopen and production-load latency remain separate gates; unknown
device-buffer ranges remain unsupported.

Drain `Event::Command` to observe sample/mixer admission, and forward natural
`Event::Finished` only to C's `complete_voice`. Native completion tracks the
application callback's consumed frame boundary. It neither advances A nor proves
that physical output has reached a speaker. The native guide gives the complete
integration and lifecycle contract.

## Browser Web Audio adapter

Import the pure module at [browser/browser-audio.mjs](browser/browser-audio.mjs):

```js
import { BrowserAudio } from "./browser/browser-audio.mjs";

const audio = new BrowserAudio({
  loadSample: async (assetKey, { signal }) => {
    // Resolve an authorized entry in the host's content-addressed provider.
    return provider.loadAudio(assetKey, { signal });
  },
  maxVoices: 128,
  maxPendingDecodes: 16,
  maxPcmBytes: 64 * 1024 * 1024,
  maxEncodedBytes: 4 * 1024 * 1024,
});

enableSoundButton.addEventListener("click", async () => {
  await audio.unlockFromGesture();
});
```

The default factory creates the platform AudioContext only on a gesture. `contextFactory` is injectable for a host or controlled test context. Do not await unrelated work before the gesture unlock; the adapter checks the platform's active user-activation signal when available.

Provider results are one of:

```js
{ pcm: { sampleRate: 22050, channels: 1, samples: new Int16Array(/* interleaved */) } }
{ pcm: { sampleRate: 22050, channels: 2, samples: new Float32Array(/* interleaved */) } }
{ encoded: arrayBuffer, format: "wav", decodedBytes: expectedFloat32PcmBytes }
{ encoded: arrayBuffer, format: "mp3", decodedBytes: expectedFloat32PcmBytes }
```

Encoded resources use actual `decodeAudioData`; codec availability/failure is explicit. The expected float32 byte size reserves memory before decoding, and actual returned dimensions must fit it. The platform codec's internal temporary allocations remain a browser capability boundary. XA/UTK must first be cooked to PCM/WAVE.

`apply` / `applyAll` accept Rust's externally tagged intent shape. A sample key is a 32-byte array or lowercase 64-character hexadecimal string; arrays are snapshotted on admission. Serialize u64 voice/seek values as exact decimal strings at the JavaScript boundary if they can exceed Number's safe integer range.

```js
audio.apply({
  Start: {
    voice: { generation: "1", serial: "1" },
    sample: authorizedAssetKey,
    group: "Fx",
    gain: 1,
    pan: 0,
    looped: false,
    seek_frame: "0",
  },
});
```

Use `suspend()`, `resumeFromGesture()`, `reset()`, and `dispose()` for host lifecycle. `snapshot()` reports state/budgets/diagnostics. `settled()` is a test/provider-drain helper; a provider that never resolves or honors cancellation can keep it pending, so do not await it on the simulation or render path. Cancelled decodes retain their budget until they settle. Replacement voices cannot attach to the aborted request for the same asset.

Paused voices retain phase. Resume while the context is suspended queues a restart for gesture recovery. Interruption drops one-shots and retains loops; replacement of a closed context requires a gesture. Async completion cannot revive stopped/disposed voices. `takeFinished()` returns exact `{generation, serial}` decimal-string objects for the cosmetic host to pass to `AudioSystem::complete_voice`.

## Build and verify

From the repository root, using a provisioned offline Cargo home:

```sh
cargo test --offline --manifest-path crates/audio-runtime/Cargo.toml
cargo fmt --manifest-path crates/audio-runtime/Cargo.toml -- --check
cargo build --offline --manifest-path crates/audio-runtime/Cargo.toml --bin audio-decode
node --test crates/audio-runtime/browser/*.test.mjs
AUDIO_DECODER=/absolute/path/to/audio-decode python3 -m unittest discover -s tools/swarm-c/audio-cooker/tests -v
```

For this checked-in revision the complete local results were **59 Rust tests, 19 Node tests, and 11 Python tests**, all passing with no skips. The Python suite used real external FFmpeg and FFplay with synthetic audio and a silent SDL device.

The continuous transport has its own locked test commands in
[native/README.md](native/README.md). Its passing local tests do not extend the
older parent-package result to the separate CPAL dependency graph. That graph
and the actual ALSA-null stream have their own successful `bc529fd4` job. The
[Swarm C verification ledger](../../docs/swarm-c/VERIFICATION.md) tracks exact CI
revisions and the remaining physical/platform/load gates.

The [cooker README](../../tools/swarm-c/audio-cooker/README.md) gives the additional original C# comparison command. Its **32 cases** compare complete WAVE bytes (12 XA and 20 UTK), with retained hashes in [reference-evidence.json](../../tools/swarm-c/audio-cooker/reference-evidence.json). They do not establish real game payload reachability. Supported deployment targets still need real browser gesture/device qualification and authorized content fixtures.
