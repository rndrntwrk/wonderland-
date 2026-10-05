# Palette and sprite authoring

The CPU-only format package can now write **PALT 0/1 and SPR2 1000/1001** resources. The APIs consume explicit palette indices, RGBA pixels, and optional depth from the existing decoded types. They do not choose a palette, render sprites, or silently quantize colors.

## Public API

```rust
use wonderland_legacy_formats::sprites::{
    encode_palt, encode_spr2, encode_spr2_with_palettes,
    EncodedSpr2, Spr2AlphaMode,
};

// One palette; reject a change that cannot preserve its exact alpha bytes.
let palette_bytes = encode_palt(&palette, &limits)?;
let sprite_bytes = encode_spr2(&sprites, &palette, &limits)?;

// Explicit source-compatible alpha quantization; resolve each effective PALT ID.
let EncodedSpr2 { bytes, quantized_alpha_pixels } = encode_spr2_with_palettes(
    &sprites,
    |id| palettes.get(&id),
    Spr2AlphaMode::QuantizeLikeSource,
    &limits,
)?;
```

`encode_palt` preserves the input version, eight reserved bytes, and RGB entries. Every palette entry must have alpha 255 because PALT has no alpha channel. A rejected encode leaves the supplied value untouched.

`encode_spr2` is the strict convenience API. The palette-aware variant resolves each frame's **effective** palette ID exactly once. For version 1000, all frames use the low 16 bits of the file's default palette ID. Version 1001 uses the default when the raw frame palette ID is zero or `0xA3A3`; otherwise it uses the raw ID. A mismatch with the decoded effective ID is rejected.

## What the writer preserves and recomputes

| Data | Behavior |
|---|---|
| Set version, default palette ID, declared frame count | Preserved; count must equal the supplied frame count |
| Frame dimensions, supported channel flags, position, raw palette ID, transparent index | Preserved after consistency validation |
| Frame offsets, stored frame sizes, offset table | Recomputed from checked output lengths |
| Palette RGB and pixel indices | Must agree; no inferred color quantizer |
| Alpha | Exact mode rejects nonrepresentable values; explicit source mode reports how many alpha bytes change |
| Transparent pixels | Must match the source transparent palette RGB/index and depth 255 when depth is present |
| RLE command choice and padding | Canonical source command selection; new padding bytes are zero |
| Original command choices, arbitrary original padding, unused encoded tails | Preserved only by unchanged raw-resource passthrough, not by canonical re-encoding |

Partial alpha uses a five-bit stored value. The original encoder calculates `ceil(alpha * 31 / 255)` and the decoder calculates `floor(stored * 255 / 31)`. The Rust implementation uses exact bounded integer arithmetic. For example, alpha 123 is exactly representable; alpha 124 becomes 131 only when the caller selects `QuantizeLikeSource`. The returned `quantized_alpha_pixels` makes that change observable.

Color and palette-index arrays are required for authoring. A depth array must be present exactly when its channel flag is set and must have the full frame dimensions. Partial alpha also requires depth because its source command stores both. The writer preserves the source alpha flag rather than assuming it controls command decoding: the original reader decodes alpha commands regardless of that flag. Unknown flags, SPR# input, inconsistent per-frame versions, reserved fields that SPR2 cannot represent, missing palettes, and mismatched arrays fail explicitly.

## Bounds and atomicity

Every frame is checked before output allocation. The writer checks frame and palette counts, frame pixel limits, retained frame/pixel memory, aggregate output bytes, 32-bit offsets, and the source's 13-bit command fields. It holds a small checked frame plan while calculating the exact output allocation.

Transparent runs and skipped row spans larger than 8,191 are split into valid commands. A nontransparent row whose encoded byte count exceeds the format field is rejected; it cannot be split into another row without changing the image. This deliberately avoids the original writer's truncating cast on oversized inputs. The 9,000-row and 9,000-pixel transparent-run tests exercise those boundaries.

To publish the resource, replace the guarded resource payload in a creator transaction and export through the original-aware IFF writer. [Indexed IFF rebuilding](indexed-iff.md) preserves the resource map when supported; [creator transactions](creator.md) provide source/resource guards and atomic filesystem publication. The sprite API itself performs no file writes.

## Source and verification

Source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

- `TSOClient/tso.files/Formats/IFF/Chunks/PALT.cs`: palette read/write layout.
- `TSOClient/tso.files/Formats/IFF/Chunks/SPR2.cs`: both container layouts, palette fallback, frame header, channel and transparency decoding.
- `TSOClient/tso.files/Formats/IFF/Chunks/SPR2FrameEncoder.cs`: run selection, row records, padding, skip/end markers, and alpha quantization.

Run the focused Rust tests and independent source command check:

```sh
cargo test --locked --manifest-path crates/legacy-formats/Cargo.toml --test sprite_authoring
python3 tools/swarm-b/sprite-oracle.py
python3 tools/swarm-b/sprite-reader-oracle.py
```

The source check compiles the **unchanged** `SPR2FrameEncoder.cs` with minimal frame/Color containers and byte-writing plumbing. It checks 257 authored command vectors: a mixed command frame, transparent rows, an odd color run, and every partial input alpha from 1 through 254. The concatenated command stream SHA-256 is `fc27c24370758905ed03846d98ff9862b2bf89cd421c84d6a713d30e49640ce0`.

This is direct evidence for the source encoder's command behavior. It is not full original-runtime resource loading, renderer comparison, sprite editing UI, or gameplay qualification. Source-derived oversized-input handling is tested separately from the original writer, whose wrapping output is intentionally not reproduced.

The independent reader probe feeds Rust-authored output into four methods extracted unchanged from the pinned `SPR2.cs`. It passes **18 cases, 34 frames, and 294,034 pixels** across both layouts, flags 1/3/5/7, raw/default palette fallbacks, transparent index 257, signed position extrema, zero dimensions, 65,535-row/pixel transparent spans, and maximum valid color/depth/alpha rows. All compared metadata and pixel channels match. The extracted method SHA-256 is `0d5a24fe4449de91f4413dba38208d5ee3a134ed614beb10bdbb4a0ae97479ba`.

Review also caught empty-frame CPU amplification: height does not consume a pixel budget when width is zero. The encoder now emits the blank-row spans directly. The regression covers 1,000 maximum-height, zero-width frames with `max_pixels = 0`, retaining exactly 44,012 output bytes. The source reader still accepts the output. No renderer paths or private installation assets are supplied by either oracle.
