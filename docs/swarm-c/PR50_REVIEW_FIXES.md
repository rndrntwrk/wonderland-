# PR50 review corrections — 9 October 2026

Base: `add9e160678fc2189622559599c8e280cdd9cd1d`, tree
`0ad11af6fc72305d1512eb13caf8f5746130dd39`. No merge or deployment.

## R1: strict visibility and a usable camera witness

The base already incorporates the review's compositor requirement into every
`canvasPixels` call: retained, idle, disconnect and checkpoint comparisons take
new ordinary screenshots as well as comparing complete accepted framebuffer
bytes. Its blank and stale-nonblank negative controls remain unchanged.

Native browser run 37877107372 failed both pose journeys at their rotated view,
after initial movement and retained-pose checks passed. The shared authored
Vitaboy mesh is one zero-depth triangle. It becomes edge-on during camera turns;
it is not a valid always-visible witness for that assertion. A new real Rust
render test reproduces an empty projection from this planar input. The new
volumetric witness is a four-face tetrahedron with the same resource identifiers,
bone, outfits, texture and animation bytes. Only the browser fixture generator
uses it; the old planar decoder/pose fixture and original game assets are intact.

The new witness must contain actual visible red pixels at all eight tested camera
angles. The actual browser's full-frame equality, compositor-visible identity,
blank/stale rejection, real pointer picking, accepted-tick grouping and receipt
requirements are not loosened. The artwork remains an authored test resource,
not original human-Sim artwork. Fresh browser results belong to the new commit;
the old failure is not relabelled passing.

## R2: cancellable construction, not merely cancellable draw batches

`WorldFacadeJob::new` and final encoding remain synchronous native APIs. The old
browser called that constructor on the DOM thread, so step-level cancellation
could not interrupt validation, lighting, topology/layout and scene preparation.

Ruling: isolate the entire unchanged CPU export in a dedicated Rust/WASM Web
Worker rather than duplicate all preparation algorithms as browser-specific
incremental implementations. The browser can terminate a worker while any of
those routines or compression is executing. No main-thread rendering fallback
exists. Without worker support, the export fails recoverably; the game remains
usable. Native CLI users retain the existing deterministic APIs.

The added `wonderland-facade-worker` binary has an empty startup; it does not mount
Leptos, connect a session or instantiate a live renderer. Its exported function
calls the shipping `execute_facade_worker_request` and produces the same FSOf,
receipt and source hash as native stepped execution. Trunk 0.21.14's existing
worker asset pipeline builds it alongside the application. The bootstrap imports
those generated bindings; no compiled output or source call site is fabricated.

The main Rust panel does only bounded source marshaling and result handling. It
polls while yielding, cancels on the existing generation/source/disposal boundary,
and still awaits checksum verification before publishing download links. The
worker host owns at most one worker, one transferred input and one bounded result.
It copies borrowed WASM bytes before returning, rejects shared memory, never
reuses IDs, rejects stale/wrong-version results, terminates on cancellation,
startup/runtime/message errors or the two-minute watchdog, and consumes results
once. Old cancellation/completion cannot alter a replacement job.

Limits: 32 MiB serialized request, 16 MiB output, 64 KiB receipt, and the existing
source/scene/raster budgets. Snapshot serialization and bounded result copying
still occur on the main thread; this is not zero-copy or physical-device frame
latency qualification. A worker owns an independent ordinary WASM memory; this
requires neither SharedArrayBuffer nor cross-origin isolation. Input/receipt IDs
are disposable task identities, never actor/lot/effect authority.

The worker transfer envelope is internal schema 1, distinct from persistent
WLB1/native wire. Version its ABI when changing this shape. Deploy the generated
worker JS/WASM and bootstrap with the same application release; production
release/cache qualification remains an existing F/D gate, not an implicit result
of this patch. No registry versions or Cargo.lock changes are required.

## Tests and evidence boundaries

The meaningful camera-witness RED rejects the old planar geometry; the solid
fixture passes. The planar negative-control test must still expose its blind
angle. The host's initial unconnected implementation fails all eight new tests;
the connected implementation passes cancellation during unresponsive preparation,
replacement, input/result/identity budgets, copying, one-use outputs, startup,
error and timeout cleanup. These Node tests use a deterministic Worker adapter,
not a claim of actual browser execution.

Three native worker tests compare complete output bytes/metadata with the existing
exporter and reject malformed, oversized or wrong-version inputs. The normal
actual application gate now requires the three packaged worker artifacts, checks
that a real worker started before cancellation, verifies its closure, and proves
each successful export came through a fresh worker and leaves no live worker.
Its existing independent FSOf parser, SHA-256, lit/shadowed pixels, invalid import
retention and graphics-reset checks remain required.

Reproduce using the unchanged compiler, lockfile and browser dependencies:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --bin wonderland-facade-worker --locked -- -D warnings
node --test tools/native-browser/*test.mjs apps/web-shell/tests/gpu/*.test.mjs \
  apps/web-shell/scripts/native-*.test.mjs crates/audio-runtime/browser/*.test.mjs
(cd apps/web-shell && trunk build --release --locked)
```

Run the source-import/export application gate and all native-player journeys on
that same packaged release. Current publication/CI/review conclusions belong in
the PR with exact commit/run IDs; this document makes no premature hosted pass or
independent approval claim. Prior native/compiler/network/setup failures remain
recorded rather than erased.

## Original scope remains

These corrections address PR50's review/acceptance checkpoint, not complete C or
E. Full2D/hybrid, native lighting transport, broader environment/camera/contact,
attachments, audio-provider and checkpoint-history work, complete authorized
content, physical devices and real authenticated multiplayer remain separate.
No original assets, simulation/wire rules, money/ownership state, parent/main
branches or E's SQL/WSS service implementation are changed.
