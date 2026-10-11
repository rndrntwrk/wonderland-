# Connected native player: pose, terrain, audio and recovery

## Delivered integration

This combines the approved native gameplay/audio branch (PR #38, commit
`8b2d6a7197b8b57fa59060f89e3c740b95a540ab`) with the **every-accepted-tick** pose
implementation in PR #39 (`015f00e0955a791286f0013868fa0ab399821686`). PR #38
already contains PR #35's terrain/contact and current-visible selection fixes.
PR #37's competing observed-render-sample implementation is not imported.

The result is ordinary checked-in source. No assembly script is required by a
checkout or build. The parent branches, original game files, root lockfile,
protocol packet formats and authoritative VM behavior are unchanged.

The native lot consumes every returned accepted avatar frame before render
notifications can coalesce, then separately consumes the same complete validated
runtime outcomes for sound and activity. A correlated receipt alone settles the
independent acknowledgement deadline, after releasing the mutable runtime borrow.
Presentation capture failure diagnoses/reset poses without dropping accepted
simulation transitions. A malformed batch cannot publish partial poses, sound or
completion history.

The combined component retains exact native entity/generation/GUID, content-bank
and connection ownership. Terrain height and Hidden!=0 rendering/picking remain
intact. Ordinary ticks preserve valid retained skeletal channels; resource and
checkpoint boundaries reset unavailable visual history instead of inventing it.
Duplicate and recovered history cannot replay sound or uncertain player actions.

## Audio root cause and correction

PR #38's hosted Native browser run `37598348303` failed in the sound journey
with `No real playing voice`. Its other four workflows passed. The failure is
not recast as a passing run.

The native sound interval previously called `silence()` whenever its callback
arrived more than 250 ms after the preceding callback. That conflated ordinary
**foreground event-loop delay** with background/device suspension and destroyed
already accepted looping sounds. The underlying AudioCadence already discards
excess catch-up work; the native host also independently checks document visibility
and AudioContext state.

A new real-browser regression ran the actual PR #38 hosted application, started
an accepted loop, then performed a 400 ms foreground task. The page remained
visible, its AudioContext was running and its lot continued receiving accepted
ticks, yet the loop count fell from one to zero. This reproduced the defect
without changing browser clocks, protocol inputs, UI, CSS or WASM bytes.

The production correction removes only the inferred-suspension branch. The
existing cadence cap, true hidden/non-running device guard, explicit StopSound,
connection cleanup and sample/voice ownership remain in force. The rebuilt test
checks that the **same voice ID** survives foreground delay and volume changes.
Actual Pause sound still releases voices, and Resume sound does not replay old
cues. The fixture requires a separately selected new accepted action to play again.

## Verification completed locally

All final application journeys use the same optimized Rust/WASM build. They are
controlled service tests, not deployed-server or full-content qualification.

| Gate | Result |
| --- | --- |
| Full native workspace | 1,466 passed, zero failed; five existing optional checks ignored |
| Native socket/deadline/device Node tests | 27 passed, zero failed/skipped |
| Existing audio Node tests | 59 passed, zero failed/skipped |
| Formatting and strict native/WASM Clippy | Passed |
| Optimized Trunk application | Built and exercised |
| Native versus ordinary WASM pose probe | All 18,973 output bytes equal; zero host imports; non-shared memory |
| Probe negative controls | Seven equal-but-wrong semantic results and one altered result rejected |
| Standard action/cancellation/responsive journey | 16 check records / 12 captures |
| Avatar/resource/recovery journey | 10 / 6 |
| Elevated terrain/hidden-avatar journey | 12 / 7 |
| Ended-pose journey | 6 / 4 |
| One/three/eighteen-tick packet journey | 8 / 4 |
| Actual receipt-deadline journey | 7 / 4 |
| Game-audio journey, including foreground delay and real device suspension | 14 / 3 |
| Combined terrain/pose/audio/recovery journey | 18 / 3 |

All eight final reports indicate success and have empty recorded error lists.
That is 91 overlapping check records and 43 captures, **not 91 new features**.
The 20 native pose/replay regressions are inherited from the imported PR #39
implementation and are included in the workspace total, not additional cases.
The combined journey passed twice on the unchanged application; repetition is
not counted again. Chromium is 141.0.7390.37 with Playwright 1.56.1. Existing
responsive checks cover 1440x1000, 390x844, 320x600 and 844x390.

