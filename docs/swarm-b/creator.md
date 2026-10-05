# Creator: offline resource inspection and guarded transactions

`wonderland-creator` is a standalone Rust library and CLI for inspecting local resources and making precise, reviewable edits. It consumes the real `wonderland-legacy-formats` IFF/container parsers, semantic resource APIs and palette/sprite codecs. It supports source-bound add, remove, metadata and typed-edit transactions, including source-backed resource-map rebuilding, plus [guarded SPR2 editing packages](creator-sprites.md) with exact index/alpha/depth planes and palette RGB. It does not implement a VM or a graphical Volcanic replacement.

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

The regressions execute import → inspect → edit/transaction → export → reopen through the actual compiled CLI. They check exact intended byte changes, add/remove/key/flags/label operations, opaque chunk retention, multi-operation commits, replay and resource-version/hash conflicts, late validation failures, strict JSON fields/types, bounded sidecar reads, path traversal and symlink rejection, indexed-map rebuilding and unsupported-map failure, typed palette RGB edits, FAR extraction, a real-size synthetic city BMP edit, and the explicit unsupported debugger result. Library tests additionally verify that a late candidate failure preserves both parsed state and exported bytes, explicit key swaps, allocation/count limits, and consecutive transactions using refreshed map state. No original assets are bundled or required.

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

`inspect` emits JSON with the exact file SHA-256, header hex, every chunk's type/key, flags, label hex, size, payload hash, source format version, and validation status. BHAV instructions include opcode, both branch bytes and the eight operand bytes. Strings include exact language-set boundaries, recorded language codes, encoding, readable values/comments and original value hex. BCON constants, SLOT counts and PALT `colors_rgb` entries are included. Lossless hex fields remain usable when labels/type bytes are not text.

`list` provides a compact resource-key/size/hash inventory. `validate` checks known BHAV, BCON, STR#/CTSS/TTAs, SLOT, OBJD, TTAB, GLOB, PIFF and PALT payloads. Unknown kinds remain opaque and are retained; successful validation does not certify their internal semantics. `inspect` can report a malformed supported payload while preserving its envelope for examination; a typed edit of that payload requires successful decoding. Existing unrelated payloads remain byte-exact during transactions; run `validate` when whole-document semantic validation is desired.

Import/export is a bounded IFF round trip. An untouched document exports the original bytes. Unchanged chunks keep their order, labels, flags and data; additions append in transaction order and removals preserve the surviving order. Indexed files use `iff::encode_rebuilding_index` against the exact current source. Supported version 0/1 maps rebuild offsets, counts, type groups, IDs, flags, visible labels, sizes and the header pointer. The source map must already be complete and consistent with the source envelope. Unsupported or inconsistent maps retain exact unchanged export and fail explicitly on changed output. There is no silent map repair, map removal or conversion to an unindexed file.

### Guarded edits

Every CLI edit requires the source file SHA-256 from `inspect` and the exact target resource's on-disk format version. Decimal and `0x` hexadecimal integers are accepted. Use `none` for resources with no format-version field, including BCON and unknown payloads. String format versions are the unsigned 16-bit representation of the recorded signed value: `0xfffc` represents format `-4`, for example. Changed output has a new SHA-256 guard; byte-identical no-ops retain it.

```sh
creator --root ./private-work edit object.iff edited.iff BHAV 4096 FILE_SHA256 0x8002 bhav-branch 0 254 255
creator --root ./private-work edit object.iff edited.iff BHAV 4096 FILE_SHA256 0x8002 bhav-operand 0 0102030405060708
creator --root ./private-work edit object.iff edited.iff 'STR#' 129 FILE_SHA256 0xfffd string 0 1 'Replacement text'
creator --root ./private-work edit object.iff edited.iff BCON 4096 FILE_SHA256 none tuning 2 750
creator --root ./private-work edit object.iff edited.iff SLOT 128 FILE_SHA256 4 slot 0 1.0 -2.5 3.0
creator --root ./private-work edit object.iff edited.iff PALT 128 FILE_SHA256 1 palette 7 40 50 60
creator --root ./private-work edit object.iff edited.iff ZZZZ 9 FILE_SHA256 none unknown replacement.bin
```

The placeholder `FILE_SHA256` must be replaced by the full 64-character file hash. BHAV routine indices, string set/index, tuning index and slot index are zero-based. BHAV edits retain the source format, reserved header and trailing bytes. Operand editing changes eight raw bytes; primitive-specific operand interpretation is outside this tool's current scope.

