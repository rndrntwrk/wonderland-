# Source-backed format authoring

This document describes the format additions in the remaining Swarm B work. It
extends the preceding [sprite authoring](sprite-authoring.md),
[indexed IFF](indexed-iff.md), [cooker](cooker.md), and
[cooked runtime](cooked-runtime.md) contracts. The native codec, content and cooker
gates below pass. The [combined verification record](REMAINING-VERIFICATION.md)
tracks the final integrated browser gate and independent review. This document
is not a declaration that all W01 acceptance or the complete application is finished.

The original-source revision is
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. The Rust continuation starts at
`3d6b04e0a600cb5f947145123cddf6cb2c75c5d7`. Original source and assets are read
without modification. Libraries expose CPU data and explicit dependency keys;
they allocate no graphics objects and perform no file or network lookup.

## Standalone avatar formats

`wonderland_legacy_formats::vitaboy` now exposes matching encoders for the
existing standalone readers:

| Format | Supported stored version | Public writer | Original source under `TSOClient/tso.vitaboy.model/` |
|---|---:|---|---|
| Mesh | 2 | `encode_mesh` | `Mesh.cs` |
| Animation | 2 | `encode_animation` | `Animation.cs` |
| Skeleton | 1 | `encode_skeleton` | `Skeleton.cs` |
| Binding | 1 | `encode_binding` | `Binding.cs` |
| Appearance | 1 | `encode_appearance` | `Appearance.cs` |
| Outfit | 1 | `encode_outfit` | `Outfit.cs` |

Each writer accepts the typed value and `&Limits`, returning `Result<Vec<u8>>`.
Integers and selectors follow the original big-endian standalone layout;
floating-point values follow the original little-endian layout. `F32Bits`
retains the exact finite binary32 representation. The `FreeSo` coordinate policy
inverts vector X and quaternion Y/Z/W by toggling the sign bit. Applying the same
policy during writing reverses that conversion, including signed zero.

Writers retain field ordering, face and vertex ordering, property pairs, event
ordering, flags, reserved fields and raw blend weights. They check derived
metadata against the actual source fields: frame counts must match motion
headers; hierarchy indices must agree with parent names and child membership;
binding ranges and repeated mesh counts must agree with the arrays. An absent
property flag cannot silently discard an edited property list. Unsupported
versions and trailing payloads remain explicit errors.

Original bytes remain the lexical no-op authority for source formats whose
encoding has multiple valid representations. A semantic encoder does not claim
to reproduce an original compressor's choices or text formatting.

### Outfit and appearance references

Additional bounded readers and writers expose:

| Type | Stored fields retained | Source |
|---|---|---|
| `PurchasableOutfit` | Version, gender, asset-size declaration, ignored four-byte prefix and 64-bit outfit ID | `PurchasableOutfit.cs` |
| `Collection` | Signed ordered index and file-key pairs, including repeated or negative indices | `Collection.cs` |
| `HandGroup` | Version and all 18 appearance keys, in source skin/hand/gesture order | `HandGroup.cs` |

The original purchase and hand-group readers do not branch on their version
declarations. The codecs preserve these declarations without inventing a
version-specific interpretation. Exact trailing-byte rejection prevents an
unrecognized extension from being presented as decoded data.

## Legacy avatar headers and samples

The TS1 formats are defined by `BCF.cs`, `Skeleton.cs`, `Appearance.cs`,
`Animation.cs`, `Mesh.cs`, `CFP.cs`, and
`TSOClient/tso.files/Utils/BCFReadProxy.cs` at the source pin.

| Resource | Representation | API and context |
|---|---|---|
| BCF | Little-endian binary, one-byte ASCII string lengths | `decode_bcf` / `encode_bcf`, `LegacyEncoding::Binary` |
| CMX | Line-based text with a `version` header | The same BCF model with `LegacyEncoding::Text` |
| BMF | Little-endian mesh with skin and texture names | `decode_bmf` / `encode_bmf`, `LegacyEncoding::Binary` |
| SKN | Line-based mesh; blend index precedes raw weight | The same `LegacyMesh` model with `LegacyEncoding::Text` |
| CFP | Component streams with literal, delta and repeat commands | `decode_cfp` requires translation and rotation counts from the exact BCF animation header |

