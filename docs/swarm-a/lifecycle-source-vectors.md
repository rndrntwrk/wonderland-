# Native source vectors: lifecycle, main frames, notify, and scheduler

Reference: FreeSO commit 4c6b3e8f5835b228723caea3c9f683c62f244f73.

The runner tools/swarm-a/reference-lifecycle.py verifies SHA-256 hashes for ten original C# inputs, expands the companion C# template with unchanged source methods, compiles with Mono, and checks the resulting TSV against reference-lifecycle.expected.tsv. Both the pinned external checkout and this branch's unchanged C# copy produced the same vectors.

Run from the repository root:

    python3 tools/swarm-a/reference-lifecycle.py
    python3 tools/swarm-a/reference-lifecycle.py --source /path/to/FreeSO

The command requires mcs and mono; the recorded execution used Mono 6.8.0.105 amd64 with unchecked arithmetic, matching the numeric harness policy. It downloads nothing and removes its temporary generated source and assembly. The --record option intentionally refreshes the expected output and is for reviewed fixture maintenance.

## Actual source compiled

Paths in this section are relative to TSOClient/tso.simantics/. The complete original Engine/VMScheduler.cs, Primitives/VMNotifyOutOfIdle.cs, and primitive handler/operand/exit-code declarations compile unchanged.

| Input | Exact executable source used |
|---|---|
| Entities/VMEntity.cs:297–329,439–442,485–551 | Entity tick scheduling, RunEveryFrame, Init, and Reset. |
| Entities/VMEntity.cs:557–650 | Entrypoint overloads and ExecuteGenericEntryPoint. |
| Engine/VMThread.cs:82–129,185–193,738–750 | Check evaluation, thread construction, and frame push. |
| Engine/VMThread.cs:316–321 | The empty-stack main-restart block within Tick. |
| Engine/VMStackFrame.cs:36–55 | Stack-object reference/ID accessors. |
| VM.cs:487–503 | Sorted object-list insertion and deduplication. |
| VMContext.cs:524–532 | Actual xorshift/multiplication NextRandom. |

The template supplies object lookup, routine metadata, storage, logging, and renderer/content/sandbox shims. Its small thread fixture executor records a frame and executes a supplied action; it **does not implement the original BHAV instruction loop**. The original empty-stack restart block is embedded in that executor. These boundaries permit exact entrypoint/frame/scheduler/check-temp observations without claiming full instruction, queue, content, renderer, or persistence parity.

Fixture routine IDs map entry 0→4096, entry 8→4104, main 1→4097, and reset 3→4099. Ordinary fixture routines declare four arguments and two locals; source Push allocates four locals using max(Locals, Arguments). TSV stack=0 means the original null stack-object reference. Caller and callee remain the owner entity.

## Exact lifecycle vectors

| Fixture | Source output |
|---|---|
| Default creation, owner 7 | Init 4096 and dynamic-multitile 4104 run immediately as checks, with caller/callee 7, stack 0, and four zero arguments. Main 4097 is queued, not executed by Init. It has null stack/four zero arguments; the initial wake is tick 1. |
| Creation MainParam=321, MainStackOBJ=9 | Both creation fields are zero immediately after Init. Queued and first main use stack 9 and arguments [321,0,0,0]. Restarted main uses stack 0 and [0,0,0,0]. |
| Creation MainParam=-123, missing MainStackOBJ=99 | The first main receives signed parameter -123 and null stack. Both creation fields are consumed. Restart uses zeros/null. |
| Main entry disabled, unresolved, or empty | Both initialization checks run. No main frame is queued, but both creation fields are consumed and the initial wake remains tick 1. |
| Ghost creation, parameter 321/stack 9 | Both checks run inside balanced sandbox/restore pairs. No entity thread or calendar wake is created. Creation fields remain 321/9; room becomes -1 and light refresh is requested once. |
| Reset with creation fields 55/9, interrupt true, temp0=22 | Reset check 4099 runs, then main is queued with null stack/four zeros. Existing temp0, interrupt, and creation fields remain unchanged. |
| Reset whose entry-3 body writes temp0/XL0 from 11/22 to 77/88 | With Scheduler.RunningNow=false, the owner keeps 11/22. With RunningNow=true, the owner receives 77/88. Both resets queue one main frame. |

