# Verified cooked releases in the real runtime

`wonderland-content-runtime-bridge` can construct the pinned A `ContentSet`
from selected cooker packs without opening an original IFF, PIFF, OTF, source
archive or import report. It uses the real runtime at
`8a0e251d19e222a0a6833d7408ca629f674e1729` and the source baseline
`4c6b3e8f5835b228723caea3c9f683c62f244f73`.

The manifest describes immutable resources. A separate, independently selected
binding describes which resources form each ordered runtime scope, which OBJD
defines each object, its semiglobal owner, its resolved tuning, and explicit
normalized runtime metadata. The final binding must contain the expected A
content and tuning descriptor. An unsealed draft cannot expose runtime content.

## Local preparation and consumption

Before A is composed into the main repository, use the existing assembly runner
described in [runtime-bridge.md](runtime-bridge.md). It exports the exact pinned
A subtree into a separate directory; no A implementation is vendored here.

```sh
python3 tools/swarm-b/runtime-bridge.py --cargo /root/.cargo/bin/cargo \
  /absolute/scratch/bridge build --locked --offline --no-default-features \
  --bin runtime-bridge --example cooked-fixture
```

The authored example creates a reproducible release using the production cooker.
It applies a real PIFF and layered tuning and removes its source files before
returning. It writes only the selected pack, although its manifest also contains
unrelated packs. The destination must be a new directory.

```sh
/absolute/scratch/bridge/target/debug/examples/cooked-fixture /absolute/scratch/release
```

A release directory uses the cooker's flat layout: `manifest.json` and
`<sha256>.wlp`. Locally authored preparation metadata can live alongside it as
`draft.json`. Prepare the final binding only after verifying all selected packs
and successfully constructing and validating the real `ContentSet`:

```sh
runtime-bridge release-prepare /absolute/scratch/release/draft.json \
  --release-dir /absolute/scratch/release \
  --output /absolute/scratch/release/binding.json
```

The command writes a new binding and returns its `binding_sha256` and manifest
digest. Existing output files are never replaced. Record that digest from this
trusted local preparation step. For a downloaded release, obtain it through the
trusted channel used to select the release; computing a digest from an arbitrary
download does not authenticate its author or metadata.

```sh
runtime-bridge release-plan /absolute/scratch/release/binding.json \
  --binding-sha256 RECORDED_BINDING_SHA256 \
  --manifest /absolute/scratch/release/manifest.json

runtime-bridge release-load /absolute/scratch/release/binding.json \
  --binding-sha256 RECORDED_BINDING_SHA256 \
  --release-dir /absolute/scratch/release

runtime-bridge release-replay /absolute/scratch/release/binding.json \
  --binding-sha256 RECORDED_BINDING_SHA256 \
  --release-dir /absolute/scratch/release \
  --scenario /absolute/scratch/release/scenario.json
```

`release-plan` reads the binding and manifest without opening pack files. Its
report names exactly the packs required for the bound dependency closure.
`release-load` verifies those packs and returns the actual runtime descriptor and
object/interaction import report. `release-replay` additionally runs the explicit
scenario described below. Unselected packs need not be present.

The CLI reads only bounded regular files, rejects observed symbolic links and
`..` traversal in input paths or output parents, and uses create-new output.
Directories must remain stable during a command; the portable path preflight
does not provide protection against concurrent hostile directory replacement.
No command sends effects or publishes files to an external service.

## Binding schema and trust boundaries

The Rust byte APIs are:

```rust,ignore
PreparedDraft::from_json(draft_bytes, manifest_bytes, &limits)?
    .seal(&selected_packs, &limits)?; // CookedRuntimeBindingV1

let prepared = PreparedRelease::from_binding(
    binding_bytes, &independently_selected_binding_sha256,
    manifest_bytes, &limits,
)?;
prepared.required_packs(); // &[PackRequest { digest, byte_len }]
prepared.plan();           // existing verified LoadPlan
let loaded = prepared.load(&selected_packs, &limits)?;
// loaded.content is the actual A ContentSet; loaded.report has cooked provenance.
```