`Bcf` contains skeletons, appearances and animation headers. It does not pretend
that those animation headers contain their motion samples. `BcfAnimation`
retains `xskill_name`, the original translation and rotation counts, moving
integer, motion flags and event lists. `BcfAnimation::enrich(cfp_bytes, limits)`
validates the supplied CFP against those counts and returns an `Animation`.
There is no implicit path search in this API.

`TSOClient/tso.content/TS1/TS1BCFProvider.cs` resolves a companion by appending
`.cfp` to `XSkillName` and applying invariant lowercase. The source mesh provider
`TS1BMFProvider.cs` removes `.mesh`, applies invariant lowercase, then tries BMF
before SKN. A caller that resolves names must retain the selected source and
hash; these source lookup rules do not grant a license to choose an arbitrary
similarly named resource. Missing companions remain missing dependencies.

The BCF skeleton reader skips a record with an empty bone name after reading its
parent name. `BcfSkeleton::skipped_bones` retains the skipped record and insertion
position. BCF appearance bindings retain their inline bone, mesh-name, censor
flags and reserved integer. BCF motion property lists use the original single
pair-list layout, which differs from standalone Vitaboy properties. Time
properties retain source order, including decreasing timestamps and repeated
keys.

CFP decoding resets the previous sample to zero separately for translation
X/Y/Z and rotation X/Y/Z/W. A repeat command emits its stored count **plus one**,
matching the output after the source switch statement. A repeat that crosses a
component boundary is rejected. Delta byte 253 has no source table entry and is
rejected. The encoder uses exact literals and repeats; it does not introduce
lossy delta quantization. This also avoids the original compressor's invalid
256-entry search over its 253-entry delta table.

### Text conversion policy

Text output is canonical rather than byte preserving. The original line reader
allows numeric groups separated by literal spaces and reads strings from their
own lines. CMX version declarations and UTF-8 text names are retained in the
semantic model. Unsupported encodings are reported instead of decoding with a
platform-specific code page.

The original framework's `Single.Parse` normalizes a textual negative zero to
positive zero before coordinate conversion. This has been checked with an
independent C# numeric probe. Binary formats retain signed zero. Consequently,
binary-to-text conversion needs an explicit normalization decision whenever a
wire float contains negative zero. The default writer rejects that loss.
`encode_bcf_text` and `encode_bmf_text` accept `LegacyTextPolicy::Exact` or
`NormalizeSignedZero` and return `LegacyTextOutput { bytes,
normalized_signed_zeros }`. The count describes wire floats before coordinate
conversion. Nine significant decimal digits preserve other finite binary32
values through the original double-intermediate parser. Literal vectors also
cover underflow, subnormals, the largest finite value, decimal rounding and the
source's permissive invariant comma grouping. This policy is tied to the
verified Mono 6.8.0.105 framework; it is not a claim that every newer CLR's
`Single.Parse` has identical semantics.

## Sprite and drawing-group authoring

`sprites::encode_dgrp` supports all decoded source versions 20000 through 20004.
The older record layouts retain their type word and signed integer offsets.
The layouts only write the object-offset components their version actually
stores. An edited absent component is rejected, including a negative-zero bit
pattern that would otherwise be lost.

`sprites::encode_spr` writes source-readable SPR# command streams with an
explicit palette. Version 1000 supports either byte order. Version 1001 uses
the source-supported little-endian path and retains its declared frame count.
The writer preserves palette indices for opaque pixels and validates the
format's representable transparency and metadata. It uses bounded literal,
repeat and transparent commands and checks the one-byte row-size limit. Raw
input bytes are still necessary for an exact no-op of a noncanonical original
command stream. Source anchors are the SPR# and DGRP chunk readers in
`TSOClient/tso.files/Formats/IFF/Chunks/`.

