# Content and isolated runtime bridge

`wonderland-content-runtime-bridge` connects Swarm B's resolved content to the
actual Swarm A `sim-core` library. It imports a defined subset of source resources,
runs isolated behavior queries, restores validated snapshots, reads typed watches,
replays whole accepted ticks, projects real interaction facts and active queue
users, and executes a proved read-only check subset. No substitute interpreter is
used.

The simulation source is pinned to
`8a0e251d19e222a0a6833d7408ca629f674e1729`. The original FreeSO source used for the
resource mappings and execution fixture is the repository baseline
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. The bridge does not edit or vendor
Swarm A's tracked source. Its sibling `sim-core` path dependency matches the
eventual integrated repository; until that sibling is merged, the assembly runner
below supplies the exact Git-pinned subtree in a separate scratch directory.

The temporary `interaction_rules` dependency is the existing
`wonderland-interactions-check` package at `tools/swarm-b-check/interactions`.
Its library path points directly to the authoritative B-owned
`crates/sim-core/src/interactions/mod.rs`. The assembly links that package as one
explicit allowed B source path; it does not copy B code into the pinned A export.
Swarm F can replace this package boundary when composing the two owned modules.

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

`watch(StateWatch)` reads typed attributes, ObjectData, temps, extended temps,
locals or arguments at a specified stored frame depth, person data, motives,
globals, or the thread stop value. Generation and register-bank bounds are
checked. Depth zero is the oldest stored frame; the final depth is the current
frame. Person data, motive and global watches use A's actual memory reader,
including source-computed fields such as global clock values.
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
4096 watches. Fields use `attribute/N`, `object-data/N`, `temp/N`, `temp-xl/N`,
`local/DEPTH/N`, `arg/DEPTH/N`, `person-data/N`, `motive/N`, `global/N`, or `stop`,
with at most 17 ASCII bytes and canonical unsigned decimal u16 depth/indexes.
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

The current `tools/creator-web` application binds the real Creator IFF and sprite
codecs. It does **not** bind `SimDebugProvider` or hold a live `SimRuntime`. The
standalone B checkout also has no A root crate manifest; composing that graphical
runtime dependency requires the pinned A assembly or the eventual integrated
tree. The provider and inspection APIs are ready for such a host to supply an
authenticated runtime, actual immutable content, snapshots and accepted inputs.
An uploaded JSON report can be displayed as a report; it does not establish a
live VM or instruction debugger. Graphical runtime selection, watch controls,
accepted-tick playback and the entity inspection consumer remain unwired. Real
instruction break/step/trace additionally needs the A operation documented below.

### Bounded stored-state inspection

`inspection::inspect_entity_json(&SimRuntime, EntityRef, max_bytes)` emits the
`wonderland.runtime-entity-inspection.v1` schema after checking the live entity
generation. Its borrowed view includes lot/epoch/tick/content identity, object
memory and containment, actual active queue users and advertisements, stored
frame context/locals/arguments, registers, action strings, diagnostics and actual
route continuations. Advertisement maps use ordered records so tuple keys remain
valid JSON. Route entries carry the real continuation/route IDs, phase, target,
position, callback token and whether completion resumes a VM frame.
`slot_state` contains the actual physical slot definitions and live reservation
and occupant records involving the entity. An owner view includes all users of
its slots; an actor view includes that actor's records on other owners' slots.

The browser wire types are explicit:

| Values | JSON representation |
|---|---|
| Every u64, including nested continuation/request IDs, lot, epoch, tick, revision, reservation operation/sequence and expiry | Canonical decimal **string**, including zero |
| u32 GUID/generation, u16 indices, i16 registers, i32 extended temps and smaller integers | JSON number |
| Optional state | `null` when absent; an empty collection remains an empty array |
| Enum state | A's serde enum shape; nested u64 payloads receive the same string conversion |