`PackBytes` borrows a supplied digest and byte slice. It performs no filesystem
or network lookup. The caller must supply exactly the required pack set, without
duplicates or extras. All selected packs, their member hashes and complete
manifest membership are checked before semantic decoding begins.

`CookedRuntimeDraftV1` contains `schema_version: 1` and `recipe`.
`CookedRuntimeBindingV1` additionally requires `expected_content`, containing
`content_sha256` and `tuning_sha256`. `canonical_bytes` emits deterministic,
bounded JSON. Preparation computes those descriptors from verified conversion;
loading recomputes and compares them before returning content.

Every recipe field is required:

| Field | Meaning |
|---|---|
| `sim_core_revision`, `source_baseline` | Exact pinned revisions above; the manifest baseline must agree. |
| `manifest_sha256` | Digest of the actual manifest bytes. |
| `variant` | Explicit string or `null`, passed to manifest load planning. |
| `scopes` | Ordered scope records: `id`, `namespace`, `source_name`, `resources`. |
| `objects` | Object records: `guid`, `object_resource`, `private_scope`, `semiglobal`, `global_scope`, `tuning_resource`, `runtime`. |
| `options` | Explicit locale, TTAB dialect, normalized runtime tuning and animations. |

Scope namespaces are the strings `private`, `semiglobal`, or `global`. Scope
resource arrays contain manifest logical IDs in the authored encounter order.
An object's semiglobal binding is either `null` or `{ "scope": "id", "owner":
123 }`; its global binding is a scope ID or `null`. Its object resource must be
the selected OBJD member of its private scope and contain the stated GUID.

All scope members must be simulation-critical semantic resources with the
appropriate IFF codec. The producer can set explicit cooker criticality
overrides for the bound closure. UsesFnTable objects use the source-backed
semantic OBJf decoder. The runtime still admits at most 256 functions and an
exact function-table payload; preserved codec tails are not executable tables.
Unknown payloads, unapplied PIFF, duplicate chunk keys, missing dependencies,
unused scopes, cross-scope member aliases and mixed namespaces are rejected.

Resources within a scope must share complete manifest provenance. The private
scope and packed resolved-tuning record must share complete provenance; the
embedded tuning identity and shared scopes' tuning cohort must agree. The
packed tuning's semiglobal-presence flag must match the object binding, and a
nonempty global cache requires a bound global scope.
OTF, rewrite, upgrade and dynamic tuning results are read directly from the
verified packed cache; they are not recomputed from partial source inputs.

The scope identity is a hash of the explicit ordered, unindexed cooked IFF
projection. It is **not** the original whole-IFF hash or a resolver identity.
The cooker manifest does not retain original encounter order, and archive
provenance alone does not establish the grouping of extracted IFF resources.
The independently selected binding is the authority for that grouping and
order. Reordering resources can change named-tree resolution and therefore the
sealed descriptor. The unbound `import-report.json` is never consulted.

The load plan's `manifest_hash` follows the existing canonical manifest API;
the cooked report's `manifest_sha256` is the recipe-pinned digest of the actual
input bytes. These can differ for valid noncanonical JSON formatting. The A
descriptor binds actual converted runtime content and tuning, not source names
or a claimed original resolver identity.

## Explicit normalized metadata

`cooked_metadata.rs` defines every accepted field. Generate a complete template
with `RuntimeMetadataV1::from(RuntimeMetadata { ... })` and
`ImportOptionsV1::from(ImportOptions { ... })`, or inspect the authored example's
draft. JSON records accept objects only, reject unknown and duplicate fields,
and require nullable fields to be written explicitly as `null` when absent.

The metadata includes footprint rectangles, all placement flags and ignored
entity references, master GUID, family, and indexed routing/search slots. The
options include locale fallback, standard/TSBO TTAB choice, owner/table/index
tuning entries, optional TSO motive matrices, motive limits, relationship
multiplier bits, fire enablement, and animation metadata with ordered time
properties. Duplicate tuple keys, set identities or property keys are rejected;
they are never silently coalesced or overwritten. Finite `f32` bits preserve
negative zero. Animation event order is preserved even when timestamps descend.

These values are explicit author inputs. This consumer does not infer complete
collision geometry, slots, animation resources or non-BCON tuning from an OBJD.
Global routing slots are not exposed by the existing bridge conversion API.
Final validation is performed by the actual pinned `ContentSet` implementation.

## Replay scenario

`ReplayScenarioV1::from_json` accepts this complete, strict object:

```json
{
  "schema_version": 1,
  "mode": "ts1",
  "lot_id": 11,
  "authority_epoch": 7,
  "seed": 123,
  "lot_width": 8,
  "lot_height": 8,
  "lot_levels": 1,
  "object_guid": 123,
  "tile_x": 3,
  "tile_y": 3,
  "level": 1,
  "facing": 0,
  "initial_attributes": [0],
  "routine_id": 4096,
  "args": [],
  "instruction_budget": 10
}
```

Mode is `ts1` or `tso`. The object is spawned in a new empty lot on tick 1.
Tick 2 writes the explicitly supplied attribute bank through accepted memory
commands; its length must exactly equal the imported object's attribute count.
The harness restores that actual snapshot into a second Replica runtime, runs
an isolated behavior query and checks snapshot immutability. Both replicas
then process the same accepted StartBehavior tick. The harness compares their
state hashes and snapshots and requires zero effect dispatches. It reports the
actual stop status, instruction counts, temps, diagnostics and attributes.

Both runtimes have the Replica role. This local authored-state harness provides
no server authentication, accepted-command authority, external effect dispatcher,
full queue/check-tree provider, breakpoints, instruction stepping or executed
instruction trace. A fault, wait or budget stop is reported as such; a successful
report proves replay agreement for one tick, not completion of a whole behavior
or production gameplay.

## Bounds and verification

Default binding size is 1 MiB, manifest size is 8 MiB, and total selected pack
bytes are capped at 128 MiB. The caller can lower the library limits. The
conversion admission cap is the smaller of the legacy decoded-byte limit and
A's 32 MiB content limit; it cumulatively accounts for parsed metadata,
conversion workspaces and decoded output. JSON preflight reserves parser
scratch, node/key storage and decoded string/conversion workspace without
building a generic JSON value tree, and caps nesting at 32. This is a
conservative admission policy, not a claim that total process RSS is 32 MiB:
borrowed input packs, file buffers and runtime execution/snapshot allocations
are separate bounded surfaces.

Scenario input is at most 64 KiB with a 4 MiB metadata admission cap. Lots are
at most 64×64×8, facing is 0–7, arguments are capped at 255, attributes at 4096,
and the instruction budget at 1–100,000. The harness caps entities and
continuations at 256, commands per tick at 4096, and total tick instructions at
100,000. Reports are counted before allocating an exact buffer, limited to
1 MiB and rejected rather than truncated.

Run the real pack-only native/WASI check with:

```sh
python3 tools/swarm-b/runtime-bridge-cooked-parity.py \
  --assembly /absolute/scratch/bridge --cargo /root/.cargo/bin/cargo
```

The WASI host preopens only the generated cooked release directory. Both
platforms must produce the authored query result `ReturnTrue`, three instructions,
temps `[37, 41, 0, ...]`, tick-3 attribute `[41]`, and the pinned state/snapshot
digests. The comparator rejects equal but wrong outputs, a changed digest and
a corrupted selected pack. This evidence is separate from the existing
original-source probe in `runtime-bridge-parity.py`.

The integration suite additionally uses real private→semiglobal→global calls,
literal expected execution results, owner-specific packed tuning, strict
metadata conversion, malformed/mixed bindings, and source-removal acceptance.
