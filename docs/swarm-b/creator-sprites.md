# Creator: guarded SPR2 editing packages

Creator exports an existing SPR2 resource and its required palettes into one
editable JSON file, changes exact indexed pixels or palette colors, and imports
the result into a guarded IFF transaction. The workflow supports SPR2 versions
1000/1001 and PALT versions 0/1 through the existing CPU format codecs. Palette
indices, straight alpha and depth remain separate byte planes.

The package binds edits to the exact source IFF and selected payloads. Import
replaces the complete candidate only after encoding, resource-map rebuilding
and reopening succeed. An unchanged package returns the exact original IFF,
including its SPR2 command choices, padding and unused tails.

## CLI workflow

Build `tools/creator/Cargo.toml` with Rust 1.90.0 as described in
[Creator](creator.md). All paths below are relative to the selected private
workspace. This example assumes `object.iff` contains `SPR2/8`, its first frame
has depth, and its required `PALT/7` contains entry 2. Use `list` to choose the
resource and inspect the exported JSON for its actual dimensions, palette IDs
and channel flags.

```sh
creator --root ./private-work list object.iff
creator --root ./private-work sprite-export object.iff 8 sprite.json
creator --root ./private-work sprite-pixel sprite.json pixels.json 0 0 0 2 123 19
creator --root ./private-work sprite-palette pixels.json colors.json 7 2 40 50 60
creator --root ./private-work sprite-import object.iff candidate.iff colors.json
creator --root ./private-work sprite-export candidate.iff 8 verified.json
```

This changes frame 0's top-left sample to palette index 2, alpha 123 and depth
19, then changes palette entry 2 to RGB `(40, 50, 60)`. Both changes publish
together. Coordinates and frame/color indices are zero-based. CLI integers
accept decimal or a `0x` prefix; byte values must fit 0 through 255. The final
`sprite-pixel` argument must be a byte for frames with depth and `none` for
frames without it. Fully transparent pixels require the frame's transparent
index reduced to its low byte, and depth 255 when depth is present.

| Command | Purpose |
| --- | --- |
| `sprite-export INPUT SPR2_ID PACKAGE_JSON` | Decode a sprite with exactly its real required palettes and export guarded JSON. |
| `sprite-import INPUT OUTPUT PACKAGE_JSON` | Validate the package against this source, encode changed payloads, rebuild the original IFF map and publish atomically. |
| `sprite-pixel PACKAGE_JSON OUTPUT_JSON FRAME X Y INDEX ALPHA DEPTH` | Change one exact sample after checking coordinates and channel consistency. |
| `sprite-palette PACKAGE_JSON OUTPUT_JSON PALETTE_ID INDEX R G B` | Change one required palette RGB triple without changing identity or color count. |
| `sprite-alpha-mode PACKAGE_JSON OUTPUT_JSON exact\|source` | Choose exact alpha or explicitly enable source alpha quantization. |

Package editing commands may use the same input and output path. The original
package hash is rechecked before atomic replacement. `sprite-import` rechecks
the package and source IFF before publication. Workspace rules require relative
paths, ordinary files and existing parent directories; parent traversal and
symlinks are refused. Commands return 0 on success and 2 on error. Rejection
leaves an existing destination untouched.

After publishing a changed IFF, export a fresh package from that IFF for the
next edit. Replaying an older package against changed source is a conflict.
An exact no-op retains its hash and remains replayable.

## Alpha and explicit quantization

PALT stores RGB only; alpha belongs to each sprite pixel. Package alpha is
straight, unpremultiplied, and defaults to `"exact"`. Partial alpha must be
representable by the source's five-bit stored value and requires depth because
the corresponding command stores both. Alpha 123 is exact; alpha 124 is refused.

```sh
creator --root ./private-work sprite-alpha-mode sprite.json source-mode.json source
creator --root ./private-work sprite-pixel source-mode.json quantized.json 0 0 0 2 124 19
creator --root ./private-work sprite-import object.iff candidate.iff quantized.json
```

Source mode stores `ceil(alpha * 31 / 255)` and decodes that value as
`floor(stored * 255 / 31)`, using bounded integer arithmetic. Alpha 124 therefore
becomes 131. The report counts every authored alpha byte changed by conversion.
The package keeps its authored value until import; a subsequent export exposes
the stored, decoded value. Switching a package back to exact mode fails without
changing it if any alpha needs rounding.

No-op detection compares values after this conversion. If the source already
contains alpha 131, source mode can receive alpha 124 with all other values
unchanged, report one quantized pixel, and preserve the exact original bytes.

## Schema 1

