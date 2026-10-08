# C renderer / accepted native player integration

## Pins and scope

This is an isolated continuation of PR47 (`0c9f2c194bd6280d16cf6c4438941c1974da2690`,
tree `5d67dc06605383a8ec96afa7c1606c6b53b4382b`) with the cumulative C presentation
changes from common PR18 `a318581f33770540808aefcf132018255a3a1d94` through PR41
`41b1c76a6e80d8546640db00af79c3bbad0d11c1` (tree
`5750a1a491452d7e50f8df18f678732d1aeaf254`). It does not re-merge PR37's competing
observed-render-sample history. PR47 already incorporates every accepted tick,
terrain/current-visibility picking, gameplay audio and receipt deadlines.

Approved basis: the source plan's W07–W10 and W17.2 integration target. No baseline
capability is retired. No parent/main branch, original asset, simulation semantics,
server authority, SQL adapter or deployment is changed. The offline source archive
was matched against all 6,269 original Git blobs before editing; the initial local
Git index's CRLF normalization was corrected to the published raw bytes. The
local reconstructed base tree matches PR47 exactly, but its local commit identity
is not the published commit identity. Apply the delivered patch to the published
PR47 commit, not to the archive-local commit or to PR41.

## Product changes

The same WorldViewport now consumes the C WebGL2 color/ID pipeline for the native
player and source-world viewer. Source normal/portal masks, lightmap safety,
generation fencing, PNG capture and FSOf/receiving tools coexist with native
Vitaboy meshes, every-tick pose history, terrain placement, current-scene pick
validation, independent action receipt deadlines and accepted gameplay sounds.
The source-world inspector retains its export controls. No new native HUD button
or live-lighting provider is implied by reusing its renderer.

### Avatar addressing and exports

Native Vitaboy models use wrap addressing, whereas source FSOm/architecture uses
clamp. Schema 4 makes that per-draw distinction explicit. Schemas 1–3 retain their
existing shape and reject the new field; schema 4 requires it on every draw.
GLSL wraps the **interpolated** UV with `fract`, never individual input vertices.
The actual color and ID loop resets `uWrap` on every draw. The same selector drives
the independent CPU reference and loaded-world facade export. Source lightmap
sampling retains its separate clamped bilinear behavior.

The new schema is a disposable Rust/JavaScript render packet, not a multiplayer
protocol or an authority grant. The loader's existing failure and graphics-context
recovery paths remain. No browser engine/backend qualification is inferred from
native GLES execution.

### Native posed resources versus immutable source resources

C rejected every mesh change under an unchanged content-pack identity. Native
accepted animation intentionally changes CPU-skinned geometry on each accepted
tick while the content-pack identity remains fixed. `renderer/native_pose.rs`
allows only the native adapter's LiveSession posed-model convention on a newer
tick with the same provenance/lot/epoch/content and unchanged static materials.

Static non-Vitaboy models must remain identical. Posed models must be referenced
by generation-aware native entities, not blueprint/snapshot records, and the
owner's visual revision must equal the accepted tick. Their pose/content keys
must follow the adapter's existing convention. Reusing a pose key for different
model bytes or a texture key for different image bytes fails. Hidden removal and
new entity generations are supported. A failed replacement retains the old GPU
frame/pick. This is a presentation consistency guard; the native replica/adapter
still owns authoritative admission and resource decoding. It is not safe to use
renderer metadata to authenticate a remote sender.

### WLB1 binary compatibility

A file-facing optional lighting field was inserted into WorldDocument. Its serde
`skip_serializing_if` attribute does not introduce a positional binary option:
a derived bincode decoder tried to consume absent bytes and shifted the rest of
native Bootstrap. This broke the inherited native-player tests after integration.

`live_wire/player/appearance_v1.rs` serializes the original ordered eleven-field
appearance record and reconstructs it with no light recipe. A golden fixture has
exactly 12,796 bytes and SHA-256
`f72071935f91247083da73dde5a8ea724ca016f2c6a01b96d99ba4c38f957d9c`, matching the
unmodified PR47 encoder. NativePlayer successfully opens those bytes.