The TTAB reader and writer now follow the original version branches beyond
version 10. Standard tables above version 10 have no compression byte and store
the additional flags; TSBO tables retain a compression byte and omit those
flags. Versions 9 and 10 retain their existing compression handling. The
original refuses active interaction tables at version 3 and below, which remain
unsupported. STR formats 0, -1, -2, -3 and -4 were already supported by the
preceding implementation.

## Reconstructed geometry and neighborhood houses

`wonderland_legacy_formats::reconstruction` implements the CPU layouts in
`TSOClient/tso.files/RC/{DGRP3DMesh,DGRP3DGeometry,DGRP3DVert,NBHm}.cs` and the
FSOM IFF wrapper in `tso.files/Formats/IFF/Chunks/FSOM.cs`.

`decode_fsom` and `encode_fsom` read and write the gzip-wrapped FSOm stream.
`decode_fsom_payload` and `encode_fsom_payload` expose its uncompressed form.
Stored versions 1, 2 and 3 retain reconstruction version, name, dynamic group
ordering, empty groups, texture references, geometry, indices and bounds.
Version 1 stores position and UV only; its absent normal remains exact positive
zero. Versions 2 and 3 store all eight vertex floats. Version 3 also stores
normal or portal depth-mask geometry. Normal generation and rendering remain
separate consumer operations.

`FsomGeometry::texture_reference()` exposes either a custom MTEX ID or a source
sprite index and rotation. The original source selects DGRP
`GetImage(1, 3, PixelDir).Sprites[PixelSPR]`. Resolving that selection requires the
matching DGRP, its SPR#/SPR2 and, where applicable, the exact PALT. Custom MTEX
references first consult the original replacement-texture provider. With a DGRP,
the source key is its IFF filename after replacing `.` with `_` and `spf` with
`iff`, followed by `_TEX_{id}.png`; only an absent replacement falls back to the
same-IFF MTEX. Without a DGRP, the key is `FSO_TEX_{id}.png`. The codec does not
guess a renderer texture or treat an unavailable sprite as a successful binding.
The importer must retain an explicit replacement choice or state that no external
replacement was supplied. Duplicate texture identities after provider resolution
are also a consumer-level validity check because the original uses a dictionary
keyed by resolved texture objects.

`FsomMesh::is_current_reconstruction()` reports the source provider's separate
reconstruction-version admission rule. Retaining an older reconstruction in an
editor is distinct from claiming that the source renderer accepts it.

Gzip handling validates optional headers, header CRC, payload CRC, length,
complete DEFLATE termination and exact input consumption. Decompression reserves
a bounded workspace and checks the advertised decoded size before allocation.
The deterministic writer emits valid stored DEFLATE blocks. A Python zlib
fixture independently exercises Huffman-coded input and back references; an
incomplete final-block fixture checks that decoded bytes alone cannot establish
a complete stream.

`decode_nbhm` and `encode_nbhm` retain house-number/position records and the model
marker. Duplicate records remain ordered; `Nbhm::house` implements the original
last-assignment lookup. Data for an unrecognized following model is rejected.

## PNG texture data

`textures::decode_png` exposes width, height, original bit depth and RGBA8
pixels. It checks chunk structure, CRCs, complete IEND, dimensions, allocation
admission and decoder limits. Palette, low-bit grayscale, tRNS and Adam7 input
use the pure Rust PNG decoder. APNG requires a separate animation contract and
is rejected. Text and color-profile expansion are not requested.

Sixteen-bit samples convert to their high eight bits and retain
`source_bit_depth = 16`, so callers can distinguish this conversion from an
exact eight-bit source. Original bytes must be retained for lossless no-op
export. The decoder retains RGB values under zero alpha. Creator's city-image
authoring decoder has its own explicit precision policy and additionally
handles the source's BMP forms.

An MTEX wrapper may contain image forms other than PNG. Those forms remain
opaque unless their actual decoder is selected and validated. Adding a PNG
decoder is not evidence that every MTEX or external image format is supported.

