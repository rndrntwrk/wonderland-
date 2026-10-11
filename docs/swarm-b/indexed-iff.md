# Indexed IFF editing

`wonderland-legacy-formats` can rebuild a validated version 0 or version 1
`rsmp` resource map when its source document is available. The writer preserves
unrelated chunk bytes and updates the file-header pointer, entry offsets,
counts, IDs, flags, labels and recognized size fields. Untouched indexed files
retain their original bytes, including opaque or unsupported map metadata.

The pinned source corpus establishes **192 editable indexed originals and 195
strict byte-exact passthrough originals**. It contains no original version 1
maps. Version 1 support has source-derived fixtures and independent historical
reader evidence; it does not claim a version 1 original-asset census.

## API and transaction contract

Use `iff::IffDocument::decode`, edit `file_mut()`, and call `encode`. A caller
that maintains its own document representation can use:

```rust
iff::encode_rebuilding_index(
    original_bytes: &[u8],
    edited: &iff::IffFile,
    limits: &Limits,
) -> Result<Vec<u8>>
```

The function validates the map against **original** chunk boundaries and
metadata before computing the edited layout. Checking a stale map against
the edited offsets would reject valid growth and could accept an unrelated
pointer by coincidence, so the original bytes are required.

The header and complete `rsmp` chunk are writer-managed. Callers retain their
source bytes until encoding succeeds; relocating the map in the chunk list is
allowed. After a successful transaction, decode the returned bytes to establish
the baseline for another transaction. The creator follows this sequence and
publishes its new bytes and parsed document together.

Ordinary `iff::encode(&IffFile, &Limits)` continues to reject any resource map or
nonzero map pointer because an isolated edited `IffFile` cannot establish the
original offset evidence. Adding or removing an index, changing its metadata
directly, or using an orphan map with a zero header pointer is unsupported.

## Original format evidence

The decisive reference is
`Other/tools/Iffinator/Iffinator/srcs.zip`. Its `iff.cpp` validator and
`mk_iff.cpp` writer establish the following layout:

| Field | Version 0 | Version 1 |
|---|---|---|
| File header +60 | Big-endian `u32` offset to the map chunk header | Same |
| Map body +0 | Little-endian reserved `u32`, zero | Same |
| Map body +4 | Little-endian version `u32`, zero | One |
| Map body +8 | Four literal bytes `pmsr` | Same |
| Map body +12 | Size-like `u32`, little endian | Same |
| Map body +16 | Type-group count, little-endian `u32` | Same |
| Type-group header | Reversed FourCC, then little-endian entry count | Same |
| Entry offset | Little-endian `u32` offset to a resource chunk header | Same |
| Entry ID | Little-endian `u16` | Little-endian `u32`, high word must be zero |
| Entry flags | Little-endian `u16` | Same |
| Entry label | Raw NUL-terminated bytes, padded to an even map-body offset | One-byte count followed by that many raw bytes, no padding |

Source anchors inside the archive:

- `iff.cpp:98–111`: label traversal for both versions.
- `iff.cpp:124–141`: reserved field, version, magic and ignored size-like field.
- `iff.cpp:147–164`: reversed FourCC and resource offset interpretation.
- `iff.cpp:174–202`: ID, flags and label consistency; version 1 high-ID rejection.
- `mk_iff.cpp:374–406`: version 0 map writer; zero internal size, NUL labels,
  zero alignment padding.
- `mk_iff.cpp:410–467`: normal chunk envelopes, IFF-relative offsets, file-header
  map pointer, and separate map chunk without a self-entry.

The old C# reader in
`Other/tools/SimsLib/SimsLib/IFF/Old/Iff.cs:220–308` corroborates the fields.
Its version 0 label helper is incomplete, so it is not used to establish
padding. The modern `TSOClient/tso.files/Formats/IFF/IffFile.cs:156–169,244–282`
reads but ignores the map pointer and writes a zero pointer with a TODO. It
does not provide a resource-map rebuild implementation.

The source C++ writer emits zero padding. All 8,948 padding bytes in the
indexed corpus are instead `0xA3`. The new writer preserves an existing
entry's padding byte when applicable, and emits source-supported zero padding
for a new alignment byte. Labels remain bytes and can use the entire 64-byte
chunk-label field; no UTF-8 conversion is imposed.

## Accepted metadata and preserved bytes

An edited indexed document must contain one source map selected by a nonzero,
correct original pointer. The map must index each non-map chunk exactly once,
with matching original type, ID, flags and visible label. Duplicate groups,
duplicate entries, missing coverage, self-entries, trailing bytes, bad offsets,
unsupported versions, nonzero reserved fields and nonzero version 1 high ID
words fail explicitly. Counted version 1 labels containing extra NUL/tail bytes
are outside this strict subset.

The original C++ reader ignores the size-like field. The corpus has three
consistent conventions, which the writer recognizes and preserves:

- Zero, as emitted by the historical writer.
- Whole chunk byte count: map payload length plus 76.
- Bytes following the 12-byte reserved/version/signature prefix: map payload
  length minus 12.

Other nonzero values are ambiguous and prevent edits. A zero field remains
zero; the two length conventions are recalculated when the map changes.

Surviving map type and entry order is retained. New entries follow surviving
entries of the same type; newly introduced types follow existing types.
Deleted entries and empty groups are removed. The non-map chunks retain the
caller's requested order, flags, labels and payloads. Moving the map itself is
supported, including placing it before resources whose offsets depend on the
rebuilt map size.

## Bounds