A borrowing serializer applies the u64 rule recursively without constructing an
intermediate JSON value tree. Native `usize` values serialized through u64 (for
example a fault's bank length) follow that string rule as well. Clients can use
`BigInt(decimalString)` for arithmetic; they must not coerce identifiers through
JavaScript `Number`. Regressions cover `9007199254740993`, `18446744073709551615`,
actual reservation tokens, nested waiting IDs and exact one-byte output limits.

The complete JSON report has a caller limit capped at 1 MiB. A nonallocating
serialization pass admits all bytes, including escaped fault and action-string
text, before a fixed-capacity output vector is allocated. An undersized limit
fails instead of truncating. The caller supplies its authenticated debugging or
preview capability; this pure API does not grant access or include private EOD
host state. Stored positions remain distinct from executed instruction history.

## Interaction content, query and queue adapters

`interactions::RuntimeCatalog::from_imported` accepts trusted in-process import
reports and explicitly supplied global interaction bindings. It checks actual A
routine namespaces, object/code-owner identity, duplicate TTA indices, label and
definition bounds, then binds the catalog to A's actual content/tuning descriptor.
Global rows belong to their `code_owner` target GUID: the same raw global TTAB
must be resolved for each target owner that uses it. One object's private call
context is never applied to another object. An empty global input explicitly
selects no global entries. Absent and empty local tables remain distinct.

`RuntimeInteractionWorld` implements B's real `WorldProvider` against a borrowed,
validated A runtime and catalog. It projects live generations and revisions,
actual avatar species/age/permissions/carrying/ghost facts, source TS1 visitor
state, disabled flags, the multipart base object's Broken flag, hidden and
out-of-world state, the target's raw Occupied flag, RNG and registers. The service owner supplies
an `InteractionAuthority` implementation for authenticated principal operations
and TSO ownership, including the donated-object mayor rewrite. Missing ownership
fails the TSO snapshot instead of treating an unknown owner as a permission.

The integrating authority also supplies a monotonically advanced nonzero world
revision. It must change for every query-visible state or access-policy update,
including changes between ticks. A borrowed runtime/authority view remains fixed
during a query. The adapter checks the fully serialized snapshot size before
capture; its provider-state budget is capped at 8 MiB and preserves actual A
snapshot validation. UI offer queries use a detached copy and cannot mutate the
live simulation.

`ReadOnlyChecks` implements `CheckTreeProvider` for a statically certified subset.
It traverses **every instruction** in the complete direct-call closure, including
currently unreachable branches, with a cap of 512 routines and 131,072
instructions. It admits Expression comparison operators 0, 1, 2, 8, 14, 15 and 16,
Test Object Type, and resolved direct calls. Writes, random draws, advertisements,
action-string mutations, indirect calls, unknown primitives and unresolved
dependencies fail before execution. A's real interpreter then runs the proved
check with the actual caller/callee/stack-object/code-owner context, four source
arguments and the shared query instruction allowance. Return values and actual
instruction counts determine B's offers; empty mutation output follows from the
proof, rather than assumed defaults. This subset works for UI and tick-owned
queries because it cannot alter the omitted world/RNG/output state.

The root check routine is resolved using its declared binding owner; the check
frame retains the action's CodeOwner, and every nested direct call resolves in
that action context. They can differ in the public WorldProvider contract, so the
proof and interpreter preserve both identities. RuntimeCatalog checks the actual
source target/action binding; another WorldProvider must establish its own
authoritative effective-content bindings before supplying definitions.

For intent validation, the adapter maps B's detached `target_occupied = false`
onto only the target's ObjectData word 8, bit 5, matching
`VMNetInteractionCmd.Verify:48–51`. Other flags, queued users, UseCount and other
avatars' using frames remain intact. Ordinary UI checks carry the captured raw
flag. All changes stay in the detached candidate; validation does not clear the
live object's flag or advance a tick.

`project_active_queues` reads the complete authoritative B `ActionQueue` set. It
uses only each queue's protected active prefix, including suspended parents;
future entries do not count. It checks live actor/target generations, avatar
ownership and matching dialect, rejects duplicate queue owners, and deduplicates
multiple active uses by one avatar. Missing queues clear prior usage projections.
The resulting actual A `SetInteractionProjection` commands preserve existing
advertisements and make source ObjectData UseCount reads see the queue users.

Preparation is bounded to 8 MiB, 32,767 queues and 131,072 active entries, as well
as A's accepted command limit. `PreparedProjection::into_tick` rejects any change
to the canonical runtime state since preparation and places projection commands
before the supplied trusted tail. A's real `step` validates and commits the whole
transaction. Queue objects and command tails are authoritative integration inputs;
the adapter does not deserialize client claims or authenticate them itself.
The scheduler prepares this projection from the complete queue state for that
accepted tick. Its runtime-state binding cannot detect a queue-only mutation;
the scheduler must preserve that queue revision through acceptance and associate
later queue changes with a subsequent preparation.

### Interfaces still awaiting integration

The full mutable check operation remains unavailable: A's public query result
omits the candidate state, RNG, action strings and advertisement dictionary.
The queue projection also does not push or complete real action frames. A's
public StartBehavior command cannot preserve the required parent action frame,
ActionTree flag and special-result semantics. Break/step/trace needs an actual
instruction-suspension capability in the core. The exact checked anchors and
minimal extension contracts are recorded in
[runtime-extension-contract.md](runtime-extension-contract.md).

Complete object paths additionally need their actual global/semi resources,
normalized routes/slots, multipart topology, animation content and service
providers. Existing real routing, slot, memory and tick APIs remain usable;
their existence does not establish that missing source dependencies have been
supplied or that queue execution has been integrated.

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

### Chair, bed and appliance source-family execution

The `source-families` example evaluates every representable private BHAV in the
three selected original IFFs. The algorithm is identical for all objects: both
object-self and avatar-to-object contexts, in both actual VM dialects, an isolated
synchronous query, and one real accepted StartBehavior tick restored from a
snapshot. It retains source routine hashes, declarations, unsupported imports,
unresolved call sites, query results, real stored frames, memory, events and final
hashes. Each accepted/rejected transaction is compared between authority and
replica, including failure atomicity and snapshot round trips.

| Family | Original IFF | Selected OBJD / GUID |
|---|---|---|
| Chair | `Chair_fso_Bouncy_Beach_Ball.iff` | 16807 / `0462db31` |
| Bed | `k8capbedts.iff` | 16831 / `f8ea4345` |
| Appliance | `k8oblfridgehd.iff` | 16809 / `46ea4345` |

The executable verifies the exact pinned file SHA-256 before decoding. The
fixture supplies one source object and one authored avatar in an empty lot,
zeroed source-sized attributes, private BCON/STR resources, default geometry and
an authored avatar that explicitly permits floor/terrain movement, plus neutral
TSO motive tuning. It disables lifecycle/autonomy setup and
does not fill in absent globals, semiglobals, animation metadata, routing-slot
normalization or multipart topology. This is a diagnostic execution harness,
separate from the strict content importer, which still rejects incomplete
effective scopes. The report keeps `complete_gameplay: false` for every family.

Focused source assertions cover the chair's original Room Impact helper, the
bed's original sleep-begin/sleep-end motive writes, the appliance's original
random-bias check and exact RNG progression, and the missing global 280 reached
from each object's main routine. The complete report exposes failures beyond
those focused cases rather than replacing the unresolved behavior.

```sh
python3 tools/swarm-b/runtime-bridge.py --cargo /root/.cargo/bin/cargo \
  /tmp/wonderland-runtime-assembly run --locked --offline --no-default-features \
  --example source-families -- /absolute/path/to/wonderland

python3 tools/swarm-b/runtime-source-families-parity.py \
  --assembly /tmp/wonderland-runtime-assembly \
  --cargo /root/.cargo/bin/cargo \
  --report /tmp/source-family-execution.json \
  --inventory /tmp/source-family-outcomes.json
```

The family parity driver verifies all six source/dialect rows, all original
private routine/context pairs, literal helper outcomes, the unresolved main
call, query isolation, failure atomicity, final snapshots and exact native/WASI
JSON equality. It rejects changed source identity and accepted-state hashes.
The optional full report is written only after the comparison succeeds; stdout
contains a smaller summary and the full report's SHA-256.

The verified native/WASI run contains 144 distinct original private routines and
576 routine/context/dialect cases. Its 1,318,573-byte complete report has SHA-256
`fe480079aaffc1c167feb0ae5438e324b9bdebd3ac51ed4e101141b421b3e2be`.
The checked [source-family results](runtime-source-family-results.json) retain
each source/routine hash, all unresolved call sites and primitive requirements,
every query/accepted outcome, instruction counts and accepted state hashes.
The optional executable report adds the full stored memory, frames and events.

| Family / dialect | Cases | Return true | Return false | Completed error | Faulted | Waiting | Sleeping |
|---|---:|---:|---:|---:|---:|---:|---:|
| Chair / TS1 | 42 | 8 | 8 | 20 | 4 | 2 | 0 |
| Chair / TSO | 42 | 8 | 8 | 20 | 4 | 2 | 0 |
| Bed / TS1 | 144 | 40 | 11 | 49 | 40 | 2 | 2 |
| Bed / TSO | 144 | 40 | 11 | 49 | 40 | 2 | 2 |
| Appliance / TS1 | 102 | 26 | 20 | 26 | 15 | 15 | 0 |
| Appliance / TSO | 102 | 26 | 18 | 24 | 7 | 27 | 0 |

These are actual end-of-tick stop states. Return true is a routine result within
the explicitly incomplete harness. Completed errors, missing entities,
avatar-only host faults and unfinished requests remain visible. The appliance's
STR# 300 is rejected as source format 512; its raw bytes and SHA-256 remain in the
original IFF and its rejection is recorded, with no invented empty string table.
Every object's main routine reaches the absent global 280. The static unresolved
call-site counts are 23 for the chair, 28 for the bed and 37 for the appliance.

These tick observations do not certify a complete select/walk/reserve/use/animate/
needs/cancel gameplay path. The unresolved source and runtime contracts remain
visible in the report and extension document.

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
