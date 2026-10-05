# Creator: offline resource inspection and guarded edits

`wonderland-creator` is a standalone Rust library and CLI for inspecting local resources and making precise, reviewable edits. It consumes the real `wonderland-legacy-formats` IFF/container parsers and semantic BHAV, strings, BCON, OBJD and SLOT APIs. It does not implement a VM or a graphical Volcanic replacement.

## Build and verify without external assets

Run from the repository root with Rust 1.90.0. The CLI regression uses Python 3's standard library to generate all input fixtures and validate its JSON output.

```sh
cargo test --locked --manifest-path tools/creator/Cargo.toml -- --nocapture
cargo clippy --locked --manifest-path tools/creator/Cargo.toml --all-targets --no-deps -- -D warnings
cargo run --locked --manifest-path tools/creator/Cargo.toml -- --help
cargo run --locked --manifest-path tools/creator/Cargo.toml -- inventory
cargo run --locked --manifest-path tools/creator/Cargo.toml -- debug-capabilities
```

Add `--offline` after the dependencies are cached if the build must not access
the package registry.

The regression executes import → inspect → branch edit → export → reopen through the actual compiled CLI. It checks exact intended byte changes, unknown chunk retention, malformed edits and stale hashes/versions preserving the existing output, path traversal and symlink rejection, FAR extraction to an explicitly chosen output, a real-size synthetic city BMP edit, and the explicit unsupported debugger result. No original assets are bundled or required.

## CLI workflow

All file commands accept `--root DIRECTORY` before the command. Every input and output path is relative to this directory. Output parent directories must already exist. Existing output files are replaced atomically after successful validation. Commands return exit code `0` on success and `2` on error.

```sh
creator --root ./private-work inspect object.iff
creator --root ./private-work list object.iff
creator --root ./private-work validate object.iff
creator --root ./private-work import object.iff imported.iff
creator --root ./private-work export imported.iff exported.iff
creator --root ./private-work metadata object.iff metadata.json
creator --root ./private-work extract object.iff BHAV 4096 routine.bin
```

`inspect` emits JSON with the exact file SHA-256, header hex, every chunk's type/key, flags, label hex, size, payload hash, source format version, and validation status. BHAV instructions include opcode, both branch bytes and the eight operand bytes. Strings include exact language-set boundaries, recorded language codes, encoding, readable values/comments and original value hex. BCON constants and SLOT counts are included. Lossless hex fields remain usable when labels/type bytes are not text.

`list` provides a compact resource-key/size/hash inventory. `validate` checks known BHAV, BCON, STR#/CTSS/TTAs, SLOT and OBJD payloads. Unknown kinds remain opaque and are retained; successful validation does not certify their internal semantics. `inspect` can report a malformed supported payload while preserving its envelope for examination; a typed edit of that payload requires successful decoding.

Import/export is a bounded IFF round trip. An untouched document exports the original bytes. Unknown chunk order, labels, flags and data survive edits to a different chunk. Indexed IFF resource-map metadata (`rsmp` or a nonzero header pointer) is opaque: exact unchanged export works, while a changed document fails explicitly because rebuilding that metadata is not implemented.

### Guarded edits

Every CLI edit requires the source file SHA-256 from `inspect` and the exact target resource's on-disk format version. Decimal and `0x` hexadecimal integers are accepted. Use `none` for resources with no format-version field, including BCON and unknown payloads. String format versions are the unsigned 16-bit representation of the recorded signed value: `0xfffc` represents format `-4`, for example. Every new output has a new SHA-256 guard.

```sh
creator --root ./private-work edit object.iff edited.iff BHAV 4096 FILE_SHA256 0x8002 bhav-branch 0 254 255
creator --root ./private-work edit object.iff edited.iff BHAV 4096 FILE_SHA256 0x8002 bhav-operand 0 0102030405060708
creator --root ./private-work edit object.iff edited.iff 'STR#' 129 FILE_SHA256 0xfffd string 0 1 'Replacement text'
creator --root ./private-work edit object.iff edited.iff BCON 4096 FILE_SHA256 none tuning 2 750
creator --root ./private-work edit object.iff edited.iff SLOT 128 FILE_SHA256 4 slot 0 1.0 -2.5 3.0
creator --root ./private-work edit object.iff edited.iff ZZZZ 9 FILE_SHA256 none unknown replacement.bin
```

The placeholder `FILE_SHA256` must be replaced by the full 64-character file hash. BHAV routine indices, string set/index, tuning index and slot index are zero-based. BHAV edits retain the source format, reserved header and trailing bytes. Operand editing changes eight raw bytes; primitive-specific operand interpretation is outside this tool's current scope.

CFG validation rejects either branch if it is below `253` and outside the instruction array. Source sentinel `254` returns true and `255` returns false. Sentinel `253` follows the available alternate branch, or becomes a VM error when both branches are `253`. Metadata separately reports unreachable instructions and both-253 error cases. Those reports are observations, not proof of script termination or correctness. Valid loops remain valid.