CFG validation rejects either branch if it is below `253` and outside the instruction array. Source sentinel `254` returns true and `255` returns false. Sentinel `253` follows the available alternate branch, or becomes a VM error when both branches are `253`. Metadata separately reports unreachable instructions and both-253 error cases. Those reports are observations, not proof of script termination or correctness. Valid loops remain valid.

String edits address an explicit language set; they do not apply locale fallback or create a translation set. The original text encoding is retained. A character outside ASCII/Latin-1 fails instead of silently replacing data. Comments, language codes, entry order, ignored/unassigned source entries and trailing data are retained by the actual semantic codec. A source whose encoding cannot round-trip exactly is refused for editing rather than normalized. Old SLOT versions keep their on-disk version and absent fields; finite offsets are required. BCON edits retain flags and trailing bytes.

PALT `palette INDEX R G B` changes one zero-based RGB triple using the real palette decoder/encoder. Each channel must be an integer from 0 through 255. Source version 0/1, color count, reserved bytes, every other color and unrelated chunks are preserved. PALT stores RGB only, so this operation does not invent an alpha channel. `inspect` reports each entry as an RGB array; sprite rendering and dependent-palette previews remain outside the CLI.

Unknown binary replacement is restricted to kinds for which the creator has no typed validator. It is explicit binary interchange, not a promise that the replacement is meaningful to the original runtime. Known kinds cannot bypass validation through `unknown`. The `rsmp` resource is always managed by the IFF writer and cannot be added, removed, rekeyed, relabeled or raw-replaced directly.

### Guarded sprite packages

The dedicated `sprite-export`, `sprite-pixel`, `sprite-palette`,
`sprite-alpha-mode` and `sprite-import` commands provide a source-bound SPR2
1000/1001 and PALT 0/1 editing workflow. A strict JSON package carries the exact
index, straight-alpha and optional depth planes with required palette RGB.
Import checks source/resource hashes, preserves frame and palette identities,
and publishes palette/sprite changes in one atomic IFF replacement. Alpha is
exact by default; source-compatible quantization is an explicit package choice
and reports the number of affected samples. Palette-only edits and semantic
no-ops retain the original SPR2 bytes, including after quantization back to
existing values.

See [Creator sprite packages](creator-sprites.md) for the commands, complete
schema, geometry/channel editing, reports, allocation limits and recooking
acceptance. Contextual sprite validation belongs to these commands. Generic
`inspect`, `validate` and `unknown` replacement still treat SPR2 as raw
interchange and do not certify its internal sprite semantics.

### Add, remove and metadata commands

```sh
creator --root ./private-work add object.iff added.iff BCON 50000 FILE_SHA256 0x10 LABEL_HEX constants.bin
creator --root ./private-work remove object.iff removed.iff BCON 4096 FILE_SHA256 RESOURCE_SHA256 none
creator --root ./private-work set-metadata object.iff renamed.iff BHAV 4096 FILE_SHA256 RESOURCE_SHA256 0x8002 BHAV 4097 0x10 LABEL_HEX
```

`RESOURCE_SHA256` is the target chunk's `sha256` from `inspect`; `FILE_SHA256` is its enclosing `source_sha256`. Both accept 64 hexadecimal characters, case-insensitively. `LABEL_HEX` must contain exactly 128 hexadecimal characters representing all 64 on-disk label bytes, including NULs and any opaque bytes after the first NUL. Copy `label_hex` from `inspect` to retain the label. The `flags` value and IDs must fit unsigned 16-bit integers. Four-byte kinds in these commands are supplied as text; JSON transactions support every possible four-byte kind through `kind_hex`.

`add` reads a bounded raw payload, validates every known semantic kind plus PALT, and requires that the new `(kind, ID)` is absent. `remove` checks the source, payload hash and exact format version before removing the target. `set-metadata` requires all new metadata explicitly: kind, ID, flags and label bytes. Its payload is preserved and validated under the new kind. A key collision fails; another resource is never overwritten implicitly. Resource IDs and labels do not rewrite references in BHAVs, object records, palettes or other resources. Choose all dependent changes explicitly.

Versions are those reported by `inspect`: BHAV/strings/SLOT/OBJD/PALT/PIFF use their recorded format field; TTAB uses its field when present and `none` for the source's empty table without a version. BCON, GLOB and opaque kinds use `none`. Unsupported or malformed versioned targets whose guard cannot be decoded are refused.

### Atomic JSON transactions

```sh
creator --root ./private-work transaction object.iff candidate.iff transaction.json
creator --root ./private-work inspect candidate.iff
creator --root ./private-work validate candidate.iff
```

