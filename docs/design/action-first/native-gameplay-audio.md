# Native gameplay sound

Base: PR35, `8d0f78c60a885c6968abe349afd8c89ef890c8f8`.

The native lot now consumes sound presentation events from the existing
`NativePlayer::receive_update` boundary. It uses the existing original-resource
parsers, HIT scheduler, PCM codecs, mixer and WebAudio host. It does not introduce
a second simulation, a sound-driven gameplay acknowledgement or an asset download.

## Player flow

Load a selected local cohort containing `wonderland-audio.json` through **Game
content → Apply files**. Enter a native lot. The **Needs** inspector reports whether
game sound resources are ready. Use the existing **Sound settings → Enable sound**
button to activate browser playback. Effects, voices, music and ambience volume
groups and global mute keep their existing settings. User-selected music uses its
separate host; disposing the native sound lifetime does not dispose that host.

Needs and source Actions share one inspector region. Selecting a source action
closes Needs, and opening Needs closes the source Actions inspector. This prevents
the sound status/needs card from blocking buttons underneath it.

## Accepted-event and device boundaries

* Complete accepted tick batches alone create source cues. Avatar `Sound`
  properties retain their original nested ordinal, even around non-sound
  properties. Sound primitive 23 resolves the scoped FWAV name with global
  fallback, source owner selection and source flags. Primitive 48 stops its exact
  owner. The original engine still decides sound event sharing and HIT execution.
* Cue identity retains lot, authority timeline, tick, event/nested ordinals and
  exact entity generation. Duplicate ticks, checkpoint tails and receipts never
  create a second sound. A wrong timeline or future batch is rejected before
  scheduling; missing resources are reported and retired rather than guessed.
* Browser audio is activation-gated. Events received while sound is locked,
  suspended, unavailable or the page is hidden are discarded, not accumulated.
  Pausing/background suspension stops active native voices; enabling sound again
  requires a later accepted cue rather than replaying an old loop.
* The real engine runs at its existing 60 Hz audio cadence independently of VM
  ticks. A long scheduling gap does not catch up seconds of sound. Positional
  gain/pan uses the same validated orbit camera as the renderer and the existing
  source-derived 3D equations and floor attenuation, not a guessed screen axis.
* A disconnect, recovery request, content replacement or route exit stops native
  voices and invalidates pending sound-host initialization. A replacement connection starts at its
  latest accepted tick. Asynchronous module completion checks the connection and
  content identity before opening a new device lifetime.
* Voice generations and serials cross JavaScript as decimal strings, never
  lossy Numbers. Decoded PCM is copied out of temporary WASM memory views.
  Completion feedback returns only to the sound engine. Only the unchanged,
  validated correlated gameplay receipt path calls the socket's `settled()`.

Sound failures remain presentation diagnostics. They do not roll back accepted
simulation, turn unknown actions into failures, extend receipt deadlines, issue
commands or automatically retry an operation.

## Explicit local manifest

The following is the **authored test cohort**, not a claim that these IDs identify
any installed original game pack:

```json
{
  "version": 1,
  "groups": [{"kind": "new_main", "hit": "sound.hit", "events": "sound.evt"}],
  "tracks": [{"instance": 7, "file": "sound.trk"}],
  "samples": [{"id": 8, "file": "sound.wav", "group": "fx"}],
  "fwav": [{"scope": 75000557, "file": "sound.iff"}]
}
```

Production manifests must use the actual imported source identities: the track
instance and fallback track IDs, TKDT sound ID and FWAV code-owner GUID. No ID is
inferred from a filename. `scope: null` explicitly supplies global FWAV entries.
Group kinds are `new_main`, `relationships`, `tso_ep5`, `tso_v2`, `tso_v3` and
`turkey`; group order/duplicate validation remains in the existing event bank.
An optional `hsm` field names the selected group's HSM file. Sample volume groups
are `fx`, `music`, `vox` and `ambience`.

Paths resolve only inside the selected manifest folder. No path traversal,
absolute path, URL, arbitrary script, filesystem lookup or network fetch is
permitted. Folder imports borrow existing input bytes rather than copying the
whole cohort. Multiple manifests or ambiguous filenames are rejected. Parsing an
invalid new cohort leaves the previously applied avatar/audio content unchanged.

The importer accepts standalone HIT/EVT/HSM/track resources, PCM WAVE, XA and UTK
samples through existing decoders, and original IFF FWAV chunks. It does **not**
automatically expand a complete FAR/DBPF installation or invent a content catalog.

### Limits

Manifest 128 KiB; at most 6 groups, 4,096 track bindings, 128 sample bindings,
512 FWAV files and 4,096 FWAV entries; referenced input 16 MiB per file and 64 MiB
total. Retained sample admission is capped at 8 MiB i16 data per sample and
32 MiB aggregate Float32-equivalent decoded data. Browser session registration
independently enforces its sample count and residency budget. Existing engine,
voice, resource and decoder limits also remain in force. These are bounded
admission budgets, not a whole-process RSS or physical-device performance claim.

## Reproduction

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
node --test apps/web-shell/scripts/native-*.test.mjs
node --test crates/audio-runtime/browser/*.test.mjs
(cd apps/web-shell && trunk build --release --locked)
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install chromium
node tools/native-browser/verify-audio.mjs
```

The audio witness creates a test-only sine-wave PCM sample and original-format
metadata, loads it through the real content UI, and selects test-only native
primitive actions over the controlled WebSocket authority. It samples a real
`AnalyserNode` tapped from the application's existing output graph. It does not
replace the audio backend, inject gameplay acceptance, override autoplay policy,
modify browser time or claim that a physical speaker was heard. Its source action
and sound artwork are explicitly authored fixtures, not original-art acceptance.
The unchanged standard avatar/action/receipt journeys remain separate regressions.

## Remaining qualification

Original human/avatar/sound cohorts and full game parity remain unqualified.
Source object/avatar HIT variable projection, station/hitlist/FSC cohort loading,
restoring already-running source loops from server checkpoints, custom rig/contact
animation and full original audio-host parity are not supplied by this increment.
The existing richer audio library is reused but its unconnected providers are not
renamed complete. The original FSOv/VMNet lane is still refresh-only. Production
native authority/content services, distinct-player multiplayer, durable economic
reconciliation and full Buy/Build/property/social/object dialogs remain separate.

No original files, font assets, lockfile or dependency versions are changed.
No merge, deployment, independent approval or physical-device certification is
implied by local or hosted tests. Check the adjacent verification record and the
named published commit's CI results for the precise executed scope.
