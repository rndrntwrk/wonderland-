# Runtime interfaces required to finish B interaction and instruction debugging

This is a concrete integration request to the A/F owners. It is **not an
implemented API**. B does not change the A implementation or the shared protocol
in this branch. The findings below were checked against A commit
`8a0e251d19e222a0a6833d7408ca629f674e1729` and original FreeSO commit
`4c6b3e8f5835b228723caea3c9f683c62f244f73`.

The public A runtime already accepts complete ticks, validates their state
compatibility, evaluates isolated routines, performs routing and slot commands,
and exposes validated stored frames. B now supplies actual interaction content
and fact adapters, a proved read-only check subset, complete active-queue user
projection, bounded entity inspection, and additional scoped watches. The three
interfaces below are what prevent those surfaces from becoming a complete
source-equivalent queue/check/debugger integration.

## 1. Complete detached check execution

### Evidence

At the A pin, `crates/sim-core/src/runtime.rs:197–204` defines `QueryOutcome` with
only stop, temps, extended temps, instruction count and diagnostics.
`SimRuntime::query_behavior`, at lines 468–508, clones the runtime state and runs a
temporary check thread, then discards the candidate state and the remaining
thread fields. `run_synchronous_thread`, at lines 1970–2037, and the private
`RuntimeHost`, beginning at line 2039, own the actual primitive host behavior.
B cannot obtain equivalent state by calling the public `VmThread::run` with that
host.

The original `VMThread.EvaluateCheck`,
[VMThread.cs](../../TSOClient/tso.simantics/Engine/VMThread.cs):92–127, clones the
original temporary arrays separately for each out-of-tick check, shares them for
in-tick checks, runs with `IsCheck`, optionally installs queued action context,
and collects action-string variants and modified motive advertisements. Its
result distinguishes an aborted VM and dialog cooldown. `CheckAction` at
947–1052 and `GetPieMenu` in
[VMEntity.cs](../../TSOClient/tso.simantics/Entities/VMEntity.cs):980–1034 consume
those effects, including the per-entry HideInteraction reset. The current B
interaction engine deliberately preserves this distinction in
`query.rs::collect_offers`.

### Minimal supported operation

A should add a core-owned operation that evaluates the same interpreter and
host on a detached candidate and returns the **complete candidate plus the
completed temporary check thread**. The names below describe the contract;
they do not require A to adopt B types or this exact spelling.

| Input/output | Required meaning |
|---|---|
| Base binding | Lot, authority epoch, actual content/tuning descriptor, tick and canonical starting-state hash. A stale candidate cannot be committed against a different state. |
| Input | Actor generation, routine key, caller/callee/stack-object context, code owner, bounded argument array, total instruction budget, and explicit query origin. Optional action context must carry the source interaction number/flags needed by primitives. |
| Detached state | All actual candidate `SimState` changes, including RNG, attributes, person data, motives, object status, scheduler/continuations and lifecycle changes. A checked full-state envelope is acceptable initially; an incomplete hand-maintained delta is not. |
| Temporary thread | Final temps, extended temps, advertisements, ordered action-string variants and their parameters, stop, diagnostics, actual instruction count and query termination reason. Distinguish absent action-string collection from an empty collected list. |
| Out-of-tick origin | Each check starts with the caller's original temp arrays; RNG and other detached changes can carry between checks in one menu evaluation. Discard the entire candidate at the UI boundary. No authoritative state or external dispatcher is touched. |
| In-tick origin | Temp aliases and check-visible state obey the actual active-thread rules; all changes are staged inside the same accepted simulation transaction. No extra clock advance or entity-count RNG mix is introduced by the check. |
| Requests and effects | Return deterministic, bounded pending records and source termination semantics. Query execution never dispatches durable or private service effects. A owns the treatment of primitives that cannot complete synchronously. |

The simplest first implementation can move the candidate state and temporary
thread already produced by `query_behavior` into an opaque validated result.
It must retain the origin-sensitive alias rules implemented around
`RuntimeHost::entity_temps_alias` and synchronous checks; exposing a clone alone
does not establish those rules. Applying an in-tick result belongs inside A's
transaction, not a sequence of public B `WriteMemory` commands after a tick.

Required acceptance cases: rejected checks preserve the correct shared RNG and
in-tick temps; UI checks leave their source snapshot byte-identical on every exit;
multiple action-string variants retain source order and parameter zero;
advertisements reset for each check; a no-variant result retains its modified
advertisements; HideInteraction/Hidden/out-of-world filtering sees actual
candidate values; dialog, deletion, nested checks and budget exhaustion cannot
leak a partial authoritative commit. Run each accepted-tail case on authority,
replica and WASI with exact hashes.

### What B can execute now

`interactions::ReadOnlyChecks` certifies every instruction in the complete direct
call closure before running A's real query. It admits Expression comparisons
(operators 0, 1, 2, 8, 14, 15 and 16), Test Object Type, and resolved direct calls.
A's implementations in `primitives/arithmetic.rs`, `primitives/entities.rs:67–92`
and `vm/interpreter.rs:300–339` establish that this subset does not change world
state, RNG, advertisements or action strings. A new temporary frame may change
its own control position; no omitted caller-visible output is required.
The root check binding is resolved separately from the action's frame CodeOwner;
proof traversal and actual nested calls retain that action context. B also maps
the source intent-verification override onto only the detached target's raw
Occupied flag, preserving UseCount and wider in-use state.
Stateful, indirect, unknown or unresolved code is rejected before execution.
This is useful real execution with a proof boundary, not a replacement for the
complete operation above.

## 2. Atomic queued-action frame execution

