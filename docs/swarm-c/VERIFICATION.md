# Swarm C verification

This ledger records what ran and what each result proves. It intentionally
separates historical local evidence, commit-specific CI, synthetic reference
comparisons and physical acceptance. Do not carry a passing result over to a
modified commit without rerunning the affected checks.

## Recorded evidence

Evidence snapshot: 2026-10-05. Source baseline:
`4c6b3e8f5835b228723caea3c9f683c62f244f73`.

The renderer checkpoint is `95c45b0bdc514669e616648d61258c33563adc5b`.
Its [reference job 111875220500](https://github.com/rndrntwrk/wonderland-/actions/runs/37343212639/job/111875220500)
repeated the native/WASM, source-codec/native-audio and pinned-A successes.
The combined reference job still failed because the already-reviewed audio-test
and formatting corrections were not part of that renderer-only checkpoint.
The library correction batch below resolves those local failures and must receive
its own combined CI result after publication.

## Reviewed library correction batch

The following checks ran on the final correction files on 2026-10-05. They are
local implementation/review evidence, not a green aggregate CI claim.

| Scope | Fresh result | Independent review and limit |
|---|---|---|
| Geometry | 79 debug tests and 79 release tests passed | 8 independent regression cases passed. All 21 runs of the unchanged C# simplifier matched ordered triangle indices and every output position/UV f32 bit. Permanent intermediate-schedule and output bowl regressions caught the arithmetic drift before correction. |
| Avatar | 32 integration tests and 3 compile-fail doctests passed | 6 independent regression/source-vector cases, cooker compilation and the genuine pinned-A probe passed after the identity getter migration. Retagging rig/clip construction identity is impossible through the public API; incompatible poses fail without partial mutation. |
| Audio | 59 Rust tests, 19 actual Node tests and 11 Python tests passed | Reviewer reran 8 focused Rust/Node/Python checks covering all four lifecycle/transaction findings and both cooker fixes. All 32 unchanged-source XA/UTK vectors again produced identical complete WAV bytes. FFplay used a dummy device; no physical output proof. |
| Formatting | Geometry, avatar, audio and iso formatting checks passed | The corrected `Stop { voice: second }` test now compiles. The combined package script still needs a passing run at the published correction commit. |

The source notes and committed regressions record the original equations and
specific fixes. Synthetic/source tests do not replace provider, device or complete
client acceptance.

## Historical CI evidence

| Check | Recorded result | Evidence and limit |
|---|---|---|
| Bevy native build and runtime | Passed at C commit `809ec200e186871970b2e4cabb06d7f6c08ecd9e` | [Run 37337020509, job 111854251163](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251163). Rust 1.95.0, hosted Linux software graphics, bounded native fixture run. No native image-parity or physical-GPU claim. |
| Fyrox native build and runtime | Passed at the same commit | [Job 111854251326](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251326). Same evidence class; the native report explicitly leaves hardware and image parity unqualified. |
| Bevy WebGPU build/package | Passed; browser job failed | [Job 111854251231](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251231). Actual WASM and matching bindgen package were built. Browser readiness timed out; no image qualification. |
| Bevy WebGL2 build/package | Passed; browser job failed | [Job 111854251466](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251466). Browser reached image comparison; fractional CSS capture geometry caused a dimension failure. No color/ID parity result can be inferred. |
| Fyrox WebGL2 build/package | Passed; browser job failed | [Job 111854251146](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251146). Same capture-dimension boundary. |
| Complete reference CI | Failed at both recorded checkpoints | [Earlier job 111854251030](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251030); [latest job 111864173654](https://github.com/rndrntwrk/wonderland-/actions/runs/37339944118/job/111864173654). At the latter, core/iso/geometry/avatar/fixture suites passed, while the new audio test's `Stop { second }` field typo blocked its test compilation and three formatting checks failed. Individual green probes do not override this combined failure. |
| A/C simulation authority boundary | Passed locally and in reference CI at `96066054` | [Probe and provenance](../../tools/swarm-c/avatar-integration/README.md) and the latest reference job: genuine pinned-A execution for 60 ticks with C absent and at 30/60/120 Hz. Every state hash and ordered event vector matched; two genuine A cues ran in each cadence and duplicate accepted ticks produced no extra cues/events. This is a synthetic native integration fixture, not a browser or real-content run. |
| Original-source XA/UTK comparison and native audio | Passed locally and in reference CI at `96066054` | [reference_probe.py](../../tools/swarm-c/audio-cooker/reference_probe.py): 12 XA and 20 UTK synthetic vectors produced exactly equal complete WAV bytes from unchanged C# and Rust decoders. All nine Python cooker/decoder/native-player tests passed, including actual FFplay with a dummy device. Audible output and a live native callback backend remain separate gates. |
| Avatar original-method vectors | Passed for the recorded hashed assembly | [Source result](../../tools/swarm-c/avatar-cooker/source-probe-results.txt) and [scope](avatar-source-notes.md). Extracted original methods, synthetic vectors and explicit tolerance; assembly version is 0.0.0.0. Historical MonoGame 3.6.0.1625 parity is not established. |
| Native/WASM presentation equality | Passed at `96066054` | [verify-native-wasm.sh](../../tools/swarm-c/verify-native-wasm.sh) executed all 18 records with exact equality, zero host imports and non-shared memory. WASM SHA256: `f443cd88401d3b49611e55ed8c2c26b395f187ac8e8cb5a69fb3ff329f4c1667`. Observed runner execution was about 1.14 seconds with 21,364,736 memory bytes; this is reference-algorithm evidence, not GPU/browser performance. |
| Physical devices and live providers | Open | No iPhone/iPad Safari, Android Chrome, desktop physical-GPU or speaker-output qualification, licensed-content cohort, or live E city/admission acceptance is claimed. |

Both native engine jobs also passed at `96066054` in
[run 37339944118](https://github.com/rndrntwrk/wonderland-/actions/runs/37339944118)
and at `95c45b0` in
[run 37343212639](https://github.com/rndrntwrk/wonderland-/actions/runs/37343212639).
The latter compiled and executed the new Bevy compositing and Fyrox LDR paths:
[Bevy job 111875220761](https://github.com/rndrntwrk/wonderland-/actions/runs/37343212639/job/111875220761)
passed 8 library tests and native Vulkan/llvmpipe startup;
[Fyrox job 111875220905](https://github.com/rndrntwrk/wonderland-/actions/runs/37343212639/job/111875220905)
passed 10 tests and native shader/draw execution. Native smoke still does not
establish pixel parity. Browser image parity remains open.

Later successful runs should be appended with their exact commit, command/job,
artifact and scope. Preserve a failed run when it explains a correction; do not
rewrite it as success because the next revision changed.

## Reproduce the pure-library and reference checks

Run from the C repository root with Rust 1.75.0, its rustfmt component, Node 24 and
the dependency locks available:

```sh
bash tools/swarm-c/verify.sh
```

This attempts independent formatting and test checks for all five libraries and
the fixture/replay packages, generates reference output, exercises the browser
audio and host-control tests, and runs the PNG/ID comparator self-test. A failure
does not prevent the other independent checks from reporting. The combined
command exits unsuccessfully if any check failed.

Reference output under `tools/swarm-c/output/reference/` includes color PPM,
little-endian game-ID and depth buffers, fixture hashes/mesh counts in
`manifest.json`, and reference PCM. These are synthetic CPU outputs. The
fixture uses 32/64 instances of a 14-bone, 312-vertex avatar, source-depth sprites,
architecture and a small city. It does not contain a licensed production scene.

## Execute native and ordinary WASM from the same source

Install the `wasm32-unknown-unknown` target for the same Rust 1.75.0 compiler, then:

```sh
bash tools/swarm-c/verify-native-wasm.sh
```

The script builds and executes both targets from the current checkout. The
comparator checks 18 records: three view modes, 32/64 avatars and ticks 0/15/30.
It compares the full observation records, including scene/reference/audio
hashes, geometry counts and picking observations. It also checks zero host
imports, non-shared WASM memory and rejection of invalid mode/count input.

Evidence is written to `tools/swarm-c/output/native-wasm/`: native JSONL,
verification log and a report with WASM SHA256, scenarios, elapsed time and
memory. Any exact mismatch fails. This is an algorithm/cross-target gate; it
does not measure an engine, GPU or browser compositor.

## Audio and original-source comparison

Install Python 3, Mono/mcs, mono-runtime and FFmpeg/FFplay, then:

```sh
bash tools/swarm-c/verify-audio.sh
```

The script builds the actual `audio-decode` binary, runs cooker/decoder/native
player tests, and runs the unchanged-source XA/UTK differential probe. It writes
`tools/swarm-c/output/audio/source-differential.json` plus logs. Source and
synthetic-vector hashes belong in that evidence; do not replace the original
decoder with a rewritten expectation.

Native tests use dummy output and buffered file playback. They prove process,
deadline, pause/resume/stop and decoding behavior, not audible speaker output or
a continuous low-latency callback backend. Browser unit tests use controlled
backend boundaries; actual AudioContext behavior is exercised separately by
the engine/browser runner.

## Pinned A/C authority boundary

Use a separate checkout of the same repository at the exact published A commit.
The runner validates its HEAD and does not alter that checkout:

```sh
SWARM_A_PATH=/absolute/path/to/pinned-A RUSTUP_TOOLCHAIN=1.75.0 \
  python3 tools/swarm-c/avatar-integration/run.py tools/swarm-c/output/avatar-a
```

The temporary probe manifest reads A's libraries and existing synthetic fixture
helpers. It executes actual simulation transitions. Render sampling is read-only,
and source event order/cue counts are compared, not just a C-generated counter.
A's dependencies must be available before the runner's offline Cargo invocation.
The C workflow obtains the pinned provider checkout and fetches its locked
dependencies explicitly.

## Actual engine and browser checks

The [workflow](../../.github/workflows/swarm-c.yml) provisions isolated Rust
1.95.0, matching WASM target libraries and Linux graphics/audio dependencies.
Use an equivalent environment for local runs:

```sh
WONDERLAND_ENGINE=bevy WONDERLAND_BACKEND=native \
  bash tools/swarm-c/build-engine.sh
WONDERLAND_ENGINE=bevy \
  bash tools/swarm-c/run-native-engine.sh
```

Repeat with `fyrox`. For browser runs, select each of
`bevy:webgpu`, `bevy:webgl2`, `fyrox:webgl2` independently:

```sh
WONDERLAND_ENGINE=bevy WONDERLAND_BACKEND=webgl2 \
  bash tools/swarm-c/build-engine.sh
WONDERLAND_ENGINE=bevy WONDERLAND_BACKEND=webgl2 \
  bash tools/swarm-c/run-browser-engine.sh
```

The build wrapper retains compiler/source metadata, dependency resolution and
artifact hashes. Browser packaging requires a wasm-bindgen CLI matching that
engine's lock. The browser wrapper installs the pinned Playwright dependency
and actual Chromium. Both resolved engine Cargo locks and the npm lock were
committed at `95c45b0`; subsequent builds use those graphs. Bevy's WebGPU/WebGL2 feature sets are
mutually exclusive; do not use `--all-features`.

The browser runner checks actual backend observations, ordinary memory without
cross-origin isolation, submitted draws, same-scene color and GPU-ID screenshots,
input focus/cancellation, resize, suspension, AudioContext gesture/lifecycle,
repeated fixture release and actual device/context loss with application reload.
Interactive selection still uses the CPU reference with generation checks;
screenshot ID readback does not implement asynchronous GPU picking.

At the recorded runner revision, the scene matrix uses all three view modes with
32 avatars at DPR 1 and 64 at DPR 2. It is not the full count-by-DPR Cartesian
product. The declared comparison thresholds are zero stable-interior ID
mismatches, color mean absolute byte error at most 4, RMS byte error at most 12,
and at most 3% of channels differing by more than 8. Preserve raw screenshots,
metrics and the reason for any threshold change.

Outputs under `tools/swarm-c/output/<engine>-<backend>/` include build/native
reports or browser JSON, console records, screenshots and traces. CI uploads
these artifacts even after failure, with 14-day retention; retain the acceptance
evidence durably before those artifacts expire. A report's
`rendererQualified: false` remains deliberate: hosted software rendering does
not qualify physical production devices.