The complete-packet regression was also observed failing against the older
audio-only build before combining PR #39. The final combined browser preserves
the intermediate eighteen-tick animation on raised ground, permits mesh picking,
hides/restores the same retained model, produces real audio, survives foreground
load, handles mute/volume/pause/stop, and reconnects without replaying history.
It explicitly expects bind-pose reseeding where the checkpoint lacks prior bones.

### Read-before-paint test race

One inherited terrain assertion read the canvas immediately after the admitted
model count became zero. WorldViewport schedules rendering for its next paint,
so the assertion occasionally observed the previous 89 red pixels. The unchanged
diagnostic rerun passed, and source inspection confirmed this draw ordering. The
final test waits, with a five-second bound, for the actual nonbusy canvas with
zero avatar pixels. It still rejects a permanently stale image and invalid mesh
selection; no renderer behavior or pixel threshold was changed. The earlier
failure is retained separately from the final passing report.

The five ignored source/corpus tests and existing future-Rust/preload warnings
are named in the machine record. Zero recorded browser errors does not mean
warning-free execution. Author self-review of the composed call sites, ownership,
atomic publication and evidence paths is not independent review approval.

## Reproduce

Use the repository-pinned Rust toolchain, Trunk 0.21.14, and locked browser tooling.
Do not copy an old browser release beside new source. These test-only services
must stay isolated from real accounts, wallets, databases and user saves.

```sh
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
node --test --test-reporter=tap apps/web-shell/scripts/native-*.test.mjs
node --test --test-reporter=tap crates/audio-runtime/browser/*.test.mjs
(cd apps/web-shell && trunk build --release --locked)
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
cargo run -p wonderland-web-shell --example native_avatar_resources --locked \
  -- target/native-avatar-fixtures
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install --with-deps chromium
node tools/native-browser/verify.mjs
node tools/native-browser/verify-avatars.mjs
WONDERLAND_NATIVE_TERRAIN_FIXTURE=1 node tools/native-browser/verify-avatars.mjs
node tools/native-browser/verify-pose-retention.mjs
node tools/native-browser/verify-pose-batches.mjs
node tools/native-browser/verify-receipt-deadline.mjs
node tools/native-browser/verify-audio.mjs
WONDERLAND_NATIVE_COMBINED_FIXTURE=1 node tools/native-browser/verify-audio.mjs
```

Set `WONDERLAND_NATIVE_QA_OUTPUT` to a separate empty output directory for each
journey. The receipt-deadline script adds its `receipt-deadline/` subdirectory.
Do not run fixtures sharing their fixed ports concurrently. Add `--offline` to
Cargo/Trunk commands only after caching the exact locked tools/dependencies.
The native/ordinary-WASM conformance commands are in the imported read-only
`native-pose-retention.yml` workflow. The unified read-only `native-browser.yml`
workflow builds tracked source and runs all eight application journeys.

## Acceptance boundary and remaining work

The mesh and sine-wave sample are deliberately authored test fixtures encoded in
original resource formats. They prove decoder, skinning, native replay, selection,
WebAudio and lifecycle integration, **not original human-art or audio-corpus
fidelity**. Browser software rendering and signal measurement are not physical
device/Safari, perceptual speaker, performance or production-load certification.

Native mode remains opt-in. Native checkpoints still lack full historic skeletal
and running-HIT state, so new admission cannot reconstruct unavailable prior
bones or ongoing loops. Head seeking, full SLOT/bone/container attachments,
custom/pet/TS1 formats, walking/contact animation, complete sound providers and
station/hitlist/FSC cohorts remain open.

The full game additionally requires production distinct-player services and
content, durable operation reconciliation, complete Buy/Build/inventory/property,
social/object dialogs, authorized original-content acceptance, and original
FSOv/VMNet translation. The existing legacy connection remains refresh-only.
This checkpoint does not change those requirements into completed features.
Neither parent branch nor main is modified, and no deployment is performed.
Hosted CI for the new publication must be reported from its own runs, separately
from the local results above. Source transfer, if used, checks/stores bytes only.