Export writes compact JSON with a trailing newline. Edited whitespace is
accepted. Every field is required, including `depth_hex`; unknown/duplicate
fields, positional object arrays, trailing JSON, wrong scalar types and invalid
hex are errors. JSON integers use decimal notation and must fit their declared
widths. Hex is case-insensitive on input and lowercase on export.

The root object has exactly these fields:

| Field | Meaning |
| --- | --- |
| `schema_version` | Integer 1. |
| `source_sha256` | 64 hex characters guarding the complete input IFF. |
| `alpha_mode` | String `"exact"` or `"source"`. |
| `sprite` | Selected sprite object. |
| `palettes` | Array containing exactly its effective palette dependencies; export sorts by ID. |

The sprite object contains `id` (unsigned 16-bit resource ID),
`resource_sha256` (64-character payload hash), `format_version` (1000 or 1001),
`default_palette_id` (unsigned 32-bit source value), and `frames` (array).
Source identity and format fields are immutable during import.

Each frame contains these fields:

| Field | Type and rule |
| --- | --- |
| `index` | Zero-based integer; exact frame count/order and indices 0 through count minus one are retained. |
| `width`, `height` | Editable unsigned 16-bit dimensions with matching planes. Zero dimensions remain subject to frame-count bounds. |
| `flags` | Editable integer 1, 3, 5 or 7: mask 1 requires color, mask 2 indicates depth, mask 4 preserves the source alpha-channel flag. |
| `raw_palette_id` | Immutable unsigned 16-bit stored frame palette ID. |
| `palette_id` | Immutable unsigned 16-bit effective palette ID, checked against source fallback rules. |
| `transparent_index` | Immutable unsigned 16-bit palette entry, which must exist. |
| `position` | Editable `[x, y]` array of signed 16-bit integers. |
| `indices_hex` | Editable row-major index plane, two hex characters per pixel. |
| `alpha_hex` | Editable row-major straight-alpha plane, two hex characters per pixel. |
| `depth_hex` | Editable row-major depth plane of matching size when flag mask 2 is set; otherwise required JSON `null`. |

A 2-by-1 frame can use `indices_hex: "0102"`, `alpha_hex: "ff7b"` and
`depth_hex: "0013"`: samples `(index 1, alpha 255, depth 0)` and
`(index 2, alpha 123, depth 19)` in row order. Hex strings contain no separators
or internal whitespace. To resize or change channels, edit the JSON fields
together and provide complete matching planes. The pixel command changes one
existing sample at a time.

The source decodes partial-alpha commands even when flag 4 is absent. The
workflow retains that behavior; flag 4 alone does not supply depth or make
partial alpha representable. Unknown flags and colorless frames are refused.

Version 1000 resolves every frame through the low 16 bits of the default
palette ID. Version 1001 uses that default for raw IDs 0 and `0xA3A3`, otherwise
the raw ID. The complete 32-bit default and both frame IDs remain in the
package. Dependency discovery reads bounded headers and then invokes the full
existing decoder with actual PALT resources; it never reports semantic success
from a header-only scan.

Each palette contains `id` (unsigned 16-bit), `resource_sha256` (64 hex
characters), `format_version` (0 or 1), `reserved_hex` (16 hex characters for
the eight source bytes), and `colors_rgb_hex` (six hex characters per RGB
triple). Only RGB values are editable. Identity, version, reserved bytes and
color count must match the guarded source. Unused palettes are excluded and
remain untouched, even when opaque or malformed. Required palettes must be in
the same IFF; missing, extra or duplicate package palettes are errors.

Transparent indices may exceed 255. Transparent RGB uses the full unsigned
16-bit palette entry while the stored index sample is its low byte. Thus
transparent entry 257 uses `palette[257]` RGB and stored index byte 1. Both
facts are retained; indices are never inferred from RGB.

## Guards, preservation and reports

Import checks the complete IFF hash, SPR2 payload hash/version, all frame
identities, every required PALT payload hash and immutable palette metadata
against one source snapshot. It checks frame/palette relations and encoder
constraints before committing. A later failure, including an oversized encoded
row or unsupported map, cannot publish an earlier palette change.

Palette-only edits retain the original SPR2 payload byte for byte. Sprite
changes canonicalize the selected SPR2 through the existing writer, which can
change command choices, offsets, padding and unused tails, including encoding
details of semantically unchanged frames. Stored pixel values and metadata
remain those validated from the package. Unrelated payloads, keys, flags, all
64 label bytes and resource order remain exact.

A palette is a shared resource: changing its RGB entries can change the
appearance of every sprite that refers to it, even when those sprites' payloads
remain untouched. This workflow does not clone or remap palettes. Recooking
propagates the changed palette identity through dependent content keys.

