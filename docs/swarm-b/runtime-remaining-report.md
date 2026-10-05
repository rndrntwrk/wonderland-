# B runtime continuation: verified implementation and remaining integration

This report covers the B-owned runtime lane against simulation commit
`8a0e251d19e222a0a6833d7408ca629f674e1729` and original source commit
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. It does not certify completion of all
W06/W16 gameplay and graphical debugger requirements. Publishing and integration
remain coordinator-owned.

## Delivered behavior

The actual A runtime now has B-owned adapters for resolved local/global
interaction catalogs, authoritative actor/target facts, a proved read-only check
subset, and complete active-queue usage projection. Global TTAB bindings retain
per-target code ownership; broken-state filtering uses the real multipart base.
The query provider runs A's interpreter after certifying the entire direct-call
closure. Separately resolved check roots retain the action's CodeOwner for nested
calls. Intent verification clears only the detached target's raw Occupied flag;
queue UseCount and wider in-use facts remain intact. The queue projection includes
suspended parent actions, omits future actions, deduplicates users and clears
obsolete usage. Prepared commands are bound
to the actual canonical runtime state and enter one real accepted transaction.

`inspect_entity_json` exposes actual stored frames, locals, arguments, registers,
object/containment state, active users, advertisements, action strings,
diagnostics, route continuations, physical slots, reservations and occupants.
Every u64 is a canonical decimal string, recursively; u32 and smaller source
integers remain numbers. A nonallocating counting pass admits complete escaped
JSON before output allocation, with a 1 MiB cap. New watches cover local/argument
frame depth, person data, motives and computed global memory through A's reader.
The existing Creator trait and actual snapshot/tick replay behavior are preserved.

The source-family runner reads and hashes three untouched original IFFs, imports
all 144 private routines without replacing gameplay, and evaluates 576 cases:
object-self and avatar-to-object contexts in both actual dialects. Every query
checks byte-identical isolation. Every accepted tail runs on actual authority and
replica, checks state hashes and event order, suppresses replica external effects,
and round-trips the final snapshot. Failed ticks check atomicity. Original chair
room-impact, bed sleep-begin/end, and appliance RNG cases have literal assertions.
All missing content and real VM failures remain visible.

Detailed APIs and reproducible commands: [runtime-bridge.md](runtime-bridge.md).
Complete outcome inventory: [runtime-source-family-results.json](runtime-source-family-results.json).
Minimal A/F extension request: [runtime-extension-contract.md](runtime-extension-contract.md).

## Verification evidence

All commands below ran against the current sources in one dedicated exact-A
assembly. No original C# or assets and no A implementation files were edited.
After the final root-binding and Occupied fixes, the complete native suite,
formatting and all-target Clippy passed again. Native and WASI replay executables
were rebuilt, the browser-target library was checked, and all three parity
drivers passed again. The final family report and checked-in outcome inventory
are byte-identical to the earlier verified versions. The dedicated build target
was then fully cleaned; source, logs and reports remain available.

| Gate | Result |
|---|---|
| Full bridge default-feature native tests, locked/offline | 107 passed; 0 failed. Includes actual Creator provider, importer/cooked, output bounds, interaction adapter, inspection and all source-family cases. |
| Authoritative B interactions module standalone tests | 55 passed; 0 failed. These preserve the existing source-contract, queue, query/intent and nested-module tests. |
| Bridge `fmt --check` | Passed. |
| Bridge `clippy --all-targets -- -D warnings`, locked/offline | Passed. |
| Native no-default examples and `runtime-bridge` executable | Built successfully. |
| `wasm32-wasip1` no-default examples and executable | Built successfully. |
| `wasm32-unknown-unknown` no-default library check | Passed against the real pinned core. |
| Existing source native/WASI parity | Passed; literal outcomes, fixed state/snapshot hashes, 2 negative controls. |
| Existing cooked native/WASI parity | Passed after original sources were removed; selected packs only; 3 negative controls. |
| New family native/WASI parity | All 576 cases exactly equal; literal source helper outcomes; 2 changed-source/state negative controls. |
| Assembly path protection Python suite | 9 passed; 0 failed. |
| New WASI driver syntax and inventory regeneration | Passed; all 576 unique cases retained in both query and accepted outcome groups, regenerated inventory byte-identical. |
| Owned-file whitespace check | Passed. |

The existing source state hash remains
`d52e0091425b12e7d820dcb7f1b883fb36606059b7a121bb5cf9218cc272f62b`,
and snapshot SHA-256 remains
`49094c9adc9b1e3df73264ec221c323ce0027ec231af4f3e38b2b006fe0ca43d`.
The existing cooked state hash remains
`d73de0a5492c86856824210438b57324e557f90c5bc3cc0ddda22697003cab56`,
and snapshot SHA-256 remains
`621ce538f35e35a782fe2399702ba01fac12c7bb0c3dbee9af9b2d2f23e77813`.
The new full family report is 1,318,573 bytes with SHA-256
`fe480079aaffc1c167feb0ae5438e324b9bdebd3ac51ed4e101141b421b3e2be`.

## Failing acceptance cases observed before fixes

