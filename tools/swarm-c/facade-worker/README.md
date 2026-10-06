# Source-world thumbnail and facade worker

This Rust 1.75 package produces real RGBA PNG thumbnails and day/night facade
atlases from validated, normalized `RenderFrame`, mesh, material, and camera
inputs. Its source-world entry additionally prepares original Blueprint room,
altitude, camera, crop and facade geometry, then emits the original `FSOf` v1
container. It supplies both normalized and source-world fixtures and independent
PNG/FSOf/repeat verifiers. It requires no graphics device, network service, or
licensed game assets.

The associated scheduler lives in
[`render-core::derivatives`](../../../crates/render-core/src/derivatives.rs).
It supports queued and in-flight jobs, exact attempt cancellation, bounded CPU
residency, LRU eviction, and owned leases. A generic GPU cache carries those leases
through actual backend texture/buffer ownership and stale completion disposal.
The command-line program is synchronous; hosts may execute bounded worker jobs
or use incremental tasks to yield between draws. Neither component runs
simulation ticks or changes authoritative lights.

## Build and run

From the repository root:

```sh
cargo test --locked --manifest-path tools/swarm-c/facade-worker/Cargo.toml
cargo build --locked --manifest-path tools/swarm-c/facade-worker/Cargo.toml
```

If `CARGO_TARGET_DIR` is set, the executable is
`$CARGO_TARGET_DIR/debug/wonderland-facade-worker`. Otherwise it is
`tools/swarm-c/facade-worker/target/debug/wonderland-facade-worker`.

```sh
tools/swarm-c/facade-worker/target/debug/wonderland-facade-worker source-fixture /tmp/source-facade-example
tools/swarm-c/facade-worker/target/debug/wonderland-facade-worker source-render /tmp/source-facade-example/source-request.wlcsrc /tmp/source-facade-repeat
python3 tools/swarm-c/facade-worker/verify_source.py tools/swarm-c/facade-worker/target/debug/wonderland-facade-worker /tmp/source-facade-verification

tools/swarm-c/facade-worker/target/debug/wonderland-facade-worker fixture /tmp/facade-example
tools/swarm-c/facade-worker/target/debug/wonderland-facade-worker render /tmp/facade-example/request.wlcdr /tmp/facade-repeat
python3 tools/swarm-c/facade-worker/verify.py tools/swarm-c/facade-worker/target/debug/wonderland-facade-worker /tmp/facade-verification
```

Each output directory must be new. The worker returns an error instead of
replacing an existing artifact set. `metadata.json` is written last; failures
remove the new partial directory. The normalized fixture emits:

| File | Meaning | Fixture size |
| --- | --- | --- |
| `thumbnail-day.png` | Explicit-camera daytime mesh thumbnail | 256 × 256 |
| `thumbnail-night.png` | Same camera, independent night material inputs | 256 × 256 |
| `floor-day.png` | Three-column, two-row daytime floor/overlay atlas | 384 × 256 |
| `floor-night.png` | Independent night floor/overlay atlas | 384 × 256 |
| `wall-day.png` | First-fit daytime wall strips, including edge bleed | 512 × 72 |
| `wall-night.png` | Independent night wall strips | 512 × 72 |
| `request.wlcdr` | Complete normalized input used to reproduce the output | Versioned binary envelope |
| `metadata.json` | Frame, effective input, artifact, PNG, RGBA, lighting and region metadata | UTF-8 JSON |

The fixture contains two floors, eight exterior wall segments, a roof, a
generation-bound object, and procedural colors/textures. It is renderer and
lifetime evidence; it supplies no independent provider or licensed-content proof.

The **source-world fixture** uses the same declared materials and building, but
feeds actual per-tile ground and walls through source topology, altitude, camera
and geometry preparation. Its thumbnail is the original TSO **576 × 576**. It
replaces `request.wlcdr` with `source-request.wlcsrc` and adds `facade.fsof`, for
nine output files. The original container contains 112 floor/roof/overlay vertices,
456 floor indices, 32 wall vertices, 48 wall indices and the exact produced atlas
pixels. The 64-tile facade footprint is centered in the 77-tile source lot at
6.5..70.5, matching the source generator.

## Supply source-world content

`SourceWorkerRequest` contains the actual normalized `DerivativeInput`, a
`source::SourceWorld`, `source::SourceFacadeOptions`, and optional
`night_light_color`. SourceWorld supplies lot width/height/stories, raw Blueprint
altitude, base altitude, and floor-major tiles containing patterns, west/north/
south/east wall/fence flags, optional diagonal direction and optional known
indoor classification. Exact `SourceRooms` can additionally supply the original
indexed room/base table, outside flags, wall/fence lines in sixteenths, and packed
room-map halves. Optional FineArea preserves the source mask.