## Allocation and validation

Readers bound input/resource bytes and charge decoded structures before allocating
arrays, strings and graph metadata. Image and gzip paths additionally admit
compressed input, decoded output and codec workspace together. Counts, multiplication,
offsets and aggregate sample/geometry limits are checked. Writers validate and
measure before allocating the exact output, including retained input capacities
and scratch admission. These limits are conservative operation budgets, not a
claim to account for an entire process's RSS or allocator internals.

No new library code uses unsafe Rust or native graphics APIs. `flate2` uses its
Rust backend; PNG decoding is portable Rust. Codec availability, exact source
provenance and resource dependencies are distinct manifest fields.

## Verification record

The focused tests are in `crates/legacy-formats/tests/`:

| Test file | Acceptance covered |
|---|---|
| `vitaboy.rs` | Original field-order fixtures, exact standalone writer round trips, metadata and hierarchy rejection, coordinate bits and allocation admission |
| `vitaboy_references.rs` | Purchase prefix, collection record order, all hand-group appearance keys, truncation and limits |
| `source_variants.rs` | Later standard/TSBO TTAB layouts, all DGRP versions, SPR# byte orders and source command constraints |
| `reconstruction.rs` | FSOm versions 1/2/3, source vertex layout, gzip integrity and full termination, independent compressed fixture, NBHm duplicates and bounded input |
| `textures.rs` | Independent RGBA, indexed+tRNS and 16-bit PNG fixtures, truncation, CRC, trailing bytes and allocation/pixel limits |
| `legacy_vitaboy.rs` | BCF/CMX headers and properties, skipped bones, source text numeric policy, BMF/SKN weight layout and UTF-8 names, contextual CFP component/event semantics and admission |
| `source_avatar_corpus.rs` | Read-only original-asset payload comparison for every checked-in avatar and FSOm file |

On 5 October 2026 the focused final gates completed with Rust/Cargo 1.90.0:

| Gate | Result | Evidence |
|---|---|---|
| Full legacy-formats suite | 123 passed, 3 explicitly ignored, no failures | [native formats](format-evidence/native-formats.log) |
| Full content-ir suite | 14 passed, no failures or ignored tests | [native content](format-evidence/native-content.log) |
| Full asset-cooker suite | 26 passed, no failures or ignored tests | [native cooker](format-evidence/native-cooker.log) |
| Strict Clippy, all targets | Passed for all three crates with `-D warnings` | [formats](format-evidence/clippy-formats.log), [content](format-evidence/clippy-content.log), [cooker](format-evidence/clippy-cooker.log) |
| Format checks | Passed for all legacy-formats source and owned content/cooker files | [formats](format-evidence/fmt-formats.log), [content/cooker](format-evidence/fmt-content-cooker-owned.log) |
| New cooker codec/import gates | All ten new codecs validate actual bytes, preserve codec/provenance through real file imports and rebuild deterministically; reject semantic/critical misclassification | [codec/import cases](format-evidence/new-cooker-codecs.log) |
| Original avatar/FSOm census | 455 exact payload round trips: 50 animations, 120 appearances, 120 bindings, 121 meshes and 44 FSOm files | [per-file path, hash and byte evidence](format-evidence/source-avatar-corpus.log) |
| Original text-reader probe | Compiled unchanged `BCFReadProxy.cs` under Mono 6.8.0.105; zero/decimal/grouping and failure cases recorded | [probe source](format-evidence/legacy-text-source-probe.cs), [output](format-evidence/legacy-text-source-probe.log) |

The initial tests observed missing encoder/codec APIs before implementation. The
later text-number and UTF-8 SKN regressions failed on the incorrect behavior before
their fixes. The original asset census contains no original asset payload copies.
FSOm comparisons use every uncompressed source payload byte; they do not require
identical gzip headers or compressor decisions.

Three preceding tests are opt-in and remained ignored in this ordinary gate:

