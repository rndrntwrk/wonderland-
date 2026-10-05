# Task 5: original source audio integration

## Revision and ownership

Imported `crates/audio-runtime/{src,tests,browser/browser-audio.mjs,browser/browser-audio.test.mjs,README.md,Cargo.toml}` from exact C revision `f6f78be1fef247f2db47e19f56d94054f0c9e88c`. Removed standalone workspace/profile stanzas from its manifest to use the root workspace. All imported source/decoder/runtime tests remain unchanged. No native device subprojects, generated build directories, probe sine/square waves or source asset bytes were imported.

New owned paths: `crates/audio-content/**`, `apps/web-shell/src/audio_bridge.rs`, `apps/web-shell/public/audio/**`, `apps/web-shell/public/audio.css`, source-player module/tests under `crates/audio-runtime/browser/` and this report. Root owns workspace/web manifests, app/module mounting and Trunk static-copy entries.

## Implemented boundaries

- Bounded original PCM WAVE/XA/UTK metadata → real imported portable decoder → content-addressed `SampleRef`/PCM. Browser allocation is float32 frames × channels × 4.
- HIT/EVT/HSM provider binding retains complete HIT bytes and absolute entrypoint PCs, source group identity and ordered metadata. Original DBPF TRK instance IDs/backup IDs and explicit HLS encodings are bound without invented identities. FWAV scope and original global fallback remain separate.
- `AcceptedAudioSession` wraps real `AudioSystem`, original cue replay ledger, owner-generation reconciliation, logical group volume/mute and presentation completion.
- Browser driver uses a pure fixed 60 Hz accumulator independently of 30 Hz VM ticks. Browser suspension discards presentation backlog. It never retires causal IDs or acknowledges a gameplay operation. Device completion only calls `complete_voice`.
- The source camera-to-gain/pan formula is ported from original `VMEntity.cs:339–406`, including original 3D graphics-axis conversion, squared screen pan, 2.25 3D pan exponent, zoom/floor attenuation and clamp.
- `SourceAudioControls()` has no props and works standalone. Its compact dialog supports gesture activation, pause/resume, mute, Music/Effects/Voices/Ambience volumes, original MP3/WAVE loading, selected-file playlist, repeat and stop.
- Actual mute/group control modifies active Web Audio GainNodes and HTML media volumes. Local file audition uses a distinct adapter from accepted runtime playback so audition cannot consume original runtime voice serials/generations.
- MP3 and long Music WAVE use owned HTML media streaming/blob URLs, avoiding whole-track AudioBuffer allocation. Short PCM WAVE uses the imported bounded BrowserAudio decoder. Streaming identity replay/unsafe numeric identities are rejected. Stop/end/error/dispose release nodes and blob URLs.

## Exact APIs

`audio_bridge::SourceAudioControls() -> impl IntoView` (wasm)

`audio_bridge::mixer_json(&[MixerIntent]) -> Result<String, serde_json::Error>` serializes u64 generation/serial/seek as exact decimal strings.

`audio_bridge::start_audio_driver(Rc<RefCell<AcceptedAudioSession>>, notice: impl Fn(String) + 'static) -> Result<AudioDriver, JsValue>` (wasm). Retain the driver for the source/live incarnation; Drop clears interval and stops owned voices.

`audio_content::session::AcceptedAudioSession::{new,accept,tick,complete_voice,reconcile_owners,set_volume,mute,stop_all}`. The public system also exposes source `submit_volume`, `set_ambience` and causal retirement.

`audio_content::{bind_group,prepare_sample,bind_track,bind_hitlist,SourceFwav}` and `audio_content::position::source_gain_pan(CameraAudio, entity_level, view_level, no_pan, no_zoom)`.

Browser `sourceAudioHost()` is local audition; `acceptedAudioHost()` is the accepted runtime device/provider. Both support resource registration by original-byte SHA-256 key. UI controls affect both.

## Verification observed in this worker

- `cargo test -p wonderland-audio-content -p wonderland-audio-runtime`: **68 passed**, zero failures/warnings in final run (9 new content tests + 59 imported runtime/decoder tests). Covers full HIT bytes/PCs, decoded original RIFF syntax, cue duplication/missing bank rejection, separate presentation completion, mute volume restoration, 60 Hz cadence, source positional formula, owner generations, source opcode/station/FSC policy, decoder budgets and lifecycle.
- `node --test crates/audio-runtime/browser/*.test.mjs`: **25 passed**, zero failures. 19 imported BrowserAudio tests plus 6 source-player tests. Source-player tests execute actual BrowserAudio and inspect the connected GainNode values; only browser platform Context/media interfaces are controlled. Tests cover gesture lifecycle, suspend/interruption/disposal, decode cancellation and quotas, precise u64 IDs, long-file streaming, object URL reclamation, stream replay rejection and float32 decoded allocation.
- New metadata, position, source-player and cadence behavior were first exercised against missing/stub behavior and observed red before passing implementation. The original 3D positional test initially omitted the source VisualPosition×3 conversion in its expected heading; the corrected expectation now follows original source coordinates.
- Root full workspace gate, wasm/Trunk compile and browser evidence belong to the coordinating worker. This worker did not run Cua or claim hearing actual original game audio. Heavy duplicate workspace builds were avoided when root reported disk exhaustion.

## Concrete external gaps

Current installed original test content is avatar-only. No original HIT/EVT/HSM/DBPF audio event bank, FWAV archive provider or FSC sample manifest is present. The boundary supports those inputs but does not fabricate game effects, voices, ambience or an accepted event registration. Source playback UI accepts MP3/WAVE; XA/UTK real decoding is exposed through Rust content APIs rather than a browser archive/import screen.

A local original select-music MP3 is being obtained by root from the verified installer archive for browser verification; it must not be committed. Its browser evidence and actual asset provenance must be reported by root separately. No local music audition, imported crate, completed decode or audio device state establishes a live city/lot session or complete original game audio parity.