String edits address an explicit language set; they do not apply locale fallback or create a translation set. The original text encoding is retained. A character outside ASCII/Latin-1 fails instead of silently replacing data. Comments, language codes, entry order, ignored/unassigned source entries and trailing data are retained by the actual semantic codec. A source whose encoding cannot round-trip exactly is refused for editing rather than normalized. Old SLOT versions keep their on-disk version and absent fields; finite offsets are required. BCON edits retain flags and trailing bytes.

Unknown binary replacement is deliberately restricted to kinds for which the creator has no typed editor/validator. It is explicit binary interchange, not a promise that the replacement is meaningful to the original runtime. There is no chunk-add/delete/reorder operation.

External OTF tuning files have their own inspection and constant-edit commands:

```sh
creator --root ./private-work otf-inspect object.otf
creator --root ./private-work otf-edit object.otf edited.otf FILE_SHA256 3 7 -99
```

The final arguments select the signed table ID, signed key ID and signed 32-bit constant. IDs must identify a unique table/key; missing and duplicate matches fail. OTF has no on-disk numeric version field, so the guard is the exact source SHA-256 plus precise table/key identity. The real semantic encoder changes only the selected XML attribute span, retaining unknown XML, comments, quote style and unrelated whitespace. Structural edits, DTDs/general entities and unsupported XML encodings are refused.

The source file is read again and its SHA-256 is checked immediately before publishing an output. A normal external source change is a conflict. A rejected edit does not alter either the in-memory document or an existing output file.

### Containers

```sh
creator --root ./private-work container-list far1a archive.far
creator --root ./private-work container-extract far1a archive.far 0 resource.iff
```

Formats are `far1a`, `far1b`, `far3`, and `dbpf`, subject to the exact versions/compressions supported by `wonderland-legacy-formats`. The list preserves the parser's real filename or type/group/instance identity. Extraction uses a numeric index and an explicitly selected safe output path. Archive-controlled filenames are never used as filesystem output paths. The tool does not write or rebuild archives.

## City data maps

City layers are semantic images. The source uses exact RGB palette values for terrain and forest type; elevation, forest density and roads use the red channel. This tool edits uncompressed Windows BMP with a 40-byte BITMAPINFOHEADER and 24- or 32-bit pixels. Other DIB layouts, indexed BMP, compression, PNG and ambiguous sizes/offsets are explicitly unsupported. Positive height means bottom-up storage; negative height means top-down. Both work.

```sh
creator --root ./private-work city-inspect terraintype.bmp
creator --root ./private-work city-validate terraintype.bmp terrain
creator --root ./private-work city-edit terraintype.bmp edited.bmp FILE_SHA256 terrain 20 30 255 0 0
creator --root ./private-work city-export-ppm edited.bmp exact-rgb.ppm
```

`city-validate` and `city-edit` require 512×512 map dimensions. Layers are `terrain`, `elevation`, `forest-density`, `forest-type`, `road`, and `vertex-color`. Pixel coordinates are zero-based from the top left, independent of BMP row storage. Editing patches only the pixel's three BGR bytes. Header bytes, alpha, unused bytes, row padding, orientation and every unrelated pixel are preserved. No blur, resampling, quantization or color-space conversion is performed.

| Terrain | Exact RGB |
| --- | --- |
| Grass | `(0, 255, 0)` |
| Water | `(12, 0, 255)` |
| Snow | `(255, 255, 255)` |
| Rock | `(255, 0, 0)` |
| Sand | `(255, 255, 0)` |
| No terrain | `(0, 0, 0)` |

| Forest type | Exact RGB |
| --- | --- |
| Heavy forest / fir | `(0, 106, 40)` |
| Light forest / birch | `(0, 235, 66)` |
| Palm | `(255, 252, 0)` |
| Cactus | `(255, 0, 0)` |
| No forest | `(0, 0, 0)` |

RGB P6 PPM export retains exact RGB sample values and explicitly omits alpha. It is an interchange export; PPM is not claimed to be a runtime city input. BMP output is the actual supported runtime-style interchange. The tool does not rebuild neighbouring road/corner bits, provide brush strokes, render a city, edit neighbourhood records, or publish map data to a server. Most bundled FreeSO maps are PNG and therefore cannot yet be edited by this BMP-only foundation.

## Library API and provider boundary

```rust
use wonderland_creator::{default_limits, Edit, ResourceDocument};
use wonderland_legacy_formats::iff::ChunkKey;

let limits = default_limits();
let mut document = ResourceDocument::import(&input_bytes, &limits)?;
let key = ChunkKey { kind: *b"BHAV", id: 4096 };
let expected = document.guard(key, &limits)?;
document.edit(key, &expected, Edit::BhavBranch {
    instruction: 0, true_pointer: 254, false_pointer: 255,
}, &limits)?;
let edited_bytes = document.export(&limits)?;
```

`EditGuard` carries exact whole-document SHA-256, target resource SHA-256, and `Option<u32>` format version. All three are checked by the library. The CLI takes an explicit whole-file hash and version and derives the resource hash from that already-guarded input. `ResourceDocument::file()` and `chunk()` expose immutable views. Mutation goes through `edit()` and is validated/serialized before replacing the stored document. `metadata_json()` exports the inspection interchange.