| Test | Explicit reason |
|---|---|
| `original_indexed_corpus_has_exact_passthrough_and_bounded_edit_dispositions` | Its existing declaration requires the original source asset checkout and explicit `--ignored` execution. Previous indexed-IFF evidence is recorded separately. |
| `source_derived_v1_input_supports_all_five_structural_edits` | Requires an independently prepared version-1 IFF path in `WONDERLAND_IFF_V1_SOURCE`; no such new input was supplied by this codec task. |
| `repository_semantic_corpus_probe` | Its existing declaration requires the source checkout corpus and explicit execution; the global source census is owned by the coordinator. |

The coordinator's combined gate rebuilds the optimized formats → Creator → Leptos
WASM artifact from the final source tree, checks strict WASM Clippy and executes
the actual browser workflows. See the [combined verification record](REMAINING-VERIFICATION.md)
for the exact artifact, source fingerprint and independent review evidence.

## Reproducing the owned gates

From the repository root, set an isolated target path with enough free disk:

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_TARGET_DIR=/absolute/path/to/format-target

cargo test --offline --manifest-path crates/legacy-formats/Cargo.toml
cargo clippy --offline --manifest-path crates/legacy-formats/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path crates/legacy-formats/Cargo.toml -- --check
cargo test --offline --manifest-path crates/content-ir/Cargo.toml
cargo clippy --offline --manifest-path crates/content-ir/Cargo.toml --all-targets -- -D warnings
cargo test --offline --manifest-path tools/asset-cooker/Cargo.toml
cargo clippy --offline --manifest-path tools/asset-cooker/Cargo.toml --all-targets -- -D warnings
```

The per-file census additionally uses `--test source_avatar_corpus -- --nocapture`.
The text probe compiles the original
`TSOClient/tso.files/Utils/BCFReadProxy.cs` together with
`docs/swarm-b/format-evidence/legacy-text-source-probe.cs` using `mcs`; run the
result with `mono`. The probe's CFP delta calculations are independent C# numeric
vectors from the source formula, not execution of the complete original CFP
class. Binary-format correctness is also checked by literal field-order vectors.

## Exact remaining boundaries

- Actual standalone files in the checked-in source corpus were compared. No
  original TS1 BCF/CMX/BMF/SKN/CFP corpus is present there; those readers/writers
  have source-backed literal and numeric vectors, not a claim of exhaustive
  original TS1 asset parity.
- Legacy text accepts UTF-8 with optional UTF-8 BOM. Other BOM encodings that a
  platform `StreamReader` may recognize require an explicit conversion first.
  Binary Pascal names are ASCII; unrepresentable name conversion is rejected.
- Unsupported stored versions and unknown trailing layouts remain explicit
  errors. SPR# version 1001 big-endian remains unsupported because of the
  original mixed-offset behavior. Raw no-op preservation does not count as
  semantic support for an unsupported layout.
- A CFP stream has no self-describing sample counts. Generic independent CFP
  pack import cannot claim a complete animation; a caller must bind its exact
  BCF header and companion, then emit the existing semantic animation codec.
  BCF/CMX containers are noncritical visual metadata in the manifest.
- Per-file imports classify and validate the ten new codec cases. Full global
  Vitaboy file-ID/name provider discovery and companion selection still require
  explicit source mappings and dependency declarations. A validated declaration
  does not prove that an omitted dependency was discovered.
- PNG is the new shared decoded texture type. Non-PNG MTEX data remains opaque
  in the shared cooker; Creator city BMP support does not imply JPEG/BMP support
  in every texture consumer. External replacement-texture choice remains an
  explicit importer/provider contract.
- FSOm geometry reconstruction, generated normals, GPU resources and render
  acceptance are separate view-provider work. NBHm retains its model marker
  but rejects unrecognized following model bytes. Audio rendering and HIT
  execution remain separate from the existing B-owned audio metadata codecs.
- The combined verification record, rather than the earlier lane-only results,
  determines final integrated browser and independent-review status.