Supported version 0/1 `rsmp` maps and the header pointer are regenerated by the
original-aware IFF writer. Unsupported maps allow exact no-ops and reject
changed output. See [indexed IFF rebuilding](indexed-iff.md) for source-map
consistency requirements.

`sprite-import` prints one JSON report:

| Field | Meaning |
| --- | --- |
| `schema_version` | Integer 1. |
| `source_sha256`, `output_sha256` | Complete IFF hashes before and after import. |
| `changed_resources` | Authored PALT/SPR2 changes with `kind_hex`, `id`, `before_sha256` and `after_sha256`. Writer-managed map/header changes appear in the complete output hash. |
| `changed_frames` | Zero-based frames whose stored dimensions, position, flags, indices, depth or alpha change. Palette-only changes do not add entries. |
| `quantized_alpha_pixels` | Authored alpha bytes rounded in explicit source mode, including rounding back to existing source values. |

## Library API and bounds

```rust
use wonderland_creator::{default_limits, ResourceDocument};
use wonderland_creator::editors::sprites::SpritePackage;
use wonderland_legacy_formats::sprites::Spr2AlphaMode;

let limits = default_limits();
let mut document = ResourceDocument::import(&input_bytes, &limits)?;
let mut package = document.export_sprite(8, &limits)?;
package.set_pixel(0, [0, 0], 2, 123, Some(19), &limits)?;
package.set_palette_color(7, 2, [40, 50, 60])?;
package.set_alpha_mode(Spr2AlphaMode::Exact)?;
let text = package.to_json(&limits)?;
let reviewed = SpritePackage::from_json(text.as_bytes(), &limits)?;
let report = document.import_sprite(&reviewed, &limits)?;
let output_bytes = document.export(&limits)?;
```

`SpritePackage` owns private validated state. Setters either change their
requested values completely or leave the package unchanged. Import returns
`SpriteImportReport`, with `SpriteResourceChange` entries described above. The
library performs no file writes; the CLI supplies workspace checks, source
rechecks and atomic publication.

Package input/output has a hard 16 MiB cap, reduced by caller
`max_input_bytes`, `max_resource_bytes` and total allocation limits. Typed JSON
parsing reserves a conservative multiple of wire size before deserialization;
scalar map visitors reject unknown fields before buffering their values.
Serialization counts the complete output before allocating it. Frame counts
use the stricter of `max_frames` and `max_entries`. Pixel admission sums all
frames; zero-width frames still consume frame-count and metadata budgets.

Document copies, dependency collections, palettes, decoded/authored planes,
encoded payloads, reports and final writer workspace share conservative checked
allocation admission. Reopening reserves collection storage alongside the
retained candidate and encoded output. Smaller limits supplied to an existing
document are enforced again. A resource below individual byte/pixel caps can
still fail its aggregate working-allocation limit.

## Deliberate scope and verification

This workflow edits existing indexed sprites. It does not create/remove/reorder
frames, change resource or palette identities, add palette colors, import
external palettes, choose colors from RGB images, write SPR#, remap DGRP
references, or provide graphical painting and preview.

Typed export and import require that decoded source pixels can be represented
by the existing SPR2 encoder. Some accepted legacy command streams leave
hidden zero RGB after short rows; canonical encoding would normalize those
values. Typed authoring refuses those sources before edits. Ordinary raw IFF
import/export remains exact and available. Generic `inspect`, `validate` and
`unknown` replacement continue to treat SPR2 as raw interchange; their success
does not certify sprite semantics. The dedicated `sprite-*` path supplies
contextual sprite validation.

```sh
cargo test --locked --manifest-path tools/creator/Cargo.toml --test authoring
cargo test --locked --manifest-path tools/creator/Cargo.toml
cargo fmt --manifest-path tools/creator/Cargo.toml --check
cargo clippy --locked --manifest-path tools/creator/Cargo.toml --all-targets --no-deps -- -D warnings
```

Tests use hand-authored commands, real compiled CLI calls and actual format
decoders without original installation assets. They cover both layouts, exact
no-ops, combined/palette-only changes, quantization, indexed maps, transparent
index 257, palette fallbacks, geometry/channel changes, strict schema/guards,
late atomic failure, unsupported maps, path/byte bounds, aggregate pixels,
empty-frame counts, retained writer allocations and unrepresentable source
pixels.

The cooker acceptance recooks through actual import and packing APIs. Pixel
and palette edits change sprite/dependent-DGRP derived cache keys. Unrelated
payloads and content hashes remain exact, and an independent source/pack group
retains its derived key. Same-IFF derived keys can change with whole-file
provenance even when their payload is unchanged; the test distinguishes these
observations. This proves recooked content-key propagation, not live renderer
cache eviction. Codec evidence and original encoder/reader comparisons remain
in [palette and sprite authoring](sprite-authoring.md).
