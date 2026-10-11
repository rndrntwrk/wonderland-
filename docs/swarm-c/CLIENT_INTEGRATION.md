# Swarm C client delivery recovery

This branch is a new, isolated continuation of PR18 at
`a318581f33770540808aefcf132018255a3a1d94`, published in
[PR23](https://github.com/rndrntwrk/wonderland-/pull/23). The previously named
client continuation had no published ref and was not recovered. This is actual
replacement code, not that lost continuation or full original-client parity.

## Connected source viewport

The existing `WorldViewport` consumes the same validated `WorldDocument`, source
architecture, models, textures, source identities and scene controls. It
prepares bounded immutable GPU draw packets rather than rasterizing all pixels
on the browser main thread. The original CPU renderer remains the independent
reference and native test path. Missing models, masks and other source
limitations retain explicit diagnostics; this change does not invent content.

The WebGL2 owner stages shared geometry/textures, renders depth-tested source
color, and executes a separate offscreen RGB24 ID pass for interactive selection.
A pixel-pack buffer and fence make picking asynchronous. New frames, newer picks,
suspension, disposal and context loss reject pending work. Rust validates the
exact decimal frame generation and live `FrameStore` ticket before exposing a
visual target. This is not authentication or permission to execute a game action.

Pan, rotate, zoom, floor/cutaway/roof controls, source selection, DOM controls,
preview Home saves and all existing roster pages remain owned by the existing
client. The GPU module is bundled through wasm-bindgen's local module mechanism,
not a separately hosted or cross-origin script. The actual release application
was built and exercised as described below.

## Verified application handoff — 6 October 2026

The complete [Browser UI run 37438003534](https://github.com/rndrntwrk/wonderland-/actions/runs/37438003534)
passed for implementation head `63dbc9a4c173324610b770dee6b9c3b03ecdfff2`.
GitHub checked out PR merge revision
`63aec33b591b731d3fce4dbfddbce455926572b9`; the application report records that
exact revision. Documentation updates after this implementation do not change the
WASM source used for that result.

The `rust-browser-shell` job passed formatting, native workspace tests, browser
audio bridge tests, strict native and WASM Clippy, and the optimized Trunk release
build. It then exercised the built application through its real DOM entry points
and uploaded both browser evidence and the actual preview bundle. Optional
source/corpus test dispositions remain those in the native logs; this record
does not silently convert ignored tests into executed acceptance.

The application report contains **eight successful scenarios**, with no recorded
page errors or failed requests:

| Scenario | Observation |
| --- | --- |
| Original source world | Actual bundled source lot loaded and rendered; desktop screenshot retained. |
| WASM-resolved GPU selection | GPU selection resolved through Rust to `Tile 37, 37 · Floor 1`. |
| Rotation | Rotated source-world screenshot retained. |
| Import | Another source-world document imported through the application; screenshot retained. |
| Invalid import | Rejected input retained the previously admitted world. |
| Context loss/restoration | Actual WebGL context loss and restoration exercised through the WASM application. |
| Narrow viewport | Source world exercised at 390 × 844; screenshot retained. |
| Close/reopen | Reopening created a fresh GPU owner. |

Desktop captures are 1364 × 936. The recorded browser is Chromium
`151.0.7922.34`. This is software-browser and narrow-viewport evidence, not
physical mobile, speaker, live-service or multiplayer acceptance.

### Immutable evidence identity

- [Application evidence artifact 11400322335](https://github.com/rndrntwrk/wonderland-/actions/runs/37438003534/artifacts/11400322335): ZIP SHA-256 `d009fab923e6a8c8c9a36ecb44c57ee0d2cff1d9c77c12be542b32b84e3d4a34`.
- Its `report.json`: SHA-256 `067d132c6cf643928f2b8af78b1292864ad0923cc0ac7bed0fcde311106dc72e`.
- Release WASM `wonderland-web-shell-646c530fef97dc0_bg.wasm`: 8,454,529 bytes; SHA-256 `23b3401d609fe41a58c42cd868bac6eddb09f63f053b30b8d8d5b2d7c35abb59`.
- [Preview bundle artifact 11400028071](https://github.com/rndrntwrk/wonderland-/actions/runs/37438003534/artifacts/11400028071): ZIP SHA-256 `f1f6e2e03e03a87cc321e58305718a50600176cb15743bde9b9947a44e72269e`.
- Four screenshot hashes and the full release asset manifest are retained in `report.json`. Evidence was downloaded and its ZIP, report and screenshot hashes checked during handoff review.

GitHub artifact retention is finite. Preserve these artifacts with the review
handoff; an expired artifact is not replaced by an unverified rebuild with the
same name.

## Reproduction

Use the pinned workspace compiler and lockfile. From the repository root:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
node --test crates/audio-runtime/browser/*.test.mjs
node --test apps/web-shell/tests/gpu/protocol.test.mjs
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
(cd apps/web-shell && trunk build --release --locked)
```

The exact browser dependency installation and application runner command are in
[the Browser UI workflow](../../.github/workflows/browser-ui.yml). The
`Swarm C client recovery` workflow separately runs source-world Rust tests, WASM
Clippy, a Rust-generated CPU reference and actual WebGL2 color/ID/lifecycle
checks. A protocol test is not a substitute for either the standalone GPU gate
or the built-application scenarios above.

## Still separate

This increment connects the source 3D viewport and its GPU selection. It does not
complete the previously unshipped source sprite/hybrid composition, advanced
lighting/environment client adapters, continuous original VM restoration, full
native/audio host composition, or PNG/FSOf client exports. These are remaining
implementation items, not merely pending physical-device tests. Core PR12
libraries and their isolated tests do not establish their integration into PR23.

Authorized content, live services, full gameplay, physical devices and measured
target-device performance require their own acceptance. The source-lot viewer
reports unavailable scenery rather than pretending all source models and
materials are present. No merge or deployment is included.