**WLB1 rejects a lighting recipe or newer world schema. It does not silently drop
one or advertise a wire upgrade.** Actual native room-lighting transport still
needs an explicitly versioned F/E contract and provider. Static source-world
lighting is usable in the file/document path. The remaining parent schemas and
bounded canonical native decoder are unchanged.

## Regression and execution record

The initial integrated workspace reproduced sixteen native-player failures. New
behavioral regressions reproduced the WLB1 layout failure, wrap/clamp mismatch in
live/facade/GPU paths, and accepted pose rejection. The final suite additionally
covers static/texture/pose-key tampering, failed-replacement atomicity, hidden
removal and new generation selection. No game tick, content identity, image
threshold or source action rule was changed to make the checks pass.

The native browser witnesses previously opened Canvas2D on the live canvas and
read a persistent buffer with `toDataURL`. Their new test-only helper reads
**ordinary compositor screenshots**, decodes pixels and maps screenshot centers
to CSS coordinates. It neither requests another product graphics context nor
redraws/replaces the game, disables animation or uses a private GPU readback.
A separate detached Canvas2D used to test image decoding remains unchanged.
The existing action/pose/terrain/audio/deadline journeys stay required in CI.
Fresh execution of those changed browser witnesses remains necessary.

`tools/render-player/gles-packets.mjs` extracts checked-in shader bytes and records
calls made by the production material/texture binders. `verify_gles.py` executes
them on native Mesa GLES and compares CPU color and selection. The eleven scenes
include the prior eight material/lighting cases plus wrapped avatars alone, with
masks and with supplied source lighting. A clamp-only execution mutation must
fail all three avatar color witnesses. Initial coarse whole-image thresholds
missed that small visual error; the added interior avatar samples detect it
without relaxing the existing MAE4/RMS12/fraction-over-eight0.03 or exact-ID rules.
The initial sparse grid also missed a narrow avatar; the fixture now explicitly
requires nonempty, stable, independently selected avatar interior samples.

## Reproduction

Use the unchanged Rust 1.99.0 toolchain, lockfile and existing CI dependencies:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
node --test apps/web-shell/tests/gpu/*.test.mjs tools/native-browser/canvas-evidence.test.mjs
node --test apps/web-shell/scripts/native-*.test.mjs
node --test crates/audio-runtime/browser/*.test.mjs
cargo run -p wonderland-world-view --example gpu_fixture --locked -- tests/output/source-gpu
node tools/render-player/gles-packets.mjs
python3 tools/render-player/verify_gles.py
# Expected to fail on avatar interior color samples, not startup:
WONDERLAND_GLES_FORCE_CLAMP=1 python3 tools/render-player/verify_gles.py
```

The optional native GLES harness requires Mesa/libEGL and Python numpy/Pillow.
It does not download or change those tools. Use the existing native-pose-retention
workflow commands for complete native/ordinary-WASM comparison. The raw Cargo
WASM build is not a packaged Trunk application. For actual browser acceptance,
rebuild through Trunk and run both existing Browser UI/source-world and Native
browser workflows against the **same** integrated tree. Include the new compositor
helper tests, all eight native journeys and source shader/material/lighting/export
checks. Do not inject these JavaScript modules into an older compiled release.

## Review and remaining scope

Independent review is pending, especially for the frozen binary DTO, dynamic-pose
resource exception, asynchronous picks during live ticks and both application
paths. Native/GLES/WASM checks are not rendered-browser, physical-device, original
content, performance or real-service acceptance. The local browser attempt was
blocked at localhost navigation before any checks; it is retained as a failure.

Full2D/hybrid source composition, direct/first-person/environment coverage,
complete native lighting providers, original avatar/contact/outfit qualification,
remaining audio families/variables and historical bone/loop checkpoint recovery
remain open. Production distinct-player services, durable operations and full
Buy/Build/property/social dialogs are also not supplied by this integration.
E's W13.1/W15.1 start independently after contract freeze; this pass neither
implements those services nor makes C rendering an authority dependency.
