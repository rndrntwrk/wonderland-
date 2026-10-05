# Content and isolated runtime bridge

`wonderland-content-runtime-bridge` connects Swarm B's resolved content to the
actual Swarm A `sim-core` library. It imports a defined subset of source resources,
runs isolated behavior queries, restores validated snapshots, reads typed watches,
and replays whole accepted ticks. No substitute interpreter or runtime provider is
used.

The simulation source is pinned to
`8a0e251d19e222a0a6833d7408ca629f674e1729`. The original FreeSO source used for the
resource mappings and execution fixture is the repository baseline
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. The bridge does not edit or vendor
Swarm A's tracked source. Its sibling `sim-core` path dependency matches the
eventual integrated repository; until that sibling is merged, the assembly runner
below supplies the exact Git-pinned subtree in a separate scratch directory.

## Build and reproduce

Use Rust 1.90 and a dedicated assembly directory **outside** the source repository.
The pinned simulation commit must already exist in the local Git object database.
The script uses `git archive` for `crates/sim-core`; it does not clone repository
submodules or require a network Cargo Git dependency.

From the repository root:

```sh
python3 tools/swarm-b/runtime-bridge.py --cargo /root/.cargo/bin/cargo \
  /tmp/wonderland-runtime-assembly test --locked --offline

python3 tools/swarm-b/runtime-bridge.py --cargo /root/.cargo/bin/cargo \
  /tmp/wonderland-runtime-assembly clippy --locked --offline --all-targets -- -D warnings

python3 tools/swarm-b/runtime-bridge.py --cargo /root/.cargo/bin/cargo \
  /tmp/wonderland-runtime-assembly fmt --check

python3 tools/swarm-b/runtime-bridge.py --cargo /root/.cargo/bin/cargo \
  /tmp/wonderland-runtime-assembly check --locked --offline \
  --target wasm32-unknown-unknown --lib --no-default-features
```

Replace the example Cargo path with the installed Rust 1.90 executable. The runner
prepends an explicit Cargo executable's directory to `PATH`, disables incremental
compilation and debug information, and defaults to two build jobs and the
assembly's own `target` directory. Registry dependencies must be cached for
`--offline`; `Cargo.lock` pins their versions.

The assembly stamp records the simulation revision, source repository, and every
exported simulation file hash. Reuse verifies the complete simulation subtree,
including the absence of extra files such as an injected `build.rs`. Before any
refresh, it checks the owned directories, stamp, manifests, lockfile, and exported
files. Symlinks in those paths and hard-linked owned files are rejected. The only
allowed source symlinks are the explicitly declared B crate, test, and corpus
links. This is a stable-directory preflight and leaf-write protection contract;
it is not a filesystem sandbox against concurrent directory replacement.

The runner's path regressions are executable independently:

```sh
python3 -m unittest discover -s tests/integration/swarm_b_runtime -p test_assembly.py -v
```

## Public API and conversion contract

The library reexports `sim_core`, so callers use the same runtime types as the
adapter. Its central conversion entry point is:

```rust
content::import_content(
    objects: &[ObjectImport<'_>],
    options: ImportOptions,
    limits: &wonderland_legacy_formats::Limits,
) -> Result<ImportedContent, String>
```

Each `ObjectImport` selects one OBJD chunk from an effective `ResolvedContent`, an
explicit semiglobal namespace owner where needed, and `RuntimeMetadata` supplied
by the integrating content owner. `ImportOptions` selects a locale, explicit TTAB
dialect, animation metadata, and non-BCON runtime tuning. The result contains the
validated immutable A `ContentSet` and per-object interaction/provenance reports.

