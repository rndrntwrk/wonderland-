# Independent runtime bridge review by formats completion

Date: 5 October 2026. Reviewer: `/root/formats_completion`; author:
`/root/runtime_completion`. No author-owned source edited. Scope:
`interaction_queries.rs`, `interactions.rs`, `inspection.rs`, `json_u64.rs` and
their direct source/API contracts and focused tests.

## Disposition

Independent review signed off after targeted fix verification on 5 October
2026. RR-F1 and RR-F2 are resolved. The original independent root-binding
reproducer now matches actual pinned A, and all four new targeted regression
cases were independently executed successfully. No blocking finding remains in
the requested read-only proof, global catalog binding, queue projection or JSON
DTO scope. Final aggregate/build gates remain with the coordinator.

## RR-F1 — root check silently resolves under the wrong declared owner

Original reviewed locations:
`crates/content-runtime-bridge/src/interaction_queries.rs:484–485,532–542`.
`ReadOnlyChecks::evaluate` resolves and certifies `check.routine_id` using
`definition.action.code_owner_guid`, ignoring `check.code_owner_guid`.

B's public `WorldProvider` contract in
`crates/sim-core/src/interactions/adapters/mod.rs` explicitly allows a resolved
check binding to have another owner while retaining the action CodeOwner in the
check frame. These are distinct inputs: the root routine's namespace selects
the code to execute, and the frame owner determines nested calls and tuning.
The existing implementation silently substitutes another private BHAV when two
owners have the same private ID.

Independent reproduction uses existing verified A/bridge rlibs, with no broad
rebuild. Both content owners have private routine 4096; owner1 tests 1==1 and
owner2 tests 1==2. Target GUID is 1. Pinned A receives root `Private(2)/4096`
and frame `code_owner=1`, returning `Completed(ReturnFalse)`. A public
`InteractionSnapshot` with action owner1/check owner2 passed to `query_in_tick`
through `ReadOnlyChecks` instead returns one offer. Exact output:

```text
actual A explicit check owner=2, action context=1: Completed(ReturnFalse)
target GUID=1, action owner=1, check owner=2: Ok(1)
target GUID=1, action owner=2, check owner=2: Ok(0)
```

Reproducer source and executable:
`/workspace/scratch/378e4c36af7b/creator-review-formats/runtime-binding.rs` and
`runtime-binding`. Built with rustc against
`runtime-remaining-assembly/target/debug/deps/libwonderland_content_runtime_bridge-30d69d41825b72fc.rlib`.

Required correction: resolve the declared root independently, prove that exact
root while following nested calls with the action owner, and execute that root
with the action owner in FrameContext. An explicit unsupported error for a
nonrepresentable cross-owner case would also be safe; silently replacing it is
not. Author accepted and is implementing full representable binding semantics.

The normal RuntimeCatalog path already checks code_owner against the target's
imported GUID and validates resolved routine keys. The second probe confirms
that a custom authoritative provider can supply a different target/action
binding; this direct provider has no authenticated network constructor. Author
will make its trusted-definition boundary explicit while preserving the generic
B adapter contract. This is not reported as a network authorization bypass.

## RR-F2 — intent validation does not map the detached raw Occupied flag

Identified by the author while correcting RR-F1; reviewer independently checked
the source/control flow. Original
`TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetInteractionCmd.cs:48–51`
saves, clears and restores only the target's raw Occupied flag. In A this is
ObjectData8 bit5. B's `validate_intent` correctly sets the detached
`CheckState.target_occupied=false`, but the runtime provider does not copy this
field into the decoded target before running a certified comparison.
Additionally, RuntimeWorld currently initializes target_occupied from the wider
`runtime_memory::is_in_use`, which includes users and other occupancy sources,
rather than that raw flag.

Author is adding regressions for an occupied-bit comparison during intent
validation and a queued-user case with the raw bit clear. Required behavior is
to alter only the detached raw flag, retaining queue users and UseCount, and
leave the live simulation untouched. No independent runtime execution of RR-F2
was claimed at the initial checkpoint; the follow-up below executes the fixed
behavior independently.

## Independent fix follow-up

The author observed four regression failures before the fixes, then all thirteen
interaction-adapter tests passed. Reviewer read both author logs
`runtime-review-red.log` and `runtime-review-green.log`, inspected the revised
implementation, relinked the independent original probe against the new rlib,
and independently executed each of the four new test cases:

| Regression | Independent result |
| --- | --- |
| `declared_check_root_can_differ_from_action_code_owner_without_rebinding_nested_calls` | Passed; direct and nested roots agree with actual A's false result |
| `foreign_check_root_proof_follows_action_owner_calls_and_rejects_world_writes` | Passed; the action owner's nested write is rejected before execution |
| `intent_validation_clears_only_the_detached_target_occupied_flag_and_retains_use_count` | Passed; another flag and UseCount remain intact, live snapshot unchanged |
| `snapshot_occupied_is_the_raw_target_bit_and_not_another_avatars_using_frame` | Passed; broader in-use state does not replace the raw bit |

