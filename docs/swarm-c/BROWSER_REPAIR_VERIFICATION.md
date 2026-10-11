# Browser repair — final hosted verification

Verified 6 October 2026. This closes the identified engine-browser failures and
records the actual recovered client handoff. It does not complete every Swarm C
application integration item.

## Fresh full matrix

[Swarm C presentation run 37444262973](https://github.com/rndrntwrk/wonderland-/actions/runs/37444262973)
completed successfully at 09:52:40 UTC with **all seven jobs successful**.
The tested source is `a1e5c6c0b91835c30c260a283d08f029b1cf5dce`, tree
`bfbb65203cbc965a48bd98800caff4cf592a9462`. Subsequent handoff and evidence commits
change documentation only; they are not represented as another runtime build.

| Job | Verified result |
| --- | --- |
| Reference | 441 Rust package tests, zero failed/ignored; 50 Node tests across audio, host, readback, fixture-server and viewport regressions; PNG/ID comparator and both derivative validators pass. |
| Native/WASM and pinned A (within reference job) | All 18 complete records match; zero host imports, non-shared memory. Pinned A runs 60 genuine ticks with C absent and at 30/60/120 Hz, preserving hashes/events with two cues per cadence and no duplicates. |
| Audio codecs (within reference job) | 11 Python tests and 32 synthetic complete-WAV comparisons against unchanged original C# decoders pass. |
| Native audio | CPAL build and actual ALSA-null callback test pass; not physical speaker or latency qualification. |
| Bevy native | Actual native build and software-renderer execution pass. |
| Fyrox native | Actual native build and software-renderer execution pass. |
| Bevy WebGPU | Actual build, GPU picking/readback and ordinary headed-browser image/lifecycle gate pass. |
| Bevy WebGL2 | Actual build, GPU picking and ordinary image/lifecycle gate pass. |
| Fyrox WebGL2 | Actual build, GPU picking and ordinary image/lifecycle gate pass. |

Browser evidence was downloaded and its ZIP/report identities checked, not
inferred from green job labels alone. Chromium is 151.0.7922.34, with software
rendering. WebGPU uses the headed Xvfb path; WebGL2 uses the recorded headless
software path. Physical devices and the historical headless WebGPU compositor
failure are not qualified by the headed result.

| Browser variant | Scenes | Ordinary checks | Lifecycle/diagnostic cases | Physical ID samples | ID mismatches | Maximum color RMS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Bevy WebGPU | 6 | 38 | 18 | 52,719 | 0 | 5.938738 |
| Bevy WebGL2 | 6 | 38 | 16 | 52,719 | 0 | 5.901143 |
| Fyrox WebGL2 | 6 | 38 | 16 | 52,719 | 0 | 6.006997 |

Each variant includes 15,167 ownerless-occlusion samples. The separate GPU
picking runner completes 60 exact picks and 18 interrupted transfers per variant;
all 12 WebGPU raw copies complete. Those private readbacks do not replace the
ordinary page screenshots above. Unchanged limits are zero stable-ID mismatches,
color MAE 4, RMS 12, and channel fraction over eight 0.03.

## Repair and regression proof

`4700c6dce9f46894488a6da08e8d675b84cc8e85` wires `waitForDrawableViewport` into the
actual browser resize and restoration gate. The existing helper had not been
called; the fixed 250 ms pause could observe stale backing dimensions at DPR2.
The test now waits for real CSS size, actual backing pixels and DPR, while
preserving scene identity and rejecting permanently incorrect dimensions. It
never forces a canvas size to satisfy the test or weakens an image threshold.

Seven new tests cover both assembly connections, delayed 450 ms readiness,
permanent mismatch, immediate readiness, shape/DPR requirements and invalid
requests. Before wiring, two assembly tests fail and five behavioural tests pass;
after wiring all seven pass. The first publisher preflight failed on a Node24
reporter-format assumption before publishing the correction. Explicit TAP output
fixed that preflight; the failed run remains retained. Local actual-browser work
was blocked by managed localhost policy before graphics initialization and is
not counted as GPU evidence.

[Guarded repair run 37443889381](https://github.com/rndrntwrk/wonderland-/actions/runs/37443889381)
first verified the new runner against immutable engine artifact 11400893769.
The full matrix above then rebuilt and exercised the actual source. The exact
archives, report hashes and tested source are listed in
[evidence/browser-repair-2026-10-06.json](evidence/browser-repair-2026-10-06.json).

## Recovered application

[PR23](https://github.com/rndrntwrk/wonderland-/pull/23) publishes a replacement
source-3D GPU viewport and asynchronous picking, stacked on PR18. Its actual
[Browser UI run 37438003534](https://github.com/rndrntwrk/wonderland-/actions/runs/37438003534)
passes the workspace tests, audio tests, formatting, strict native/WASM Clippy,
release Trunk build and eight built-application scenarios. Implementation is
`63dbc9a4c173324610b770dee6b9c3b03ecdfff2`, tested through PR merge revision
`63aec33b591b731d3fce4dbfddbce455926572b9`. Client documentation head
`7c2430e08e814d2d5a1c1b8ae5292d82f1260924` changes no runtime source.

Those eight scenarios cover original source-lot entry, Rust-resolved GPU
selection, rotation, valid import, invalid-import retention, actual WebGL
loss/restoration through WASM, 390×844 narrow layout and close/reopen with a fresh
GPU owner. The report records no page errors or failed requests and retains four
screenshots. Exact bundle and WASM identities are in the
[client guide](https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/docs/swarm-c/CLIENT_INTEGRATION.md).

## Remaining code and qualification

The earlier comprehensive client continuation was not recovered. PR23 does not
yet deliver source sprite/hybrid/Full2D composition, advanced lighting/environment
client adapters, full accepted-avatar/audio-host composition, PNG/FSOf client
exports or continuous original VM restoration. These remain implementation
items; passing the core library or engine fixtures does not provide the missing
application code. The original 17-package scope remains in [COVERAGE.md](COVERAGE.md).

Complete authorized source cohorts, live city/admission/multiplayer service
acceptance, physical browser/GPU/audio devices, measured target performance and
final engine selection are also separate. Both PRs remain unmerged and
undeployed. Earlier handoffs are preserved byte-for-byte under
`evidence/pre-recovery-handoff/`; unsupported full-client claims there are not
current acceptance.