VMEntity.Init:501–513 consumes nonzero creation fields **before** attempting main. A missing main must not defer consumption until a later restart. ExecuteGenericEntryPoint:591–598 passes stackOBJ directly; VMStackFrame:36–43 maps null to zero. There is no fallback to the caller. VMThread:316–321 restarts main through the parameterless entrypoint overload.

The reset-temp fixtures compile the actual Reset path: VMEntity:540 calls immediate entry 3, ExecuteGenericEntryPoint:599–602 calls EvaluateCheck, and VMThread:94–100 clones both arrays when the scheduler is not running. Source Reset never copies those cloned arrays back. An externally admitted reset must not unconditionally copy the temporary check's returned banks into the owner.

## Check and notify vectors

The original check evaluator shares temp arrays only while Scheduler.RunningNow is true. The test body changes temp0/XL0 from 11/22 to 77/88:

| Running now | Fixture aborts during execution | Numeric return | Owner temp0 / XL0 afterward |
|---|---|---:|---:|
| False | False | 4, RETURN_TRUE | 11 / 22 |
| True | False | 4, RETURN_TRUE | 77 / 88 |
| True | True | 6, ERROR | 77 / 88 |

VMNotifyOutOfIdle.Execute:8–15 always returns numeric 0, GOTO_TRUE. Null targets and targets without a thread do nothing. A target with a thread always receives Interrupt=true, even if its scheduled end is zero or already equals the current tick and no calendar wake is created.

During tick 1, object 5 notifies sleeping objects 8 and 2. Their scheduled ends immediately become 1 and 2. Repeating the notification to 8 does not duplicate its current-tick entry. Actual dispatch order is 5 then 8. Original entity tick logic then schedules processed objects for tick 2, leaving that bucket [2,5,8]. Outside active dispatch, notifying a target scheduled for tick 20 at current tick 1 moves it to tick 2.

## Scheduler vectors and deliberate Rust differences

- Scheduling IDs [7,1,5,1] for tick 1 executes [1,5,7] once each. Four entities and starting seed 100 produce seed 104 after the scheduler's count mix. The terminal object cursor is 32767 and RunningNow is false.
- At tick 40, delay 90 remains wake 130 for an ordinary object but clamps to wake 41 for headline and disabled objects.
- Source uint addition wraps u32::MAX+1 to zero. Rust's wider clock and explicit overflow errors are intentional policies.
- After source reset, the tick-1 bucket migrates to first supplied tick 100 while thread scheduled-end fields still say 1 until execution. Their next wake is 101. Rust snapshots preserve an exact calendar rather than repeating the legacy-load migration.
- Source ScheduleTick(entity,2) then ScheduleTick(entity,5) retains both entries while ScheduleIdleEnd says 5. In the fixture, each actual dispatch draws NextRandom(1) and schedules itself 100 ticks later. Through ticks 1–5 it runs at ticks 2 and 5, leaves buckets 102 and 105, and ends with exact RNG seed **139645673477529697**. Rust intentionally retains one wake, so this is an observed source-deviation vector, not an equality target. Execution/RNG consequences are also disclosed in numeric-source-notes.md.
- With seed 100 and three entities, the single deferred victim deletion sees seed 103, three entities, and RunningNow=false. After the fixture deletion, two entities remain. Duplicate requests for that victim produce one deletion. One victim deliberately avoids asserting an order for the source HashSet.

These native results fix concrete expectations for Rust runtime tests. They do not themselves compare Rust behavior, establish native/WASM equality, or turn the deliberate single-wake, clock-width, deletion-order, and snapshot-calendar policies into legacy parity.
