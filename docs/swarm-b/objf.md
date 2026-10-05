# OBJf function tables, cooking and object dependencies

The format dispatcher now recognizes the source `OBJf` function table as a
semantic resource. This lets the existing asset cooker validate and pack it
with `ResourceKind::Semantic`, `ResourceCodec::IffChunk` and an explicit
simulation-critical override. The manifest and pack schemas are unchanged.

## Source contract

The reference is [OBJf.cs](../../TSOClient/tso.files/Formats/IFF/Chunks/OBJf.cs).
Its supported little-endian layout is:

| Offset | Field |
| --- | --- |
| 0 | u32 padding |
| 4 | u32 version |
| 8 | four-byte `fJBO` tag |
| 12 | u32 function count |
| 16 onward | one u16 condition followed by one u16 action per function |
| after the table | retained trailing bytes |

`semantic::{decode_objf, encode_objf}` exposes `Objf { padding, version,
functions, trailing }` and `ObjfFunction { condition, action }`. Any source
version/padding words are preserved. Unknown tails survive an unchanged or
targeted edit. The original writer normally emits zero padding and omits tails;
this tool codec preserves them intentionally for lossless authoring.

The original reader reads but does not check its magic field. The Rust codec
admits only the named `fJBO` layout; other tags fail explicitly. It verifies
input/resource/decoded limits, the declared entry count and the entire entry
range before allocating. Encoding admits the complete output size before
allocating a fixed-capacity buffer. Semantic retained-memory accounting includes
actual function-vector and trailing-buffer capacities.

The source runtime's representable function table may be narrower than the
format API. The actual-runtime bridge retains its own function-count and exact
payload requirements. Successful format decoding alone does not establish that
a runtime can execute an object.

## Lifecycle selection in the catalog

[VMEntity.cs](../../TSOClient/tso.simantics/Entities/VMEntity.cs) selects OBJD
lifecycle fields when `UsesFnTable == 0`. Otherwise it looks up the OBJf whose
chunk ID equals the OBJD chunk ID. The updated census records every decoded
function table and applies this selection when expanding static dependencies.
It includes condition and action references and follows encoded private calls.
Missing/unsupported or duplicate selected tables remain explicit unresolved
gates; unused OBJD fields cannot silently substitute for them.

Source aliases must have identical function metadata. Both table count and
aggregate function entries are bounded. Per-object selection results live in
each cohort's `object_dependency_index`; full source table metadata is shared
once in that cohort. The chair, bed and appliance descriptors use the same data.

The pinned checked-in census contains **471 decoded OBJf tables**. There are
**456 leaves with OBJf references**, **1,315 leaves with decoded primitive
dependencies**, **568 with route-handler dependencies**, **80 with missing or
unsupported private references**, and **1,339 with global or semiglobal
references**. Its denominator is unchanged: 745 IFF/PIFF paths, 730 distinct
source hashes, 1,327 OBJD rows and 1,602 total leaves.

These are structural upper bounds. Source overrides, condition evaluation,
dead branches, patch application and invocation-specific behavior can change
what executes. No object gameplay status is promoted by this census update.

## Verification

Five focused Rust tests cover semantic dispatch, independently specified
condition/action bytes, arbitrary source header words, unknown tails, targeted
editing, empty tables, bad tags, truncation, impossible counts and exact/tight
input/allocation/output limits. The cooker acceptance test imports an authored
IFF, declares its OBJf dependency, creates repeatable critical packs, plans the
four-member semantic closure, verifies pack membership and reopens the exact
OBJf bytes. A malformed declared table is rejected before cooking.

Six catalog regressions cover source table selection, refusal to substitute
missing/ambiguous tables, bounded table/entry counts and source-alias consistency.
They also require malformed duplicates to remain ambiguous, retain exact source
ordinals, and reject alias disagreement about raw function-table identities.
The complete current verification is recorded in the buildout handoff.

```sh
cargo test --locked --manifest-path crates/legacy-formats/Cargo.toml --test objf
cargo test --locked --manifest-path tools/asset-cooker/Cargo.toml --test objf
python3 -m unittest discover -s tests/compat/catalog -p 'test_object_census.py'
```