The independent original probe now prints `Completed(ReturnFalse)` for A and
`Ok(0)` for the cross-owner bridge query. The corrected certificate accepts an
explicit root RoutineKey and follows nested calls with action CodeOwner; actual
execution receives that same root and unchanged context. Only ObjectData8's
Occupied bit is changed in the detached target. The direct provider's trusted
WorldProvider definition contract is explicit; RuntimeCatalog retains exact
target binding.

Small reproducible evidence is retained under `review-formats/`:
`runtime-binding.rs`, `runtime-binding-before.log`, `runtime-binding-after.log`,
`runtime-fix-independent.log`, and `rechecked-file-hashes.json`. Compiled review
artifacts are disposable; no broad runtime suite was rebuilt by this reviewer.

## Source-backed checks completed

A pin: `8a0e251d19e222a0a6833d7408ca629f674e1729`. Independently byte-compared
eight source files in `/workspace/scratch/378e4c36af7b/runtime-remaining-assembly`
against `git show` at that exact pin; all matched:

| A source | Reviewed contract |
| --- | --- |
| `runtime.rs` | QueryOutcome omits detached candidate/RNG/output; query_behavior clones and discards state; real RuntimeHost reads borrow immutable state; temporary-bank alias rules |
| `primitives/arithmetic.rs` | Expression operators 0,1,2,8,14,15,16 only read their operands and return a comparison; writes/list mutation/random branches excluded |
| `primitives/entities.rs` | TestObjectType reads variable/entity GUID and master GUID without mutation |
| `vm/interpreter.rs` | Direct calls resolve with parent.context.code_owner and retain parent context; argument construction reads temps |
| `vm/memory.rs` | Admitted primitive memory scopes use immutable reads, including lists, tuning, person data and clocks |
| `runtime_memory.rs` | Host memory and advertisement reads come from stored runtime/content/thread views, not ambient services |
| `snapshot.rs` | Exact envelope/content/lot/epoch validation; bounded guarded decoding and validated live entity/thread invariants |
| `state.rs` | Content/state representation and actual primitive/DTO field widths |

Original FreeSO source pin:
`4c6b3e8f5835b228723caea3c9f683c62f244f73`.
`VMEntity.GetRoutineWithOwner` always supplies Object as CodeOwner for global,
private and semiglobal routine resolution. `VMThread.CheckAction` uses the
action's CodeOwner and CheckRoutine independently, reads multipart BaseObject
Broken, and supplies the four check arguments. These support the bridge's
global-table per-target catalog and the required RR-F1 distinction.

## Other reviewed behavior

- Read-only certification traverses every instruction in the complete direct
  closure, including unreachable branches, and caps both routines and total
  instructions. Source reads match the whitelist. Disallowed writes, RNG,
  advertisements/action-string mutations, indirect calls and unresolved
  dependencies cannot enter the real query path.
- RuntimeCatalog retains local-table absence, per-target global source binding,
  source table order and exact routine namespaces; duplicate indices, invalid
  local owners and label/allocation excess are rejected before label cloning.
- RuntimeWorld borrows a validated fixed runtime/authority view, checks source
  content descriptors, live generations and caller authorization, and counts
  serialized provider-state size before capture. Authentication, TSO ownership
  projection and a monotonically advanced world revision are explicit service
  responsibilities.
- Queue projection validates complete authoritative queues, live avatar/target
  generations and modes, protected active prefixes and duplicate owners. It
  deduplicates users, clears absent queues, admits retained map/command copies,
  and binds prepared commands to A's canonical starting state hash. A's accepted
  transaction validates/commits the projection and trusted tail atomically.
  Queue-only mutation is outside this state hash and is explicitly assigned to
  the integrating scheduler's queue-revision discipline.
- ExactU64 recursively wraps options, sequences, tuples, maps, structs and enum
  payloads. Its u64 formatter uses a fixed 20-byte stack buffer; every nested
  u64 becomes a decimal string. Smaller signed/unsigned fields retain numeric
  JSON form. Existing literal test includes u64::MAX, 2^53+1, array values, map
  keys/values and nested VmStop::Waiting.
- Inspection DTOs borrow frames, arrays, diagnostics and collections; route,
  slot, reservation and advertisement views stream via Serialize. A bounded
  nonallocating counting pass includes JSON escapes before an exactly sized
  output Vec is created. Both passes use ExactU64. Stale generations, missing
  threads, zero/excess limits and one-byte-short reports fail explicitly.

Reviewed focused author tests:
`tests/integration/swarm_b_runtime/interaction_adapter.rs`, `inspection.rs`, and
`json_u64.rs` unit test. Author reports 103 default native tests and existing
native/WASI original-routine evidence; those broad gates were not redundantly
executed by this reviewer.

Initial reviewed file hashes:

```text
interaction_queries.rs 2fa0cf9b9d7d441d76c1d8a9d099f179bf5fd0e83e2452eb036ac0c9d80bbe7e
interactions.rs 1092021d63c9cbaf97dc2fb1cc1c968bfcd117cf837f93ce509c4c9299a2027f
inspection.rs 2d1f370f3c8ac37cf10546eb14e4344e3a394d5b1791084e143c358f70b88c60
json_u64.rs 9758faf4958b4028befc17c9de88af21607b4c6bcde99196935a430abd17cb4a
```
