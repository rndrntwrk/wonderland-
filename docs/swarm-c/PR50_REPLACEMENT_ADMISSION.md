# PR50 failed-replacement admission — 9 October 2026

Base: `2f39f0f959140447227a7d68ad523caa19524a0e`, tree
`0115346a327d0b82641178abce77e5196bc7b282`.

Addresses the independent P1 in review `5458842302`: a schema-valid source can
exceed expanded geometry limits or fail device installation. Previously the
source inspector immediately changed its document/title/controls/selection,
then the viewport replaced its Rust state and disposed the graphics owner on
failure. That destroyed the previous frame and pick tickets. The two earlier
inline compositor/worker findings are separate; a successful audio test does
not resolve this third finding.

## Transaction boundaries

`WorldRenderer::update_gpu` forks bounded frame/generation metadata and immutable
prepared-resource Arcs, not software raster buffers. It validates and prepares
an isolated candidate, invokes the adapter once, and commits the candidate only
after device publication succeeds. Rejected preparation, serialization or device
publication leaves the original document, full software raster and GPU selection
metadata intact. FrameStore's clone preserves its generation tombstones and
boundary counters; it does not reset or invent live identity. The additional
staging copy is presentation metadata/resources, never authoritative VM state.

The WebGL2 owner retains the previous resource graph until its first complete
candidate color draw succeeds. Allocation/preflight failure does not change the
old canvas. If a first draw fails after clearing the canvas, the owner restores
its previous dimensions and redraws the previous scene. Pending old ID transfers,
frame generation and ready capture URLs survive successful rollback. Only a
successful replacement retires them. A real lost device or failed rollback is
explicitly unavailable: that case cannot promise preserved pixels or picks.
Repeated refused candidates release their staged resources.

Source import now proposes a document to the viewport. The inspector commits
source/title, clears selection and resets controls only after the matching Arc
has actually been rendered. Failure retains those fields, the ready photo and
the CPU-owned facade. A failed import does not become a facade job's new source.
Completion runs after the mutable renderer borrow is released. Superseded
render requests cannot later overwrite a reverted frame. Re-notifying the same
already displayed document/controls/size is a no-op, not a second installation
that discards the retained export. Device/context recovery remains explicit.

Native accepted simulation continues to own gameplay truth. A refused visual
update does not rewind, alter or drop a committed simulation tick. The displayed
projection can remain old with an error, and native action admission must still
check the current authoritative identity/visibility as before. This change does
not add native lighting to the frozen WLB1 payload or alter a shared protocol.

## Regression evidence

Before the transaction correction, three of four initial Rust regressions fail
against the extracted original browser sequence (replace → prepare → publish):
preparation failure and adapter failure destroy old GPU picks, and adapter
failure destroys the whole CPU raster. The successful replacement control passes.
All four pass after the fix. A fifth test uses a **schema-valid** shared mesh
repeated in 32 instances, causing the normal expanded-scene budget to reject
before device publication; the previous frame/pick remains usable.

The shipping JavaScript owner is tested through a faulting device adapter:
allocation failure, a first-draw failure with changed dimensions, an outstanding
ID transfer, a successful replacement, twenty failed replacements without leaked
resources, and genuine context loss. Two of the original five tests fail on the
old owner; all five pass after correction. These are unit-level device tests,
not an assertion that a fake device is real browser execution. Two additional
assembly tests check the actual Rust/Leptos call sites and deferred import path.
Their old-source failures are retained separately.

The actual existing application test adds two independent failure paths:

1. Import the same valid-but-overbudget document, retaining caption/source,
   selected item, camera, original PNG/FSOf links, frame generation and exact
   regenerated PNG pixels.
2. Import a valid alternate lot while the real WebGL API throws once at its
   candidate draw (the wrapper restores itself immediately). The product must
   redraw the prior scene, retain identical complete PNG pixels and resolve a
   real subsequent GPU pick. A later valid import must still succeed and retire
   the old facade and selection.

No image threshold is relaxed, no WASM is replaced, and the fault test never
supplies a fake framebuffer. Real browser execution and its source revision must
be recorded separately from the Node/native checks. Existing context-loss,
source masks/lightmaps, worker export, all native gameplay/pose/audio/reconnect
journeys remain required on the same packaged release.

## Local checks and review boundary

The candidate passes 182 Node tests (seven new), the five new Rust cases,
affected native/all-target tests, formatting and strict affected native plus
actual web-shell/worker WASM Clippy. Full workspace tests also pass with 1,580
summary passes (including two documentation tests), zero failures and the same
five optional original-source/corpus ignores. Counts overlap and are not additive.
The duplicate fixture module lint was corrected by reusing the shared fixture
module, without suppressing lint or changing dependency versions. Two initial
cold-cache full commands were interrupted by this environment's tool deadline;
the final successful full run is recorded separately.

Reproduce with the pinned toolchain and unchanged lockfile:

```sh
cargo test --workspace --locked
cargo test -p wonderland-world-view --test gpu_replacement --locked
cargo test -p wonderland-world-view --all-targets --locked
cargo clippy -p wonderland-render-core -p wonderland-world-view --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown --lib \
  --bin wonderland-web-shell --bin wonderland-facade-worker --locked -- -D warnings
node --test tools/native-browser/*.test.mjs apps/web-shell/tests/gpu/*.test.mjs \
  apps/web-shell/scripts/native-*.test.mjs crates/audio-runtime/browser/*.test.mjs
```

The complete Native browser workflow builds and verifies the actual application.
Independent re-review of the final source is still required. Prior Codex review
requests reported an account review-usage limit; no billing/settings change is
part of this work. No merge, deployment, full C/E completion, original-content
parity or physical-device/multiplayer qualification is claimed by this fix.