| Input | Runtime mapping and validation |
|---|---|
| BHAV | Instruction opcodes, branch bytes, all eight operand bytes, locals, arguments, routine kind, format version and tree version enter A's real `VmRoutine`. A validates its own instruction/branch bounds. IDs below 4096 are global, 4096–8191 private, and 8192+ semiglobal. No fallback to a different namespace occurs on a miss. |
| OBJD | Source GUID, attribute count, animation and body string IDs, fixed raw definition words, generated function entries and level offset are converted. Attribute storage also accommodates the selected private STR# 256 labels, as in the source constructor. The source's ChairFacing bit initializes ObjectData word 8. |
| Runtime definition memory | `Version1` and `Version2` occupy indices 0 and 1; source `RawData` follows at index 2. Fixed source raw lengths are 78 words for 136, 93 for 138, 94 for 139, 95 for 140/141, and 103 for 142. Extra words preserved by round-trip tooling remain in the resolved IFF but are excluded from runtime reads. Version 136 therefore cannot acquire a repair entry from an appended word. |
| Multipart OBJD | A subobject requires its explicit master GUID. The master must be in the same effective resource with matching MasterID and SubIndex −1. Only −1 is the master sentinel; other packed signed offsets, including `0xFF00`, remain subobjects. Its runtime definition receives the same version-prefix mapping. A missing child interaction table inherits the master's selected table; an existing child table is preserved. |
| OBJf | When `UsesFnTable` is set, the matching chunk is required. The bounded reader validates `fJBO`, exact length and at most 256 entries; each entry stores condition before action. Otherwise the 33 source-generated entry positions are used. Nonzero missing functions fail import. The original GUID-specific entry-17 override is retained. |
| STR# and BHAV names | Selected locale strings enter A's immutable string tables. Named-tree lookup preserves source case sensitivity, truncates names at NUL, applies ASCII replacement, and lets private names precede semiglobal names. |
| BCON/tuning | Resolved private, semiglobal and global cache values and explicit replacements become signed i16 values in the corresponding runtime owner/table/index namespace. Supplied non-BCON tuning remains explicit. Conflicting shared values fail import. |
| TTAB/TTAs | A local table takes precedence over its semiglobal counterpart. The explicit dialect decodes the authoritative bytes; a changed private semantic cache is rejected. Each report retains TTA index, resolved action/check routine, code owner, flags, permissions, and optional localized label. Missing tables remain distinct from present empty tables. Duplicate TTA indices and unresolved functions fail import. |
| SLOT | Private type-0 records determine containment slot count. Normalized per-object routing slots are supplied explicitly, validated by A, and installed under that object's GUID. This API does not expose global routing-slot installation or choose float-to-integer conversion rules. |
| Geometry and animations | Footprint, placement rules, family, normalized routing slots and animation metadata come from explicit integration inputs. Their A validators run; filenames do not imply geometry, animation availability or gameplay behavior. |

The converter verifies the effective private resource identities and effective
global/semiglobal hashes before use. One runtime global or semiglobal namespace
cannot combine unrelated resource hashes, even when their resource IDs are
disjoint. Direct calls reachable from every private routine and all entry and
interaction roots must exist in that object's supplied effective scopes.
Dynamic/indirect resolution and successful execution of every primitive are not
established by this static check.

`ImportedObject.effective_identity` is **resolver provenance**. It is retained
from the supplied B `ResolvedContent`; it is not a recomputed authentication token
for that public mutable structure. In particular, explicitly changing its tuning
cache can leave that provenance field unchanged. A's `ContentDescriptor`
independently binds the actual converted content and tuning used by snapshots and
runtime state. Use the A descriptor for runtime compatibility checks and retain
the B provenance for source/patch attribution. No JSON deserializer for arbitrary
`ContentSet` values is introduced here.

### Admission bounds

The converter rejects more than 4096 attributes before constructing an object.
It also keeps a cumulative conservative admission budget capped at the smaller
of the caller's `max_total_decoded_bytes` and A's 32 MiB content cap. It accounts
for output maps, vectors, definition/register storage, source-name strings,
animation properties, tuning, footprint rectangles, placement ignore sets,
normalized slots, and direct-call traversal. Legacy text reserves up to three
UTF-8 bytes per source byte before conversion and writes into one fixed-capacity
buffer, preserving legacy replacement semantics without geometric string growth.
Decoded string and interaction tables retain their allocation charges while
subsequent decoding and output conversion run. Decoder limits narrow to the
remaining allowance. Repeated shared-resource admissions are charged
conservatively.

