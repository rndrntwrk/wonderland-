# Swarm C client delivery recovery

This branch is a new, isolated continuation of PR18 at
`a318581f33770540808aefcf132018255a3a1d94`. The previously named client
continuation had no published ref and was not recovered. This code is not
presented as that lost continuation or as full original-client parity.

## Connected source viewport

The existing `WorldViewport` consumes the same validated `WorldDocument`, source
architecture, models, textures, source identities and scene controls. It now
prepares bounded immutable GPU draw packets rather than rasterizing all pixels
on the browser main thread. The original CPU renderer remains the independent
reference and native test path. Missing models, masks and other baseline source
limitations retain explicit diagnostics; this change does not invent content.

The WebGL2 owner stages shared geometry/textures, renders depth-tested source
color, and executes a separate offscreen RGB24 ID pass for interactive selection.
A pixel-pack buffer and fence make picking asynchronous. New frames, newer picks,
suspension, disposal and context loss reject pending work. Rust validates the
exact decimal frame generation and live `FrameStore` ticket before exposing a
visual target. This is not authentication or permission to execute a game action.

Pan, rotate, zoom, floor/cutaway/roof controls, source selection, DOM controls,
preview Home saves and all existing roster pages remain owned by the existing
client. The new module is bundled through wasm-bindgen's local module mechanism;
it is not a separately hosted or cross-origin script. The full app build must
confirm the generated module is served correctly.

## Verification

`node --test apps/web-shell/tests/gpu/protocol.test.mjs` covers admission bounds,
non-finite values, byte shape, draw references and lossless u64 generations. The
protocol was demonstrated rejecting four tests before implementation; five tests
pass after implementation. That is protocol evidence, not GPU execution.

The `Swarm C client recovery` workflow runs source-world Rust tests, actual WASM
Clippy, and a real Chromium WebGL2 gate. Rust generates the draw packets and an
independent CPU reference. The browser verifies source colors, private selection
indexes, repeated resource replacement, cancelled transfers, real context
loss/restoration, and disposal. Software-renderer results do not qualify physical
hardware or a live multiplayer journey. Formatting differences are retained as a
patch and must be committed before final handoff.

The existing Browser UI workflow remains responsible for the full workspace,
strict formatting/lint, release Trunk build and application bundle. No current
pass is claimed until those jobs finish on a cited source revision.

## Still separate

This increment connects the source 3D viewport and its GPU selection. It does not
complete the previously unshipped source sprite/hybrid composition, advanced
lighting/environment client adapters, continuous original VM restoration, full
native/audio host composition, or PNG/FSOf client exports. The core PR12 libraries
and their tests are distinct from application integration. Authorized content,
live services, full gameplay and physical devices require their own acceptance.
No merge or deployment is included.
