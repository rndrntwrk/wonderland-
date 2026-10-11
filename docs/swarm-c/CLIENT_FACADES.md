# Loaded-world FSOf exports — Swarm C W07.4 continuation

Base: PR32, `544ceb2b5e0faf99f723ad84d3ac1b91eab8dd36`.
This is actual replacement code for the previously lost facade continuation,
not a recovery claim for that missing branch. It preserves PR28 masks, PR23 PNG
capture and PR32 source-bound room lighting. It is not complete Swarm C parity.

## User and worker paths

The existing source-lot inspector now offers **Build facade**, **Cancel export**,
**Save FSOf**, **Facade details** and **Discard facade**. The download contains a
real version-1 gzip/RGBA FSOf with floor/wall textures and indexed facade geometry.
The JSON receipt contains its actual SHA-256, byte count, complete source hash,
lossless revisions, source provenance, selected resolution and bounded diagnostics.
It is a disposable visual asset, never a game save or authority credential.

The native worker uses the same job:

```sh
cargo run -p wonderland-world-view --example facade_export -- \
  normalized-world.json NEW_OUTPUT_DIRECTORY
```

It writes `facade.fsof` then `metadata.json`, refuses any existing directory,
opens files exclusively, and removes only files it actually created on failure.
Metadata is the final publication marker. A consumer must require both files and
verify the digest; a crash can leave an incomplete directory. The worker neither
changes database state nor guarantees durable world-save publication. Use a
private worker-owned parent directory, not a directory writable by adversaries.

## Source and rendering

The existing published C `render-core::derivatives` files and tests are restored
unchanged from PR12 `bf3d6432494ab436d86708a283be90d1a74267c1`. They provide
source room connectivity, exterior wall lines, source-defined atlas cameras,
insets/bleed and FSOf topology/encoding. The client uses the already locked
`flate2 1.1.10`; the lock delta adds one dependency edge, not a registry upgrade.

The new `world-view::WorldFacadeJob` takes an immutable, validated
`Arc<WorldDocument>`. It takes source altitude samples once and preserves wall,
fence, diagonal and indoor information. Source room tables are not part of this
client document, so the existing bounded presentation-topology reconstruction is
used. It is not authoritative room, collision or placement state.

The job uses the actual ordered client scene for each facade atlas region,
including normal/portal mask passes and validated per-pixel room lightmaps.
Invisible stencil passes retain their depth/stencil effects without writing color
or selection. The atlas records only the supplied lighting state. No invented
night lighting, replacement object, missing texture, or gameplay animation is
introduced to make an export look complete. Existing missing-resource diagnostics
remain in the receipt. Nearest-clamped object materials retain the current client
limitations; original GPU/filtering parity is not claimed.

Tile-less roofs now retain their declared source level when selecting a light
atlas cell. The previous ground-floor fallback was reproduced and corrected.
The atlas facade is independent of the current camera and selection overlay.
It does not call `prepare_gpu` on the live owner, advance frame generations, emit
VM events, or invalidate the existing live pick ticket.

## Bounds and lifecycle

The default floor density is four pixels per tile; callers may request 1–8.
Lots are capped at 128 tiles per side and must also pass the derived atlas limits.
Source serialization is streamed into SHA-256 with a 32 MiB input bound, rather
than allocating a second serialized copy. Provenance is bounded before work.
Prepared expanded scene limits are 16,384 parts, 250,000 vertices, 750,000 indices
and 64 MiB of buffers. Each mesh is limited to 8,192 triangles; a conservative
projected raster-work estimate limits one command to 2 million units and the job
to 100 million units, including a clipping-fan factor. These are admission bounds,
not measured frame-time or supported-content capacity claims.

FSOf limits are 16 MiB file/decoded bytes, 100,000 geometry vertices, 300,000
indices, dimensions at most 2,048 and at most 2 million texture pixels. The final
metadata limit is 64 KiB, with at most 64 bounded diagnostic records. Over-budget
inputs fail explicitly; no silently cropped or partially valid FSOf is returned.

`step(1..=128)` charges skipped commands too and yields between bounded batches.
The browser runs four commands per step and yields through the event loop.
Validation, topology/scene/light preparation and final encoding remain synchronous
bounded phases; this is **not** a Web Worker or measured no-stall mobile claim.
The native command is the preferred integration for queued server derivatives.

A source replacement, explicit cancel or unmount invalidates pending publication
and releases owned URLs. A cancelled or completed job is terminal. Camera changes
and GPU loss do not invalidate a CPU-only source snapshot or its ready facade.
Invalid imports preserve the admitted world and ready export. Discard and close
revoke both the FSOf and receipt object URLs. A final generation/source check
prevents a stale completion from installing download links.

## Verification

Local affected tests, strict affected native and WASM Clippy, Node tests, source
fixture generation, formatting and script syntax are run before publication.
Regression evidence includes a pre-implementation failure, writer-collision
cleanup, upper-roof atlas selection, cancellation, cadence independence, source
hash changes, masked facade visibility and preservation of live GPU tickets.
The normal/portal fixture includes an exterior wall view; a vertical mask viewed
only from above is not evidence that its portal body should be visible.

The read-only hosted workflows must additionally build the actual release
application, click its export/cancel/discard controls, decode exported FSOf bytes
independently in Node, check SHA-256 receipts, compare lit/shadowed atlas pixels,
reject malformed imports without losing the ready export, exercise genuine
context loss and revoke downloads on close. Before those reports are read, their
results are pending, not inferred from WASM Clippy or local tests.

Author self-review is not independent approval. No merge or deployment is
included. Full2D/hybrid composition, complete world-avatar/audio hosts, broader
cameras/environment, source thumbnails, live providers and original-VM
continuation remain separate C work. Physical devices, authorized content cohorts
and live multiplayer remain separate acceptance. E's start is not gated on these
presentation leaves; see `docs/swarm-e/C_HANDOFF.md`.