The transaction file has exactly three top-level fields: `schema_version` (integer `1`), `source_sha256` (64 hexadecimal characters) and `operations` (array). All operations address the same pretransaction snapshot. Existing resources require an `expected` object containing both `resource_sha256` and `format_version`; `null` is mandatory when no version exists. Missing versions, unknown fields, duplicate fields, wrong JSON types, invalid hex and out-of-range integers are errors. JSON numbers are decimal; the CLI's `0x` convenience does not apply inside JSON.

This example combines a typed operand edit, a removal and a validated addition. Replace the uppercase hash placeholders with hashes from `inspect` and the exact supplied payload. The label shown is 64 zero bytes.

```json
{
  "schema_version": 1,
  "source_sha256": "FILE_SHA256",
  "operations": [
    {
      "op": "edit",
      "key": {"kind_hex": "42484156", "id": 4096},
      "expected": {"resource_sha256": "BHAV_SHA256", "format_version": 32770},
      "edit": {"op": "bhav-operand", "instruction": 0, "operand_hex": "0102030405060708"}
    },
    {
      "op": "remove",
      "key": {"kind_hex": "5a5a5a5a", "id": 9},
      "expected": {"resource_sha256": "UNKNOWN_SHA256", "format_version": null}
    },
    {
      "op": "add",
      "key": {"kind_hex": "42434f4e", "id": 50000},
      "flags": 16,
      "label_hex": "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
      "payload_file": "constants.bin",
      "payload_sha256": "CONSTANTS_FILE_SHA256"
    }
  ]
}
```

Every `key` and `new_key` has exactly `kind_hex` (eight hexadecimal characters) and `id` (unsigned 16-bit integer). Required operation fields are:

| `op` | Required fields beyond `op` | Result |
| --- | --- | --- |
| `add` | `key`, `flags`, `label_hex`, `payload_file`, `payload_sha256` | Append one new resource after validation. |
| `remove` | `key`, `expected` | Remove one source resource. |
| `metadata` | `key`, `expected`, `new_key`, `flags`, `label_hex` | Preserve payload; change exact envelope metadata. |
| `edit` | `key`, `expected`, `edit` | Apply one typed payload edit. |

The nested `edit` object accepts these exact forms:

| Nested `op` | Required fields beyond `op` |
| --- | --- |
| `bhav-branch` | `instruction`, `true_pointer`, `false_pointer` |
| `bhav-operand` | `instruction`, `operand_hex` (16 hex characters) |
| `string` | `set`, `index`, `value` |
| `tuning` | `index`, `value` (unsigned 16-bit integer) |
| `slot` | `index`, `offset` (three finite numbers) |
| `palette` | `index`, `rgb` (three unsigned byte integers) |
| `unknown` | `payload_file`, `payload_sha256` |

Sidecar paths are relative to `--root`, regardless of the transaction file's subdirectory. Each sidecar is read through the same bounded workspace path/type checks and its SHA-256 must match the spec. This binds the reviewed transaction to its new payload bytes as well as the input IFF. An empty operations array is a guarded exact no-op.

An existing source key may appear in only one operation. Combine independent resource changes in a single transaction; use a fresh source hash for a later edit of the same resource. Added keys must be absent from the source, even if another operation removes that key. All final keys must be unique. Explicit metadata operations can swap two keys in one transaction because collisions are checked against the complete final key set, not a transient partially applied state. Unrelated chunks keep source order; additions follow in operation order.

Every guard, operation, payload and final IFF serialization is checked before the in-memory state changes. Successful output is reopened internally so the next transaction sees the regenerated header/map and fresh hashes. A late failure leaves the document and any existing output untouched. The CLI rechecks the input hash immediately before its atomic output replacement. Replaying a spec against changed input fails, including when input and output are the same path. Byte-identical no-ops retain their source hash and remain replayable.

To generate a valid, concrete transaction without manually copying hashes, this example adds a BCON resource and removes an explicitly chosen opaque resource. It assumes `object.iff` contains `ZZZZ/9` and does not contain `BCON/50000`:

```sh
creator --root ./private-work metadata object.iff metadata.json
python3 - <<'PY'
import hashlib, json, pathlib
root = pathlib.Path('private-work')
metadata = json.loads((root / 'metadata.json').read_text())
old = next(c for c in metadata['chunks'] if (c['kind_hex'], c['id']) == ('5a5a5a5a', 9))
assert not any((c['kind_hex'], c['id']) == ('42434f4e', 50000) for c in metadata['chunks'])
payload = bytes([2, 0, 5, 0, 10, 0])  # BCON: count 2, flags 0, constants 5 and 10.
(root / 'constants.bin').write_bytes(payload)
spec = {'schema_version': 1, 'source_sha256': metadata['source_sha256'], 'operations': [
    {'op': 'remove', 'key': {'kind_hex': old['kind_hex'], 'id': old['id']},
     'expected': {'resource_sha256': old['sha256'], 'format_version': old['format_version']}},
    {'op': 'add', 'key': {'kind_hex': '42434f4e', 'id': 50000}, 'flags': 16,
     'label_hex': b'New constants'.ljust(64, b'\0').hex(),
     'payload_file': 'constants.bin', 'payload_sha256': hashlib.sha256(payload).hexdigest()}
]}
(root / 'transaction.json').write_text(json.dumps(spec, indent=2) + '\n')
PY
creator --root ./private-work transaction object.iff candidate.iff transaction.json
```

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

`EditGuard` carries exact whole-document SHA-256, target resource SHA-256, and `Option<u32>` format version. All three are checked by the library. The existing `edit` CLI takes an explicit whole-file hash and version and derives the resource hash from that already-guarded input; new remove/metadata commands and transactions take explicit resource hashes. `ResourceDocument::file()` and `chunk()` expose immutable views. `edit()` wraps a one-operation transaction; all mutation is validated/serialized/reopened before replacing the stored document. `metadata_json()` exports the inspection interchange.

For transactions, construct `ResourceTransaction { source_hash, operations }` and call `document.transact(&transaction, &limits)`. `ResourceOperation` has `Add { chunk }`, `Remove { key, expected }`, `Edit { key, expected, edit }`, and `SetMetadata { key, expected, new_key, flags, label }`. `expected` is a `ResourceGuard { resource_hash, format_version }`; `ResourceGuard::from(&edit_guard)` converts a captured guard. `Add` accepts an `iff::IffChunk` and `SetMetadata` uses exact `[u8; 64]` label bytes. `ResourceTransaction::from_json(bytes, &workspace, &limits)` provides the strict sidecar-aware interchange for library callers. Constructing a transaction does not modify a document; `transact()` enforces source/resource expectations and publishes atomically.

`TuningDocument::import()`, `metadata_json()`, `edit_constant()` and `export()` expose the guarded, XML-preserving OTF workflow.

`Workspace::read()`, `read_limited()` and `write_atomic()` provide bounded file access; narrower read limits cannot raise the workspace ceiling. The CLI performs source hash rechecks around publication. `BmpMap::width()` and `height()` expose the validated geometry through read-only getters; geometry and storage remain private and cannot be resized through field mutation. `BmpMap::decode()`, `pixel()`, `set_pixel()`, `validate()` and `ppm()` form the exact map-editing API. Library users must choose finite `Limits` and a finite BMP pixel limit.

`debug::IsolatedDebugProvider` accepts an explicit `DebugSnapshot` and exposes watches, trace events and an optional isolated step. This interface may be connected only to a real isolated VM adapter. The installed `UnsupportedDebugProvider` returns an explicit unsupported error for watches, trace generation and stepping. `debug-capabilities` reports every execution capability as false, and `debug-step` exits with code 2. There are no made-up VM observations, instruction execution, live mutation, network calls or durable effects.

## Bounds and filesystem assumptions

CLI reads and outputs are limited to 64 MiB. Default parser limits are 10,000 entries/transaction operations, 16 MiB per resource/sidecar, 1 MiB per string, 1,048,576 BMP pixels and a 192 MiB aggregate working-copy budget. Transaction JSON is limited to 1 MiB before read/allocation, with an additional conservative parsed-JSON reserve and cumulative sidecar budget. Resource-document import reserves five source/copy footprints plus entry overhead against the budget, so large documents can be refused below the 64 MiB input ceiling. Transactions reserve operation storage plus an upper bound on source/candidate/codec copies before decoding new payloads or constructing candidates, and check regenerated output before publication. Source versions, counts, offsets and allocation limits are enforced by the actual parsers.

Transaction objects are parsed through map-only visitors; positional arrays are rejected for the document, operations, keys, guards and edits. Unknown field names fail before their values are read. Typed scalar fields and fixed RGB/offset arrays are read directly, without buffering unknown values or internally tagged enum content. Optional operation fields may be absent, but an explicit `null` cannot erase a supplied field or bypass its type/variant checks. Guard `format_version` remains the one required nullable field. Workspace reads allocate the checked file length exactly, use `read_exact`, and probe for growth with one stack byte; truncation or growth during the read is an error. Sidecar capacity, including retained allocation rather than only length, is charged to the transaction budget.