The adapter extracts source exterior rooms, computes midpoint altitude, detects
used floors, prepares original TSO/TS1 thumbnail cameras, and builds source
ground/floor/overlay/roof/wall geometry. If exact room tables are absent it uses
bounded half-tile connectivity; it never treats floor paint as proof of an
interior room. Prepared draws still provide the actual geometry/materials. Their
optional source tile placements support exact FineArea cropping for floor batches.

Call `encode_source_request(&request)` to create a `.wlcsrc` file. Its eight-byte
magic is `WLCFSR01`, followed by fixed-width little-endian bincode 1.3. The 64 MiB
file and decoded-input bound and trailing-data rejection apply before preparation.
`source-render` consumes this protocol and emits PNGs plus `facade.fsof` in
lossless RGBA mode with gzip. This is the original FSOf container field layout;
it is not a renamed normalized request.

When `night_light_color` is `None`, the prepared request is explicitly day-only:
three day PNGs, one day metadata phase, and no night textures/flag in FSOf. No
night state is synthesized from daytime data. When present, independent prepared
night materials and the supplied source outside color are retained. The metadata
records the FSOf SHA-256 in addition to all image and source identities.

Core `fsof::Fsof::decode` also reads original raw/gzip RGBA or BC3/DXT5 containers.
Count, texture, total decoded-byte, vertex and index bounds apply before
allocation; gzip checksums/termination and trailing bytes are validated. BC3
decoding handles both alpha palettes and partial blocks. Existing encoded BC3
containers can be preserved and written; new CPU renders use RGBA, without
claiming a new BC3 encoder. `render-3d::city::facade::load_source_facade` admits the
two meshes/textures into the source city transform without double unit scaling.

## Supply normalized content

`DerivativeInput` is a serializable Rust structure exported by this package and
`wonderland_render_core::derivatives`. Construct it with:

- An immutable `RenderFrame`, including lot, epoch, tick, architecture revision,
  content hash, entity generations, transforms and asset identities.
- Ordered `DerivativeDraw` meshes, explicit material indices and draw layers.
  Architecture uses `owner: None`. An owned draw must name a current frame entity
  and match that entity's asset. Its model matrix is relative to the entity.
- Independent `day` and `night` material tint/texture inputs. Colors are finite
  normalized straight RGBA; sampling is nearest and clamped. The caller declares
  alpha cutoff and depth writes. Both phases use source `LessEqual` depth tests.
- Two `LightingPass` records. `time_of_day` and provenance identify the prepared
  state; they do not compute room lights or alter simulation time. Each phase's
  uniform RGB multiplier is applied to its material and vertex colors.
- A `ThumbnailRequest` with a finite invertible clip matrix, or a
  `FacadeRequest` with explicit lot dimensions, floor count, exterior wall
  classification, midpoint terrain altitude and optional thumbnail camera.

Meshes, model transforms, entity transforms and camera inputs share the source
graphics basis: three graphics units per tile, vertical Y, lot ground X/Z. Wall
endpoints retain the source's 1/16-tile coordinates. Supplying an already
transformed world mesh under a transformed owner would apply that transform twice;
use owner-relative mesh/model coordinates or an architecture draw as appropriate.

Call `encode_request(&input)` to obtain a `.wlcdr` file. The envelope is the
eight-byte magic `WLCFDR01`, then bincode 1.3 fixed-width little-endian encoding of
`DerivativeInput`; trailing data is rejected. The file is limited to 64 MiB plus
the magic, and a changed file length fails the bounded read. This is a normalized
worker protocol, not a decoder for `.fsof`, FSOM, IFF, screenshots, or lot saves.
Both normalized CLI entry paths pass through the same bounded decoder before preparing the
job, so vector growth in a fixture constructor does not change the recorded
reservation or repeat metadata. Core queue accounting still measures each actual
prepared input's owned capacity; callers may legitimately have different spare
capacity for otherwise identical effective content.

All source provenance, actual geometry/texture bytes, frame/entity information,
draw order, lighting, cameras and layout parameters participate in the effective
input SHA-256. The derivation key additionally includes the algorithm version and
content identity. A declared provenance hash cannot hide changed effective bytes.

Output compositing uses the core reference's normalized straight-RGBA
Porter–Duff source-over. The original MonoGame `NonPremultiplied` render-target
blend uses SourceAlpha / InverseSourceAlpha factors for both RGB and alpha, so
partially transparent legacy target bytes can differ from these normalized PNGs.
This distinction and the consumer's separate premultiplication/filtering rules
are explicit source-parity limits.