Scoped identity hashing admits the complete IFF envelope, all chunk headers and
the encoder's duplicate-key workspace before encoding. Empty payloads still
consume their envelope and collection allowance. Before constructing A's
`ContentSet`, a nonallocating bincode size pass admits the final canonical
validation buffer and A's temporary animation-name index. A regression compares
that projected size with the real `ContentSet` serializer at the pinned revision.

This budget is not an exact process-memory measurement and can reject content
whose final serialization would fit. A still performs its separate canonical
serialized-size and structural validation. A malformed or over-budget import
returns an error and no partially constructed runtime content.

## Real isolated runtime operations

`IsolatedRuntime::capture(&SimRuntime)` validates a copy of an existing runtime;
`from_snapshot(bytes, content, SnapshotExpectation)` uses A's actual decoder and
runtime validation. Lot, epoch, content/tuning compatibility, checksum, stored
graph bounds and entity generations are checked by A. Both constructors select
`RuntimeRole::Replica`.

`query(&RoutineQuery)` resolves an explicit code owner and routine and invokes
A's `query_behavior`. The request carries generation-aware actor and target
references, arguments, and an instruction budget. Arguments are capped at 255
and the budget must be between one and the captured runtime's per-entity limit,
before request cloning or runtime invocation. The query runs against A's internal
state copy; the captured snapshot and live runtime remain unchanged. The result
is A's stop value, temporary registers, extended temporary registers, instruction
count, and diagnostics.

`watch(StateWatch)` reads typed attributes, ObjectData, temps, extended temps, or
the thread stop value. Generation and register-bank bounds are checked.
`frame_positions()` reports the **currently stored** frame depth, routine,
instruction pointer and optional opcode. Those positions do not establish which
instructions executed previously.

`advance(&AcceptedTick)` replays one complete accepted tick through A's `step`.
The runtime validates its tick/epoch and command contents. This is the resume
surface for a validated snapshot plus an accepted input tail. Replica role does
not dispatch durable effects externally.

### Creator provider

The default `creator-debug` feature supplies
`creator_debug::SimDebugProvider`, implementing the existing creator
`IsolatedDebugProvider` trait without a dependency cycle. The bridge depends on
creator; creator does not depend on the bridge.

The creator interface's u64 identity packs `generation << 16 | positive ObjectID`;
`creator_entity(EntityRef)` performs that encoding. `inspect` accepts at most
4096 watches. Fields use `attribute/N`, `object-data/N`, `temp/N`, `temp-xl/N`, or
`stop`, with at most 17 ASCII bytes and a canonical unsigned decimal u16 index.
All fields and identities are validated before snapshot restoration or result
string cloning. The wrapper's tick must equal the decoded snapshot's tick.
The provider's `MAX_INSPECTION_OUTPUT_BYTES` is 1 MiB for the complete report,
including output-vector storage, cloned field bytes and formatted-value
capacities. It counts borrowed values before materializing the report and uses
fixed-capacity formatting. Over-budget reports fail explicitly without
truncation, including repeated large fault messages.

`step_isolated` advances **one empty accepted tick** in the isolated copy.
It is not an instruction step. `trace` returns an explicit unsupported error:
the pinned A API exposes no executed-instruction history. Breakpoints and
instruction pause/resume are also unavailable. In particular, A converts its
dispatch instruction-limit exhaustion into a real fault; the bridge preserves
that fault instead of presenting it as a debugger pause.

### Interfaces still awaiting integration

The interaction report is a content sidecar, not a complete queue or check-tree
provider. A's public query result does not return all state needed by B's
`CheckTreeProvider`, such as advertisement/action-string output and the query's
mutated state. Its private runtime host also supplies no public seam for the full
B queue adapter. Neither trait is implemented with invented values. Routing,
animation, dialogs, EOD completion, authority decisions and durable service
providers still require their actual owners and integration interfaces.

## Source execution probe and portability

The executable accepts:

