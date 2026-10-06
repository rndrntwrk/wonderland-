# Swarm C delivery status — 6 October 2026

## Published boundaries

Core presentation libraries and engine probes are in PR12 on
`feat/swarm-c-views-audio`. The actual recovered source-world client is in PR23
on `feat/swarm-c-client-integration`, stacked on PR18. Both remain review
branches, not a merged or deployed complete game.

The previously described comprehensive client continuation was not recovered.
PR23 is replacement code for the source 3D viewport and GPU selection. It does
not contain every client adapter named in the earlier C handoff. Those claims
are retained in `evidence/pre-recovery-handoff/` for traceability, not counted as
current delivery. This correction does not retire any requested capability.

## Browser resize correction

Commit `4700c6dce9f46894488a6da08e8d675b84cc8e85` connects the existing
`waitForDrawableViewport` helper to the actual browser gate. Before this change,
commit `92d183d8c5c93ce4a4847b31ff5ca71c2431dc94` contained the helper but did not
call it; a fixed 250 ms pause raced the engine's high-DPI backing-store resize.
Both the smaller viewport and restoration now wait for matching CSS dimensions,
actual backing pixels and DPR. A permanently incorrect drawable still fails.
No page/canvas size is fabricated and no image, identity or DPR threshold changes.

Seven regression tests include delayed 450 ms completion, a permanent mismatch,
invalid requests, immediate readiness, and explicit assembly checks that the
browser runner calls the helper and the reference workflow runs its tests.
The two unwired assembly checks were demonstrated failing before the change;
all seven pass after it. Existing 43 audio/host/readback/fixture-server Node
checks and the PNG/physical-ID comparator self-test also pass locally.
`a1e5c6c0b91835c30c260a283d08f029b1cf5dce` additionally includes the seven existing
fixture-server tests in the regular reference aggregate.

The first guarded publisher attempt stopped before writing because Node 24's
human-readable test reporter did not match its TAP summary check. Explicit TAP
output corrected that preflight. Run 37443169880 remains a failed attempt, not
browser evidence. The local managed browser also blocked localhost before
application execution; that local attempt does not prove graphics correctness.

## Executed WebGPU repair verification

[Run 37443889381](https://github.com/rndrntwrk/wonderland-/actions/runs/37443889381)
passed all guarded-publication and actual browser steps. It tested runner commit
`4700c6dce9f46894488a6da08e8d675b84cc8e85`, tree
`b8c9b05c054252ebf40d1317cb39b5bc07daec0d`.

The engine and independent CPU reference were the immutable previously compiled
artifact 11400893769 from source `92d183d8c5c93ce4a4847b31ff5ca71c2431dc94`.
This isolates the runner correction; it is not a fresh Rust compilation.
The full fresh-build matrix is [run 37444262973](https://github.com/rndrntwrk/wonderland-/actions/runs/37444262973)
at `a1e5c6c0b91835c30c260a283d08f029b1cf5dce`; consult that run for its separately
recorded final job outcomes, rather than inferring them from the isolated run.

The downloaded repair report records `software-browser-checks-passed`, zero
failures, six scenes, 38 checks and 18 lifecycle/diagnostic cases. All 52,719
sampled physical ID centers match, including 15,167 ownerless occlusion centers.
The maximum color RMS is 5.9387374441505285, within the unchanged limit of 12.
The other color limits remain mean absolute error 4 and fraction over eight 0.03;
stable-ID mismatches must remain zero.

DPR1 changes 640×480 → 400×300 → 640×480 actual pixels. DPR2 changes
1280×960 → 800×600 → 1280×960 actual pixels for the corresponding CSS sizes.
All six ordinary page color/ID comparisons pass in headed Chromium
151.0.7922.34 under Xvfb with software graphics. That resolves the configured
headed-software acceptance path, not every browser compositor, physical device,
or the retained historical headless failures.

[Repair artifact 11402936071](https://github.com/rndrntwrk/wonderland-/actions/runs/37443889381/artifacts/11402936071)
contains the red/green logs, exact two-file patch, commit/tree/source hashes,
report, page captures and traces. ZIP SHA-256:
`b0ee217d7c73939f2a99a4bcec7ddbca78164b1bdf780adc27464946b95fcf16`.
Its reused WebGPU WASM SHA-256 is
`ab11bf936f54dfd6a11c80fb278034803042136a953f3e37edc7609c052a5f2a`.

## Built client application

[Browser UI run 37438003534](https://github.com/rndrntwrk/wonderland-/actions/runs/37438003534)
passed for PR23 implementation `63dbc9a4c173324610b770dee6b9c3b03ecdfff2`,
checked out as PR merge revision `63aec33b591b731d3fce4dbfddbce455926572b9`.
It includes native workspace tests, browser audio tests, formatting, strict
native/WASM Clippy, release Trunk compilation and the built application's GPU
browser test. Optional source/corpus dispositions remain in the native logs.

The actual application report contains eight scenarios: original source-lot
entry; Rust-resolved GPU selection; rotation; valid source-world import; invalid
import retaining the admitted world; actual context loss/restoration through
WASM; 390×844 narrow layout; and close/reopen with a fresh GPU owner. It records
no page errors or failed requests and retains four screenshots. This is a
source-world viewer acceptance path, not a multiplayer gameplay journey.

[Application artifact 11400322335](https://github.com/rndrntwrk/wonderland-/actions/runs/37438003534/artifacts/11400322335)
ZIP SHA-256 is `d009fab923e6a8c8c9a36ecb44c57ee0d2cff1d9c77c12be542b32b84e3d4a34`;
its report SHA-256 is `067d132c6cf643928f2b8af78b1292864ad0923cc0ac7bed0fcde311106dc72e`.
The 8,454,529-byte release WASM SHA-256 is
`23b3401d609fe41a58c42cd868bac6eddb09f63f053b30b8d8d5b2d7c35abb59`.
The [client guide](https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/docs/swarm-c/CLIENT_INTEGRATION.md)
records exact build commands and the preview bundle identity. Preserve artifacts
before their finite retention expires.

## Remaining implementation, distinct from qualification

PR23 still needs source sprite/hybrid/Full2D composition, advanced source
lighting/environment adapters, complete accepted avatar/audio-host composition,
and PNG/FSOf client generation/export. Continuous original VM restoration is
also not supplied by this client increment. Their core algorithms, source
oracles and isolated engine tests are not the missing application code.

Authorized complete content, live directory/admission and multiplayer services,
physical browser/GPU/audio devices, measured target-device performance and final
engine selection require additional acceptance. The original source capability
plan remains the denominator. Passing the repaired browser matrix or the eight
application scenarios does not mark every W07–W10/SL.5 item complete.