### Evidence

A's `AcceptedCommand::StartBehavior`, declared at `runtime.rs:80–87` and applied
at 651–680, either replaces the thread or requires an empty frame stack. Its
`VmThread::push_entry`, `vm/interpreter.rs:21–54`, creates a normal frame with
`action_tree = false`. It cannot represent pushing an interaction above an idle
or parent action, source SpecialResult handling, or run-immediately injection.

The original
[VMQueuedAction.cs](../../TSOClient/tso.simantics/Engine/VMQueuedAction.cs):47–63
sets ActionTree and carries caller, callee, stack object, code owner and source
arguments. `VMThread.cs:198–285` implements AttemptPush, run-immediately insertion,
protected active queue prefixes, callback/priority cleanup and interaction abort.
Those operations are interleaved with the actual interpreter stack. A already
models `SpecialResult::Interaction` and the `action_tree` field internally, but
does not expose an authoritative B queue seam.

### Minimal supported operation

A/F should provide one transaction-owned queue bridge, avoiding a dependency on
B's package layout. It needs these operations in the actual scheduler context:

1. Capture authenticated, generation-aware queue/action inputs and source facts.
   Include queue revision/UID, mode, flags, priority, caller, callee, stack object,
   routine/code owner, arguments, result-check state and callback identity.
2. Run the complete check operation above on the current candidate. Revalidate
   the source action immediately before execution; a prior menu offer is not a
   capability to bypass this check.
3. Push a real interaction frame with ActionTree and the appropriate special
   result **above** the source parent/idle frame; preserve parent locals,
   instruction pointer and continuation. Handle run-immediately insertion and
   inherited action flags at the same deterministic boundary.
4. Return actual completion, rejection, yield, interruption, target deletion and
   callback outcomes to B's queue state machine before releasing the active
   prefix. Complete priority, cancellation, motive-change and NonInterruptable
   cleanup in the source order.
5. Commit queue state, frame/scheduler changes, usage projections and accepted
   effects together. Reject an invalid action without partially changing either
   queue or simulation state.

The interface can be an A-owned host trait, an opaque transaction closure or
accepted commands plus deterministic callback records. A `StartBehavior` rename
cannot satisfy it. Creating a second host/interpreter in B cannot satisfy it.

B's `project_active_queues` is available now. It reads the complete authoritative
queue set, includes suspended parents in the active prefix, deduplicates users,
excludes future entries and emits A's `SetInteractionProjection`. Its prepared
result is bound to A's full canonical state hash. A then performs the actual
atomic tick. This supplies the source UseCount input, while the operations above
remain necessary for queue execution.

Required acceptance cases: two actors selecting one slot; a stale target or queue
UID; check rejection after a menu was shown; parent-idle resumption; a
run-immediately action over another active action; cancellation while routing or
animating; target deletion during use; callback/priority/motive cleanup once;
queue projection clearing after completion/disconnect; authority and replica
hash equality for the same input order.

## 3. Instruction pause, step and executed trace

### Evidence

The current public `SimRuntime::step` is a complete accepted tick.
`dispatch_entity`, `runtime.rs:1086–1128`, turns dispatch-budget exhaustion into
the real fault `source dispatch instruction limit exceeded`. Public frame
positions are stored state. Neither constitutes an executed instruction trace
or a resumable debugger pause.

The original `VMThread.cs:290–336`, 547–552 and 667–702 implements pause,
breakpoints and step-in/over/out against actual instructions and frame depth.
The interpreter and private host in the A pin expose no corresponding supported
hook. B's `trace` therefore returns an explicit unsupported result, and its
Creator step operation is labelled one whole empty accepted tick.

### Minimal supported operation

Add an opt-in, isolated debug session that owns an unfinished candidate accepted
tick and its real host/scheduler context. An instruction observer can inspect the
actual before/after boundary and request a **distinct debug suspension**. The
suspension must preserve the executing thread, active host overlays, nested
checks, command position, scheduler traversal, continuation results, pending
events/effects and instruction accounting. A snapshot that serializes only
`SimState` is insufficient while that work is outstanding.

The session should support a bounded set of generation/routine/instruction
breakpoints; step-in, step-over and step-out using actual frame depth; an ordered
bounded trace with entity, routine, instruction, opcode, branch/exit and relevant
source context; and resume using the same accepted tail. At limits it must return
an explicit trace-capacity or debug-suspension result, not silently truncate or
convert it into a gameplay fault. Debug hooks must be disabled in normal
authority execution and must never release private state or dispatch effects
from an isolated session.

Required acceptance cases: stop before a marked instruction, step through a
direct call and return, step over a child frame, step out, pause around a yielded
host request, resume after a snapshot/session round trip, and observe a genuine
budget fault. An uninterrupted run and the same run paused/resumed at every
instruction must produce identical final accepted-tick state, event order and
RNG on native and WASI.

## Content needed for complete chair, bed and appliance paths

The checked-in original object IFFs provide real private routines, TTABs,
definitions and resource requirements. The executable source-family probe tests
those bytes with A's actual interpreter and keeps every unresolved call and
actual failure visible. It does not supply missing global/semi code, animation
metadata, normalized routing slots, multipart topology or genuine TSO tuning.
Full object paths need those resources from the content owner in addition to the
three runtime interfaces above. B's strict content importer continues to reject
incomplete effective scopes; the diagnostic harness is explicitly separate.

See [runtime-bridge.md](runtime-bridge.md) for the implemented API, reproducible
tests and native/WASI evidence. None of these external requirements is marked
complete by a fixture, a static call graph, a stored frame, or a whole-tick step.
