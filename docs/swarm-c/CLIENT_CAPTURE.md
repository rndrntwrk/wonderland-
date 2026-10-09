# Source-world PNG capture — W07.4 client leaf

This increment extends the existing source-world viewer in PR23. It does not
change the world document, camera, simulation, live service, or saved Home.
The initial application source is commit
`ae33d6261bd33430bdefbe100fae04be935db294`. That commit passed native capture,
source-rendering, strict WASM Clippy, Node and actual standalone GPU/PNG checks
in [run 37545931828](https://github.com/rndrntwrk/wonderland-/actions/runs/37545931828).
The complete release/application run remains a separate acceptance gate; consult
its final outcome rather than inferring application success from those checks.

## Player flow

Open **Original lot**, or import a valid source-world document. After the scene
is ready, **Capture PNG** prepares a preview in the existing inspector. **Save
PNG** downloads the scene image; **Photo details** downloads its JSON provenance
record. **Discard photo** releases both temporary browser URLs. These are actual
DOM actions, not a screenshot generator outside the application.

The image contains the source scene without menus, chat, or the DOM selection
outline. It uses the current bounded backing resolution; it does not promise a
higher-resolution export or complete original-content coverage. Missing-source
diagnostics are retained in its details instead of silently claiming full parity.

## Identity and lifetime

Rust associates the capture with the exact successfully painted frame, admitted
`WorldDocument`, camera controls, dimensions and render diagnostics. The GPU owner
redraws the ordinary color pass and calls the native PNG encoder in the same JS
task. It does not reinstall geometry, advance the frame generation, set
`preserveDrawingBuffer`, alter the source shader, or cancel an unrelated GPU pick.

Only the latest request for the still-current scene can produce URLs. Replacement,
view changes, explicit discard, actual context loss, and disposal invalidate
captures. Suspension cancels unfinished encoding. Invalid source imports leave
the previously admitted scene and its already prepared photo intact.

The native browser encoder is asynchronous and cannot be cancelled by this
adapter. The owner caps outstanding encoder callbacks at two, including cancelled
requests; further requests fail with a retry message until those callbacks drain.
The logical request expires after ten seconds. Late encoder or SHA-256 callbacks
cannot resurrect a discarded result. This is a per-owner bound, not a claim of a
global browser-memory quota or physical-device performance qualification.

## Details record

The JSON includes the source provenance and exact revision fields, camera/view
controls, lot dimensions, rendered counts, bounded missing-resource diagnostics,
and the real PNG width, height, byte length and SHA-256. Decimal strings preserve
u64 revision/generation values above JavaScript's safe integer range. The record
identifies itself as `source-view-capture`; its checksum is content integrity,
not an authority signature or authenticated server state.

Surface admission is limited to 1,048,576 pixels with each edge at most 4096.
Encoded images are at most 8 MiB. Rust metadata is at most 65,536 bytes and retains
at most 64 diagnostic messages, each at most 512 characters; truncation is explicit.
There is one ready image/details URL pair per owner, released on replacement or
cleanup. The Rust receipt requires the exact painted generation and dimensions,
local blob URLs, and fixed generation-derived filenames.

## Implementation and verification entry points

- `public/world-gpu.mjs`: standalone wasm-bindgen snippet, GPU owner and bounded
  PNG/URL lifecycle. `world-capture.mjs` is only a compatibility re-export.
- `src/world_capture.rs`: native-testable source metadata and receipt admission.
- `src/world_renderer.rs` and `source_world_screen.rs`: actual Rust DOM controls,
  painted-frame identity, status/preview and recovery integration.
- `tests/gpu/capture.test.mjs`: malformed requests, byte/dimension bounds,
  deadlines, cancellation, delayed hashes, retry and repeated URL cleanup.
- `tests/gpu/packaging.test.mjs`: imports the emitted snippet in isolation,
  detecting unpublished sibling-module dependencies.
- `tests/gpu/browser.mjs`: real GPU rendering, independent CPU reference, exact
  decoded PNG-to-current-color-buffer comparison and interrupted capture/picking.
- `tests/gpu/application.mjs`: actual built-WASM entry, imports, visible capture
  and download links, sidecar identity, narrow layout, context recovery and reopen.

```sh
node --test apps/web-shell/tests/gpu/*.test.mjs crates/audio-runtime/browser/*.test.mjs
cargo test -p wonderland-web-shell world_capture --locked
cargo test -p wonderland-world-view --locked
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
```

Use the existing pinned compiler, lockfile, Trunk and Playwright from the
workflows. Unit success is not proof that the GPU or built application ran;
commit-specific execution results are recorded separately in the PR handoff.
The temporary write-enabled publication workflow and compressed patch transport
were removed after the application code was committed. Regular verification
runs directly on tracked source with read-only permissions.

## Remaining scope

This delivers a current-view PNG export, not an FSOf facade, a source sprite
thumbnail, a saved game, or continuous VM restore. W07.4 still includes broader
source-derived thumbnail/facade integration. Full2D/hybrid source composition,
advanced lighting/environment, accepted-avatar/audio-host integration, live
multiplayer and physical-device qualification remain separate work. The original
17-package C coverage denominator is unchanged. No merge or deployment is implied.
