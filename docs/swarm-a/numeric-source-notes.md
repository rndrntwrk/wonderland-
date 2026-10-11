# W02.1 numeric, RNG and identity contract

Baseline: FreeSO `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
Source prefix: `TSOClient/tso.simantics/` in that repository.
This package's IDs are provisional simulation-local interfaces; shared protocol
adoption remains with Swarm F.

## Published API

- `ObjectId(pub i16)`, `PersistentId(pub u32)`, and
  `EntityRef { object_id: ObjectId, generation: u32 }` preserve source widths.
- `IdAllocator::{new, allocate, release, resolve, validate, validate_state,
  len, is_empty}` allocates the smallest free positive signed local ID.
  Allocation returns `Result<EntityRef, IdError>`; release and validation are
  fallible. Deserialization validates the allocator's internal state.
- `SimRng::{new(u64), state(), next(u64), mix_entity_count(usize)}` uses the
  exact source xorshift-star transition and explicit wrapping arithmetic.
- `numeric::{wrapping_add_i32, wrapping_sub_i32, wrapping_mul_i32,
  narrow_i16, narrow_u16, div_i32, rem_i32, mod_i32, shl_i32, shr_i32,
  shl_u64, shr_u64, flag_mask_1based, is_flag_set_i32, set_flag_i32,
  clear_flag_i32, round_ties_even, legacy_f64_to_i32, legacy_f64_to_i16,
  sqrt_i16}` supplies the VM's source arithmetic. Division and remainder helpers
  return `Result<i32, NumericError>`; the other helpers return their named
  scalar types.

## Source contract and compatibility seams

`Primitives/VMExpression.cs:63` uses `int` intermediates.
`Engine/VMMemory.cs:293` reads 32 bits only for TempXL; ordinary values are
sign-extended signed shorts. `Engine/VMMemory.cs:653` writes TempXL directly and
narrows other ordinary destinations to a signed short. The parsed expression
`IsSigned` byte is not consulted by the interpreter.

`Primitives/VMExpression.cs:76` and `:85` increment/decrement an i32 temporary,
write the potentially narrowed result, then read the RHS and compare against
the original i32 temporary. The interpreter must preserve both this ordering
and aliased memory reads. The numeric helper must not narrow the comparison.

`Primitives/VMExpression.cs:120` implements modulo as
`((lhs % rhs + rhs) % rhs)` with i32 wrapping at the addition, and leaves the
LHS unchanged for a zero divisor. This is not Euclidean remainder: negative
divisors and overflowing additions differ. At `:130`, division by zero writes
`-1`. The extracted Mono probe throws on `i32::MIN / -1` and on
`i32::MIN % -1`; the helpers surface a deterministic `DivisionOverflow` fault.

`Primitives/VMExpression.cs:96` uses one-based bit numbers in a signed i32
shift. The count masks to five bits. `:186` tests `(lhs & mask) > 0`, so the
sign bit is reported false. The u64 shift helpers mask to six bits. Normal
integral overflow and narrowing are wrapping, including release builds with
overflow checks enabled.

The source uses `Math.Round` in geometry and direction conversion, for example
`Entities/VMEntity.cs:796`, `Entities/VMMultitileGroup.cs:204`, and
`Engine/VMSlotParser.cs:210`. Its default midpoint rule is ties to even, and
the negative-zero result must be preserved. Casts from floating point truncate
instead. `Primitives/VMExpression.cs:252` casts `Math.Sqrt` to a signed short.
Out-of-range/non-finite floating casts are unspecified by the C# language;
this port explicitly chooses the behavior observed with **Mono 6.8.0.105 on
amd64**: convert to i32 with an invalid/out-of-range result of `i32::MIN`, then
truncate to i16 where requested. Thus negative sqrt gives 0 and
`(short)Math.Sqrt(i32::MAX)` gives -19196. This is a named runtime policy,
not a claim that all C# runtimes have the same unspecified conversion results.
The primary language reference documents that portability boundary in
[numeric conversions](https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/builtin-types/numeric-conversions),
and the default rounding rule is specified by
[Math.Round](https://learn.microsoft.com/en-us/dotnet/api/system.math.round?view=netframework-4.8.1).

`VMContext.cs:524` returns 0 without advancing the seed for `NextRandom(0)`.
For any nonzero maximum, XOR shifts 12, 25 and 27 update the stored u64 state;
only the return value multiplies it by 2685821657736338717 with u64 wrapping
before reducing modulo the maximum. `NextRandom(1)` advances state, and seed
0 stays 0 until something else changes it. `Primitives/VMRandomNumber.cs:12`
casts the signed range to ushort, then narrows the output to short and ignores
the destination setter's boolean result.

`Engine/VMScheduler.cs:95` adds the current entity count to the RNG after
scheduled execution and before deferred deletion, including ticks with no
scheduled objects. Call this mix exactly once for that phase. Invisible RNG
draws also occur at sleep completion (`Primitives/VMSleep.cs:25`), each route
frame dispatch (`Engine/VMRoutingFrame.cs:516`), and each direct-control frame
dispatch (`Engine/VMDirectControlFrame.cs:591`). Slot candidate scoring can
consume RNG before a candidate is rejected (`Engine/VMSlotParser.cs:159`).

## Explicit identity safety changes

`Entities/VMEntity.cs:42` declares signed-short local IDs and `:43` declares
unsigned-u32 persistent IDs. `VM.cs:478` assigns the smallest remembered free
ID; `:560` makes released lower IDs available for reuse. Sorted entity lists
use signed ObjectID order (`VM.cs:487`); object query order must keep it.

At source exhaustion, `VM.cs:568` returns 0, after which a later addition can
allocate ID 0 and subsequent positive releases cannot repair the cursor until
`UpdateFreeObjectID` runs. This port **rejects exhaustion atomically** and
keeps capacity reusable after release. Zero and negative IDs remain representable
in raw serialized legacy fields but cannot be allocated or validated as live.

The source has no generation counters. This port starts generations at 1,
increments a reused slot, rejects stale and double releases, and retires a slot
at generation `u32::MAX` instead of wrapping to an old generation. It also
rejects inconsistent allocator state when deserializing. These are deliberate
safety changes required by the supplied plan; they do not claim source parity.

`Engine/VMStackFrame.cs:37` caches an entity reference separately from its raw
stack object ID. Merely deleting/reusing the ID does not retarget that existing
reference; setting `StackObjectID` performs lookup again. The runtime/VM must
keep this distinction when translating legacy handles to generation references.

## Evidence status

`tools/swarm-a/reference-numeric.cs` is a separately runnable extraction of
small source expressions and RNG, compiled with `mcs -checked-` and run under
Mono 6.8.0.105 on amd64. It is not the complete original FreeSO runtime and
does not close full-content or end-to-end differential parity.

The tests in `crates/sim-core/tests/numeric_reference.rs` were written before
the implementation. The first run exited 101 because the three modules did
not exist yet. That was a **compilation RED**, not an assertion RED. The modules
were then created once and were not removed or reverted to reproduce failure.

| Check | Result |
|---|---|
| Extracted C# probe compiled with `mcs -checked-` | Passed under Mono 6.8.0.105, amd64 |
| `numeric_reference`, Rust 1.75 debug | 14 passed, 0 failed |
| `numeric_reference`, Rust 1.75 release, overflow checks enabled | 14 passed, 0 failed |
| `rustfmt --edition 2021` on the four owned Rust files | Completed |

The acceptance cases cover promotion and narrowing, modulo addition overflow,
division/remainder faults, masked and one-based shifts, signed-zero ties-to-even,
explicit Mono float-conversion policy, four complete RNG sequences including
zero/one bounds, tick count mixing, smallest-free reuse, invalid/stale/double
release, all 32,767 positive IDs and recovery after exhaustion, generation
retirement, corrupted allocator snapshots, and restored subsequent behavior.

Reproduction commands, from the repository root:

```sh
mcs -checked- -out:/tmp/reference-numeric.exe tools/swarm-a/reference-numeric.cs
mono /tmp/reference-numeric.exe
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo CARGO_TARGET_DIR=/workspace/scratch/cb8e2814717b/toolchain/target-numeric cargo test --manifest-path crates/sim-core/Cargo.toml --offline --test numeric_reference
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo CARGO_TARGET_DIR=/workspace/scratch/cb8e2814717b/toolchain/target-numeric cargo test --manifest-path crates/sim-core/Cargo.toml --offline --release --test numeric_reference
```

The debug run used Cargo's default target directory before workers switched to
separate target directories; the command above uses the isolated directory for
subsequent reproduction. No full original-runtime, content-corpus, native/WASM
differential, or independent-review completion is claimed by these checks.

## Scheduler integration audit

Executable native source vectors are available through
`tools/swarm-a/reference-lifecycle.py`; see
`docs/swarm-a/lifecycle-source-vectors.md` for extraction scope, recorded
output, and fixture-executor limitations. The harness compiles the complete
original scheduler and notify primitive rather than recoding them.

These are source findings for the runtime owner, not additional scheduler code
implemented by W02.1:

- `Engine/VMScheduler.cs:20`: delays greater than one clamp to one for
  `RunEveryFrame`; `Entities/VMEntity.cs:439` defines that as avatar, headline,
  or game-object disabled flags greater than `ForSale`.
- `Engine/VMScheduler.cs:27`: scheduled buckets are signed-ObjectID sorted and
  deduplicated within a bucket. Scheduling a new bucket does not itself cancel
  an old one. Descheduling at `:46` only removes a future `ScheduleIdleEnd`.
  **Intentional Rust deviation:** `Scheduler::schedule` maintains one wake per
  live entity generation and cancels its previous calendar entry before adding
  a replacement. In the source, `ScheduleTick(entity, 2)` followed by
  `ScheduleTick(entity, 5)` can leave the entity in both buckets while its
  `ScheduleIdleEnd` says 5. Rust retains only tick 5. The regression
  `schedule_replaces_old_calendar_entry` verifies this new single-wake policy;
  it is not an exact C# scheduling-parity fixture. This can change observable
  thread execution and RNG state: a suppressed extra dispatch can suppress
  primitive draws (routing alone consumes `NextRandom(1)` per dispatch), and
  skipped behavior may also change entity creation/deletion before the
  scheduler's entity-count mix. The scheduler itself does not draw RNG when
  replacing an entry. Canonical single-wake calendars are a deterministic
  correction, with these gameplay/RNG consequences, rather than a transparent
  serialization normalization.
- `Engine/VMScheduler.cs:55`: interrupts do not reschedule a missing thread,
  `ScheduleIdleEnd == 0`, or an entity already scheduled for the current tick.
  A target ID greater than the executing ID can run in the current bucket;
  other targets run next tick. `VMNotifyOutOfIdle` sets the interrupt flag after
  the scheduling call (`Primitives/VMNotifyOutOfIdle.cs:12`).
- `Entities/VMEntity.cs:297`: entity tick clears the scheduled end and primitive
  count, runs the thread, and schedules the next tick if the end remains zero
  and the entity is alive. Sleep measures elapsed ticks using the saved start,
  narrows the elapsed difference to short, subtracts before checking interrupt,
  and completes only once the counter is negative (`Primitives/VMSleep.cs:14`).
- `Engine/VMScheduler.cs:95`: RNG count mixing precedes deferred deletion.
  Pending deletion is an unsorted `HashSet` at `:10`. During a running tick,
  `Entities/VMEntity.cs:1369` queues deletion without setting `Dead`; contained
  objects may run `PrePositionChange` before being queued. A deterministic
  sorted deletion policy is an explicit correction to an unspecified source
  iteration order.
- `Engine/VMScheduler.cs:64`: after reset, the tick-1 bucket moves to the
  supplied first tick ID, replacing any existing bucket at that ID; thread
  scheduled-end fields are not changed in that method. `VM.cs:827` resets the
  scheduler and `:882` schedules every restored entity for tick 1. Thread saves
  retain `ScheduleIdleStart` but omit `ScheduleIdleEnd`
  (`Engine/VMThread.cs:1110`). Restoring an exact future calendar is therefore
  a new snapshot mechanism, not the legacy decoder's literal behavior.
- `Engine/VMScheduler.cs:112`: reset clears the calendar and current IDs,
  but does not clear the pending-deletion set or current bucket field.
- `Entities/VMEntity.cs:485`: creation schedules next tick, runs init entry 0
  immediately, runs dynamic-multitile entry 8 immediately, and pushes main
  entry 1. `Engine/VMThread.cs:343` permits 500,001 ordinary dispatches because
  the limit check uses post-increment and `> 500000`. Fault handling at `:374`
  resets callee then caller; a game-object repeat fault with dialog cooldown
  greater than 590 can delete the object, while the initial cooldown is 600.