| Acceptance | Observed RED | Verified GREEN |
|---|---|---|
| Actual scoped watches and bounded inspection | Required watch variants/inspection module unavailable. | Real frame-local/argument/person/motive/global reads and bounded schema tests pass. |
| Active queue usage and world/query adapters | Required concrete adapter APIs unavailable. | Actual A UseCount observes the protected prefix, excludes future entries, clears users and replays identically. |
| Pure Test Object Type checks | Provider rejected the real opcode as unsupported. | Complete closure proof admits A's read-only handler; actual target GUID determines the offer. |
| Multipart broken-state filtering | A normal interaction on a wing was incorrectly offered when its base was broken. | The real base object's Broken flag suppresses it. |
| Global table owner binding | Observed offer list `[900, 501]` where target owner 1 should only receive `[900]`. | Global rows are bound to their explicit target code owner; the other owner cannot supply its private context. |
| Browser full-width identity | JSON numbers appeared where decimal strings were required, including `9007199254740993`. | Recursive exact-u64 serialization passes >2^53, u64::MAX, nested enum/map/array and real reservation tests. |
| Physical slot inspection | A valid actual reservation existed but the inspection field was null/missing. | Real definitions, tokens, expiry and actor/owner records are emitted with exact IDs. |
| Resolved check root and frame CodeOwner | The direct provider substituted the action owner's true routine for the declared owner's false root and missed a nested world write. | Independent root resolution, unchanged action CodeOwner for nested calls, and whole-closure rejection now match A's actual query and preserve isolation. |
| Intent verification Occupied semantics | Verification kept the raw bit set and rejected a valid intent; snapshot conflated another avatar's using frame with that raw flag. | Only detached ObjectData8 bit5 is cleared; other flags, active UseCount and live snapshot remain unchanged. |

Source-family tests characterize the original bytecode in a declared harness.
The bed's post-behavior TickCounter value of 1 was checked against both original
`VMAvatar.Tick` and A before setting its expected value. Avatar movement flags are
explicit harness inputs. The appliance's unsupported STR# 300 is recorded as a
rejected source resource; no empty replacement is created.

## Independent review

A separate formats-lane reviewer checked the query proof against the actual A
primitive host, catalog/target/code-owner bindings, queue projection/state-hash
binding, recursive u64 serialization and counting-first inspection allocation.
Their direct cross-owner fixture exposed an incorrect root substitution. The
review also confirmed the original Occupied-flag semantics. Four regressions
failed before the fixes and passed afterward; the reviewer independently reran
all four and repeated the original A-versus-adapter probe. No blocking finding
remained in the reviewed runtime scope. The complete 107-test native suite and
all-target Clippy were rerun after those changes.

## Remaining boundaries and exact ownership

| Requirement | Available now | Remaining owner/input |
|---|---|---|
| Complete mutable interaction checks | Real statically proved read-only subset and complete source/fact projection. | A must expose detached candidate state, query thread/action strings/advertisements/RNG and correct origin-sensitive temp semantics. |
| Real action queue execution | Full B queue state machine and real active-use projection. | A/F transaction seam for ActionTree/SpecialResult frame insertion above parent/idle frames, completion/cancellation callbacks and atomic queue/VM commit. |
| Complete chair/bed/appliance gameplay | Every private routine evaluated; real helper paths, failures, events and replay recorded. | Content owner must supply original global/semi code, animation metadata, normalized routing slots, multipart topology, genuine tuning and service providers. |
| Graphical runtime watches and tick playback | Public actual SimDebugProvider and bounded live-runtime inspection APIs. | Creator-web has not bound these APIs; host needs actual runtime/content/snapshot/input state and the pinned A assembly or integrated tree. |
| Instruction breakpoint/step/trace | Source CFG and stored frame inspection, explicit unsupported result, honest whole-tick stepping. | A must expose a resumable instruction suspension and executed trace with complete host/scheduler state. |
| Authentication and queue revisions | Generation, content, epoch and canonical runtime-state validation. | Integrating service supplies principal authorization, TSO ownership and monotonic query revision; scheduler preserves the prepared queue revision through tick acceptance. |

A real StartBehavior command cannot be relabelled as action-frame push. A real
instruction-budget fault cannot be relabelled as a debugger pause. Uploaded JSON
reports do not create a live VM. The three minimal extensions and source anchors
are specified in the linked contract with concrete cross-platform acceptance
cases.

## Exact changed-file allowlist for this lane

Existing files changed:

- `crates/content-runtime-bridge/Cargo.toml`
- `crates/content-runtime-bridge/Cargo.lock`
- `crates/content-runtime-bridge/src/lib.rs`
- `crates/content-runtime-bridge/src/isolated.rs`
- `crates/content-runtime-bridge/src/creator_debug.rs`
- `crates/sim-core/src/interactions/query.rs`
- `crates/sim-core/src/interactions/queue.rs`
- `tools/swarm-b/runtime-bridge.py`
- `docs/swarm-b/runtime-bridge.md`

New files:

- `crates/content-runtime-bridge/src/inspection.rs`
- `crates/content-runtime-bridge/src/json_u64.rs`
- `crates/content-runtime-bridge/src/interactions.rs`
- `crates/content-runtime-bridge/src/interaction_queries.rs`
- `tests/integration/swarm_b_runtime/inspection.rs`
- `tests/integration/swarm_b_runtime/interaction_adapter.rs`
- `tests/integration/swarm_b_runtime/source_families.rs`
- `tests/integration/swarm_b_runtime/source_families_export.rs`
- `tests/integration/swarm_b_runtime/support/families.rs`
- `tools/swarm-b/runtime-source-families-parity.py`
- `tools/swarm-b/runtime-source-families-wasi.mjs`
- `docs/swarm-b/runtime-extension-contract.md`
- `docs/swarm-b/runtime-source-family-results.json`
- `docs/swarm-b/runtime-remaining-report.md`

The existing interactions package alias compiles the exact authoritative B module
by path; no copy is inserted into the exact A export. Its temporary package
boundary is documented. Root-owned verification inputs and Creator/Creator-web
sources are outside this lane's edits.