```sh
python3 tools/swarm-b/runtime-bridge.py --cargo /root/.cargo/bin/cargo \
  /tmp/wonderland-runtime-assembly run --locked --offline --bin runtime-bridge -- capabilities

python3 tools/swarm-b/runtime-bridge.py --cargo /root/.cargo/bin/cargo \
  /tmp/wonderland-runtime-assembly run --locked --offline --bin runtime-bridge -- \
  source-replay TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff
```

General conversion, watches and accepted-tail replay are library APIs. The CLI
provides a reproducible source probe rather than a general content-file codec.
It validates SHA-256
`20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865`, then executes the
unmodified BHAV 4110, **FT - prepare for sale**, with the real interpreter. Its
three original assignment instructions set attributes 3, 0 and 1 to zero.

The four-attribute owner, initial `[10, 20, 30, 40]` values, empty 8×8 TS1 lot,
lot 11/epoch 7, and accepted spawn/start commands are an explicitly authored test
harness. The probe does not claim complete casino-bar behavior. It emits summary
JSON and hashes, never original asset payloads.

Native/WASI comparison uses the same source file via a WASI source-directory
preopen and the same compiled A/B implementation. It requires the
`wasm32-wasip1` target and a Node version with preview-1 WASI support (verified
with Node 24):

```sh
python3 tools/swarm-b/runtime-bridge-parity.py \
  --assembly /tmp/wonderland-runtime-assembly \
  --cargo /root/.cargo/bin/cargo
```

Optional `--target-dir`, `--node` and `--skip-build` arguments select existing build
tools/artifacts. The driver verifies the exact assembly even when compilation is
skipped. Build logs go to stderr; stdout contains a small JSON result.

The comparison checks literal expected outcomes as well as native/WASI equality:

| Observation | Expected result |
|---|---|
| Isolated query | `ReturnTrue`, exactly 3 instructions, unchanged captured snapshot, 20 zero temps and 2 zero extended temps |
| Snapshot plus accepted tail | Snapshot tick 1; completed tick 2; exactly 3 instructions; attributes `[0, 0, 30, 0]` |
| Replay authority comparison | Matching state hash; zero durable dispatches; successful final snapshot round trip |
| State hash | `d52e0091425b12e7d820dcb7f1b883fb36606059b7a121bb5cf9218cc272f62b` |
| Final snapshot SHA-256 | `49094c9adc9b1e3df73264ec221c323ce0027ec231af4f3e38b2b006fe0ca43d` |
| Negative controls | Changed attribute output and changed digest both rejected |

The Rust suite additionally tests actual runtime definition reads at the source's
version/GUID/attribute indices, source version bounds, OBJf entries, multipart
interaction inheritance, scope isolation, mutable semantic-cache rejection,
admission limits, stale generations, malformed snapshots and watch requests,
and the real dispatch-limit fault. The checked-in beach-ball chair is a negative
fixture: it correctly fails when its effective semiglobal has not been supplied.

## Source anchors

- [OBJD source reader](../../TSOClient/tso.files/Formats/IFF/Chunks/OBJD.cs):
  version-specific RawData prefix and field presence.
- [Definition memory reads](../../TSOClient/tso.simantics/Engine/VMMemory.cs):
  `GetEntityDefinitionVar`, including the two version words.
- [Entity construction and function tables](../../TSOClient/tso.simantics/Entities/VMEntity.cs):
  attributes, ChairFacing, `GenerateFunctionTable`, `UseTreeTableOf`.
- [Multipart construction](../../TSOClient/tso.simantics/VMContext.cs):
  master definition assignment and tree-table inheritance.
- [OBJf source reader](../../TSOClient/tso.files/Formats/IFF/Chunks/OBJf.cs):
  `fJBO` and condition/action entry order.
- [Named-tree resolution](../../TSOClient/tso.content/WorldObjectProvider.cs):
  private-first `TreeByName` binding.
- Pinned A `crates/sim-core/src/state.rs`, `runtime.rs`, `runtime_memory.rs`,
  `snapshot.rs`, and `vm/`: immutable content, actual interpreter query/step,
  definition indexing and snapshot validation. Read them from the exact assembled
  source or `git show 8a0e251d19e222a0a6833d7408ca629f674e1729:<path>`.