Source and edited envelopes are checked under the existing `Limits`. Map type
and entry counts are checked against both `max_entries` and the actual original
resource count before map-derived allocations. Minimum encoded record lengths,
input boundaries and string limits are checked before iteration or label reads.
All final chunk sizes and indexed offsets must fit their `u32` fields.

The source-aware encoder performs an allocation-free envelope scan before
cloning the original or allocating its new map structures. Its conservative
workspace plan is also bounded by `max_total_decoded_bytes`:

```text
source byte length
+ upper bound on encoded output bytes
+ upper bound on rebuilt map bytes
+ 1024 × (source chunk count + edited chunk count)
```

The collection allowance covers overlapping source-chunk copies, BTree nodes,
Vec capacity, type groups, entries, lookup tables, inclusion markers and
offsets on the supported 32- and 64-bit targets. The map upper bound allows a
separate type group for every resource and the larger supported label record
layout. This deliberately favors bounded rejection over allocator-specific
memory estimates. A regression test uses 100 tiny resources: its payload fits
a 32 KiB decoded limit, exact passthrough succeeds, and rebuilding is rejected
because the workspace does not fit.

The provided old map payload and the rebuilt payload both must satisfy
resource limits. A sufficiently tight limit can therefore reject a shrinking
edit even when its eventual output would be smaller. Exact passthrough through
`IffDocument` retains the existing limits behavior.

## Corpus result and explicit exclusions

The read-only source audit found 198 indexed originals, all using version 0.
Every map has one correct file-header pointer, no self-entry, and no trailing
map bytes. The strict writer dispositions are:

| Disposition | Files | Evidence |
|---|---:|---|
| Valid strict envelope; exact unchanged output | 195 | Complete byte comparison |
| Supported indexed edits | 192 | Five edits each, 960 outputs checked |
| Ambiguous map metadata; edits rejected | 3 | Listed below |
| Duplicate `XXXX` chunk identities; strict decode rejected | 3 | Existing strict errors retained |

The 192 supported files contain 13,084 indexed resources and 60,411,187 source
bytes. Their size conventions comprise 94 payload-minus-12 maps, 88 whole-chunk
maps and 10 zero fields.

The three strict but non-editable maps are:

| File | Reason |
|---|---|
| `k8vteplantts.iff` | Internal size is 955 for an 880-byte payload; neither recognized nonzero convention matches |
| `k8tqbbqts.iff` | Map label `Grill` differs from chunk label `Grill ` |
| `k8capmirrorsv.iff` | Map label `init wall` differs from chunk label `init wall ` |

The three duplicate-key originals are `stp_wfloveseat.iff`, `stp_wfchair.iff`
and `stp_wfplant.iff`. Their maps also have inconsistent size fields and omit
`XXXX` tombstones. This work does not reinterpret or repair those envelopes.

## Reproducing verification

Focused tests run without original assets:

```sh
cargo test --locked --manifest-path crates/legacy-formats/Cargo.toml --test indexed_iff
```

The 20 focused tests cover both versions, exact no-op and same-size output,
growth, insertion, deletion, reordering, empty maps, full-width raw labels,
rekeys and flags, repeated edits, size conventions, hostile counts, every
truncated map prefix, duplicate/incomplete maps, stale metadata, transitions
without a source index, and resource/string/workspace limits.

The full source-backed gate requires the original checkout, Cargo/Rust 1.90.0,
Python 3, and `g++`:

```sh
python3 tools/swarm-b/indexed-iff-oracle.py
```

Optional `--cargo`, `--cxx`, `--target-dir` and `--logs-dir` arguments select
tools and disposable build/output locations. The script extracts the pinned
historical reader, independently creates a version 1 fixture from the pinned
39-entry version 0 original, explicitly runs both otherwise-ignored corpus
tests, and validates each emitted output with the historical C++ validator.
The five edits are a same-size payload change, payload growth, insertion,
deletion, and reversal of the complete chunk list including the map.

The gate passed **965/965 emitted outputs**: 960 original version 0 edits and
five source-derived version 1 edits, totaling 302,174,293 bytes. Corpus inputs
and outputs have 16 MiB per-file limits; combined outputs are capped at
512 MiB. The historical reader runs only after bounded Rust checks, in batches
of 64 files with a 30-second subprocess timeout. The script saves logs and a
metadata-only `indexed-iff-oracle.json`; temporary original-content outputs
are removed at exit.

### Historical-reader qualification

The archived source lacks its matching `iff.h`. The oracle supplies declaration
and trivial accessor definitions, including the source's internal label
accessor. It changes only the obsolete allocation spelling
`new (Entry *)[m_entries]` to `new Entry*[m_entries]`. The parsing and validation
bodies remain the archived implementation. The script pins:

- Archive SHA-256: `255ede7a282284ea466edcaa9017a49c7239b4e163ac16d9434e8e16663c9866`.
- Original reader SHA-256: `bb21fd4a34c2c5951eec54c6aa977fde55f10496a975fb0a1a4f5bb1ef5e79e9`.
- Version 0 fixture SHA-256: `5b85d2cd824a88223c458eb80e564717c1a6ee4b080cefa4dcec695bd65e93a9`.

The historical validator intentionally accepts inconsistent size fields and
incomplete coverage. Its output-layout compatibility check supplements the
strict Rust validation; it does not establish hostile-input safety or full
gameplay/asset compatibility. Existing content-census metadata records the
earlier envelope/passthrough capability snapshot; this gate records the new
indexed-edit evidence separately.