## Schedule and own jobs

```rust
use wonderland_render_core::{RenderLimits, RenderFrame};
use wonderland_render_core::derivatives::*;

fn schedule(input: DerivativeInput) -> Result<(), DerivativeError> {
    let queue = DerivativeQueue::new(QueueLimits::default(), RenderLimits::default())?;
    let stamp = input.frame.stamp;
    queue.reset(stamp.lot_id, stamp.epoch)?;
    queue.admit_frame(input.frame.clone())?;
    let request = PreparedDerivative::new(input, DerivativeRenderLimits::default())?;
    let ticket = queue.submit(request)?;

    // A host may move this owned job to a bounded worker thread.
    if let Some(job) = queue.start_next()? {
        job.execute()?;
    }
    if let Some(lease) = queue.acquire(&ticket.key())? {
        let images = lease.artifact().images();
        assert!(!images.is_empty());
        // Dropping the lease releases precisely this installed generation.
    }
    queue.evict(&ticket.key())?;
    Ok(())
}
```

Accepted frame changes create a new derivative lifetime. A return from content A
to B and back to A cannot revive old work. Repeated identical frame admission is
idempotent, and resetting to the same lot/epoch still invalidates every previous
attempt. Opaque ticket identity also prevents replay into a different queue.

Same-key submission supersedes one exact attempt. Failed admission preserves the
previous queued attempt. Cancellation and eviction remove queued payloads
immediately; running payloads keep their count/byte reservations until their owned
job completes or drops. An older failure/completion cannot remove a newer request.
LRU eviction also cancels any pending refresh for the evicted key.

An acquired `DerivativeLease` pins an exact installed artifact. Reset or
replacement hides that artifact from lookup but retains its bytes as a retired
entry until the last lease drops. A pinned entry cannot be explicitly evicted;
an over-budget completion fails without evicting unrelated entries. This is CPU
ownership. For GPU upload, `upload::DerivativeGpuCache<T>::begin_upload` takes
this CPU lease and a conservative actual allocation (including alignment and
staging). The caller performs the backend upload and calls `job.complete(T)`.
Source digest, key, attempt, frame lifetime and device generation are rechecked
before installation. Stale results drop the real resource immediately. Pending
uploads and retained old GPU leases stay charged after reset until released.

For incremental browser execution, replace `job.execute()` with
`let mut task = job.into_task()?` and repeatedly call `task.step(128)?`, yielding
to the event loop on `None`. Completion returns Installed or Stale; dropping a
task releases staged pixels before its pending reservation. A single draw is
still bounded by the prepared request's conservative work limit.

## Limits and evidence

Default rendering limits allow 4,096 draws/materials, 512 views, 128 MiB of
prepared input, 128 MiB of output, and 256 million conservative work units, in
addition to `RenderLimits`. The work model counts vertex traversal, clipping and
up to seven raster triangles per clipped source triangle using conservative
projected rectangles, for both phases. Eye-plane crossings use a full-surface
bound. It is
a deterministic admission bound, not a frame-time measurement.

Default queue limits are 32 queued jobs, two unsettled in-flight jobs, 256 MiB of
pending reservations, 100 resident entries, and 64 MiB of resident CPU data.
Reservations include input geometry/textures, planned commands, artifact metadata,
output pixels, reference surfaces, shaded/transformed mesh scratch and a small
clipping cushion. Budgets describe owned payload memory; allocator bookkeeping,
worker thread stacks and caller-created copies lie outside that accounting.

`verify.py` renders the fixture twice, compares all eight files byte-for-byte,
independently decodes every PNG through Python's standard-library zlib, checks
chunk CRCs, validates dimensions/RGBA hashes, and proves that each of the three
day/night image pairs differs. It also checks malformed magic, truncation, trailing
data, impossible entity counts, oversized files, and existing-directory retention.

`verify_source.py` additionally executes the actual source-world path, independently
decodes the original FSOf field order, compares container texture bytes with the
PNG decoder, checks source topology/ground/overlay/roof units and 576-pixel
thumbnails, compares nine replay files, and checks a day-only source request.
Small current reports are saved in `fixtures/source-verification.json` and
`fixtures/synthetic-verification.json`; generated images and build artifacts are
reproducible and need not be committed.

Source equations, intentional differences and the remaining rendering algorithms
are recorded in
[`derivatives-source-notes.md`](../../../docs/swarm-c/derivatives-source-notes.md).