`TuningDocument::import()`, `metadata_json()`, `edit_constant()` and `export()` expose the guarded, XML-preserving OTF workflow.

`Workspace::read()` and `write_atomic()` provide bounded file access; the CLI performs source hash rechecks around publication. `BmpMap::width()` and `height()` expose the validated geometry through read-only getters; geometry and storage remain private and cannot be resized through field mutation. `BmpMap::decode()`, `pixel()`, `set_pixel()`, `validate()` and `ppm()` form the exact map-editing API. Library users must choose finite `Limits` and a finite BMP pixel limit.

`debug::IsolatedDebugProvider` accepts an explicit `DebugSnapshot` and exposes watches, trace events and an optional isolated step. This interface may be connected only to a real isolated VM adapter. The installed `UnsupportedDebugProvider` returns an explicit unsupported error for watches, trace generation and stepping. `debug-capabilities` reports every execution capability as false, and `debug-step` exits with code 2. There are no made-up VM observations, instruction execution, live mutation, network calls or durable effects.

## Bounds and filesystem assumptions

CLI reads and outputs are limited to 64 MiB. Default parser limits are 10,000 entries, 16 MiB per resource, 1 MiB per string, 1,048,576 BMP pixels and a 192 MiB aggregate working-copy budget. Resource-document import reserves five source/copy footprints plus entry overhead against that budget, so large documents can be refused below the 64 MiB input ceiling. Edited candidates are checked against the same reserve before publication. Source versions, counts, offsets and allocation limits are enforced by the actual parsers.

Only ordinary relative path components are accepted. Absolute paths, `..`, explicit `.` components, symlink input/output leaves and symlink parents are refused. Input/output special files such as FIFOs are refused before open or rename. New outputs use a unique sibling temporary file that is checked to differ from the requested destination before it is opened with `create_new`, complete write and `sync_all`, followed by rename; on Unix the parent is synced as well. Temporary files are removed on a pre-publication error. A post-rename directory-sync failure can report an error after the complete output was published; it cannot leave a partially written output. Atomic replacement behavior follows the host filesystem's rename guarantees.

The root must be a private local directory protected against concurrent untrusted symlink/parent renames. The safe standard-library implementation validates path components and rechecks before publication; it does not implement descriptor-relative `openat` traversal or an atomic compare-and-swap against hostile concurrent filesystem changes. Root aliases are canonicalized, and the final root component itself must not be a symlink. This is a deliberate boundary, not a sandbox claim.

## Source anchors and exact unsupported scope

The implementation was read against source revision `4c6b3e8f5835b228723caea3c9f683c62f244f73`:

- `TSOClient/tso.files/Formats/IFF/IffFile.cs`: 60-byte IFF identifier, big-endian header/chunk envelope, raw chunk handling and resource-map pointer.
- `TSOClient/tso.files/Formats/IFF/Chunks/BHAV.cs`: source versions 0x8000–0x8003 and 12-byte instructions.
- `TSOClient/tso.simantics/Engine/VMThread.cs::MoveToInstruction`: branch bytes 253/254/255.
- `TSOClient/tso.files/Formats/IFF/Chunks/STR.cs`: format 0 and -1 through -4, explicit language sets, comments and encodings.
- `TSOClient/tso.files/Formats/OTF/OTFFile.cs`: signed table/key identity and external tuning constants.
- `TSOClient/tso.files/Formats/IFF/Chunks/BCON.cs`: flags and unsigned 16-bit constants.
- `TSOClient/tso.files/Formats/IFF/Chunks/SLOT.cs`: chronological slot order, version-specific fields and offset floats.
- `TSOClient/tso.client/Rendering/City/CityMapData.cs`: map layer channels, exact terrain palette and 512×512 save shape.
- `TSOClient/tso.client/Rendering/City/CityFoliage.cs`: exact forest palette.
- `TSOClient/tso.client/Rendering/City/Plugins/MapPainterPlugin.cs` and `Documentation/Crafting a City.md`: original city tools and their map outputs.
- `TSOClient/FSO.IDE/ResourceBrowser/ResourceEditors/UnknownResourceControl.cs` and `OTFResourceControl.cs`: opaque resource UI and OTF display scope.
- `Other/tools/FarExtractor/FarExtractor/Form1.cs`, `Other/tools/XaToWav/XaToWav/Program.cs`, and `Other/tools/Mr. Shipper/Mr. Shipper/Program.cs`: archive, audio converter and asset shipping census.

`docs/compat/tools.json` inventories all immediate `Other/tools` projects and Volcanic/City Painter. It records partial foundations separately from unsupported graphical/runtime behavior. This package does not claim completed W16 parity, effective content-catalog identity/rights resolution, upgrades authoring, patch-chain authoring, graphical previews, native tool compatibility, live VM stepping, spritesheet or mesh authoring, neighborhood/server edits, or original-runtime differential verification.