Only ordinary relative path components are accepted. Absolute paths, `..`, explicit `.` components, symlink input/output leaves and symlink parents are refused. Input/output special files such as FIFOs are refused before open or rename. New outputs use a unique sibling temporary file that is checked to differ from the requested destination before it is opened with `create_new`, complete write and `sync_all`, followed by rename; on Unix the parent is synced as well. Temporary files are removed on a pre-publication error. A post-rename directory-sync failure can report an error after the complete output was published; it cannot leave a partially written output. Atomic replacement behavior follows the host filesystem's rename guarantees.

The root must be a private local directory protected against concurrent untrusted symlink/parent renames. The safe standard-library implementation validates path components and rechecks before publication; it does not implement descriptor-relative `openat` traversal or an atomic compare-and-swap against hostile concurrent filesystem changes. Root aliases are canonicalized, and the final root component itself must not be a symlink. This is a deliberate boundary, not a sandbox claim.

## Source anchors and exact unsupported scope

The implementation was read against source revision `4c6b3e8f5835b228723caea3c9f683c62f244f73`:

- `TSOClient/tso.files/Formats/IFF/IffFile.cs`: 60-byte IFF identifier, big-endian header/chunk envelope, raw chunk handling and resource-map pointer.
- `TSOClient/tso.files/Formats/IFF/IffFile.cs::AddChunk`, `FullRemoveChunk`, `MoveAndSwap` and `TSOClient/FSO.IDE/ResourceBrowser/IffNameDialog.cs`: explicit creation/removal, type/ID collision refusal and source key-swap behavior. Creator applies its own atomic snapshot-guard policy around these envelope operations.
- `Other/tools/Iffinator/Iffinator/srcs.zip` (`iff.cpp`, `mk_iff.cpp`) and `Other/tools/SimsLib/SimsLib/IFF/Old/Iff.cs`: source resource-map versions, entry widths, offset/flags/name representation and writer layout. Exact supported-map constraints are implemented in `crates/legacy-formats/src/iff/index.rs`.
- `TSOClient/tso.files/Formats/IFF/Chunks/BHAV.cs`: source versions 0x8000–0x8003 and 12-byte instructions.
- `TSOClient/tso.simantics/Engine/VMThread.cs::MoveToInstruction`: branch bytes 253/254/255.
- `TSOClient/tso.files/Formats/IFF/Chunks/STR.cs`: format 0 and -1 through -4, explicit language sets, comments and encodings.
- `TSOClient/tso.files/Formats/OTF/OTFFile.cs`: signed table/key identity and external tuning constants.
- `TSOClient/tso.files/Formats/IFF/Chunks/BCON.cs`: flags and unsigned 16-bit constants.
- `TSOClient/tso.files/Formats/IFF/Chunks/SLOT.cs`: chronological slot order, version-specific fields and offset floats.
- `TSOClient/tso.files/Formats/IFF/Chunks/PALT.cs`: palette version/count/reserved layout and RGB channel order. Creator retains the imported version/reserved bytes using the preserving Rust encoder.
- `TSOClient/tso.client/Rendering/City/CityMapData.cs`: map layer channels, exact terrain palette and 512×512 save shape.
- `TSOClient/tso.client/Rendering/City/CityFoliage.cs`: exact forest palette.
- `TSOClient/tso.client/Rendering/City/Plugins/MapPainterPlugin.cs` and `Documentation/Crafting a City.md`: original city tools and their map outputs.
- `TSOClient/FSO.IDE/ResourceBrowser/ResourceEditors/UnknownResourceControl.cs` and `OTFResourceControl.cs`: opaque resource UI and OTF display scope.
- `Other/tools/FarExtractor/FarExtractor/Form1.cs`, `Other/tools/XaToWav/XaToWav/Program.cs`, and `Other/tools/Mr. Shipper/Mr. Shipper/Program.cs`: archive, audio converter and asset shipping census.

`docs/compat/tools.json` inventories all immediate `Other/tools` projects and Volcanic/City Painter. It records partial foundations separately from unsupported graphical/runtime behavior. This package does not claim completed W16 parity, effective content-catalog identity/rights resolution, upgrades authoring, patch-chain authoring, graphical previews, native tool compatibility, live VM stepping, spritesheet or mesh authoring, neighborhood/server edits, or original-runtime differential verification.
