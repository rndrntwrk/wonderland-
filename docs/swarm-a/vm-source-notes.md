# VM, memory and primitive source notes

Source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
Owned work: W02.2 and W03, under `crates/sim-core/src/vm/`,
`src/primitives/`, and `tests/vm_*.rs` / `tests/primitives_*.rs`.
These are simulation-local Rust interfaces. They do not freeze another swarm's
content schema, interaction protocol, EOD state, or durable authority model.

## Delivered behavior and the meaning of coverage

The core executes bounded BHAV instructions, preserves the separate legacy
execution identities, resolves global/private/semiglobal calls, evaluates memory
and arithmetic, and serializes every VM frame and primitive continuation. Real
handler algorithms now cover object iteration and mutation, relationships,
functional/name calls, idle handling, fire propagation, presentation/outfit
operands, TS1 family budgets and inventory tokens. Autonomy uses the W05 scorer;
checked offers and queue admission remain B-owned inputs. Routing and animation
use the separately implemented W04/W05 providers.

This is not a claim that every registration has its complete legacy behavior.
The registry records six distinct statuses:

| Status | Meaning |
| --- | --- |
| `Implemented` | Handler control/numeric algorithm is implemented in this lane. Ordinary host reads/RNG are still required. |
| `SourceNoOp` | The pinned C# handler itself returns true without simulation changes. Only BreakPoint and TSO SysLog have this classification. |
| `HostAdapter` | The source handler algorithm is implemented and uses typed world, avatar, content or interaction providers. A missing provider returns `HostUnsupported`; it is not implicit success. |
| `Partial` | Implemented source cases coexist with explicit unsupported cases or documented approximation in the integrated provider. |
| `RequestOnly` | The eight-byte operand and execution context are transported across a typed request boundary; this alone is not a port of the C# handler's internal cases. |
| `Unsupported` | The registered handler is still explicitly rejected. TS1 MakeNewCharacter is the remaining entire unsupported handler. |

The current fresh registries contain:

| Mode | Populated slots | Implemented | SourceNoOp | HostAdapter | Partial | RequestOnly | Unsupported |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| TSO | 50 | 6 | 2 | 23 | 9 | 10 | 0 |
| TS1 | 53 | 6 | 1 | 27 | 8 | 10 | 1 |

The source has 56 registration statements: 48 common, six TS1 and two TSO.
There are 53 distinct opcode slots and 54 handler classes across those
statements. A fresh TSO registry has 49 distinct handler classes; fresh TS1 has
51. The C# process-global registry is not cleared when changing mode, so a
TS1-to-TSO transition can retain slots 3, 19 and 51. Rust deliberately constructs
an isolated registry per mode. `fresh_mode_registries_exhaustively_match_source_inventory`
checks the slot lists, mode overrides, operand aliases and missing entries.

## Exact interpreter source ledger

All anchors in this document refer to the baseline above, not a moving upstream
branch. Abbreviated C# paths start under `TSOClient/tso.simantics/` unless
explicitly prefixed by `tso.files` or `TSOClient`.

| Concern | Source anchor | Rust contract / evidence |
| --- | --- | --- |
| BHAV bodies and 12-byte instructions | `TSOClient/tso.files/Formats/IFF/Chunks/BHAV.cs:25–70` | `vm/bytecode.rs`: versions 0x8000/0x8001/0x8002 use 12-byte headers; 0x8003 uses 13 bytes and a u32 count at offset 9. Type, version, argument/local widths and all eight operand bytes are retained. |
| Primitive versus routine dispatch | `TSOClient/tso.simantics/Engine/Translator/VMRoutine.cs:24–43`; `Engine/VMThread.cs:554–582` | `opcode >= 256` invokes a routine; every missing slot below 256 returns true with a diagnostic. Opcode 255 is a missing primitive, whereas branch pointer 255 means false. The JIT's conflicting 255 boundary is not adopted. |
| Four-word subroutine operand | `Primitives/VMSubRoutine.cs:18–44`; `Engine/VMThread.cs:490–518` | Four signed words; temp substitution is enabled only when Arg1/2/3 are not all zero. The array constructor used by RunTreeByName leaves substitution disabled. A normal call allocates max(declared arguments, 4), with extra entries -1. |
| Routine scope resolution | `Engine/VMThread.cs:521–545` | IDs below 4096 resolve globally, 4096–8191 privately through code owner, >=8192 through that owner's semiglobal binding. Ordinary subcalls retain code owner, caller, callee and stack-object context. |
| Cached stack identity | `Engine/VMStackFrame.cs:30–56` | Raw signed `stack_object` and optional generation-aware `stack_object_ref` remain separate. Deleting/reusing an ID never silently retargets a cached frame; explicit assignment to StackObjectId performs the new lookup. |
| Branch/return/sentinel handling | `Engine/VMThread.cs:585–735` | True 254, false 255, error 253. Error 253 tries the instruction's non-253 true pointer, then non-253 false pointer, then propagates ERROR. Returns unwind callers; Retry discards the child Boolean; Interaction has an explicit provider boundary. |
| Frame push and empty BHAVs | `Engine/VMThread.cs:738–756` | Locals are zeroed to max(declared locals, declared arguments). The source allows zero-length actual Args; functional calls use exactly their declared argument length. An empty child fails to push and leaves the caller IP unchanged, leading to the explicit execution budget. A missing routine instead propagates ERROR. |
| Check/name execution | `Engine/VMThread.cs:92–186`; `Primitives/VMRunFunctionalTree.cs:10–74`; `Primitives/VMRunTreeByName.cs:13–67` | Typed `RoutineCheck` distinguishes EvaluateCheck from RunInMyStack, selected owner registers, caller inheritance, check state and temp/XL copyback. `CheckResult.aborting` is distinct from a routine ERROR. Source RunInMyStack's empty-stack false result is retained; leaked temporary flags are restored by the runtime. |

The old source suppresses many C# exceptions and may reset entities or display a
SimAntics dialog. The core instead returns typed `VmFault` values. It never
panics deliberately to emulate a legacy exception, starts an OS thread, performs
a wall-clock wait, or treats a missing host implementation as a successful
operation.

## Memory and expression semantics

`Engine/Scopes/VMVariableScope.cs` supplies the exact numeric scope inventory.
`Engine/VMMemory.cs:22–278` is the short read switch, `:280–291` the list lookup,
`:293–302` the big-variable read, `:364–422` packed tuning resolution,
`:466–651` the write switch, and `:653–665` the big write. `vm/memory.rs` dispatches
the source scopes 0–54 and 59, preserving source returns/throws for deprecated,
unused, invalid and read-only cases. Unknown scope numbers are explicit faults.

Registers are 20 signed i16 temps and two signed i32 XL temps. Locals and
parameters remain separate signed-short arrays. Temp-by-temp, local-by-temp,
attribute-by-temp/parameter and person-data indirections validate both indices.
The scalar accessor cannot read XL scope 42; only the big accessor can. Big writes
narrow to i16 except XL and the source money-headline scope. Literal and several
read-only setters return false, while source arithmetic intentionally ignores many
setter return values. Dynamic-sprite flags use `value > 0`, not nonzero.

Caller, callee, code owner, selected stack object and lead tile have distinct
roles. For example, packed tuning uses the callee/resource owner contract, caller
and stack persistent-ID halves use scope 59, and stack-temp scope 13 aliases the
active thread when it selects its owner, subject to the host
`entity_temps_alias(owner)` policy. An outside-tick EvaluateCheck clones ordinary
temps/XL while scope 13 still addresses the entity's actual thread. The host
therefore disables the shortcut for that separate check. `RoutineCheck` also
records `use_current_thread`: RunTreeByName destination 0 invokes the current
check thread, while the other synchronous destination invokes StackObject.Thread,
even if both carry the same EntityRef. Named execution also returns optional
`CheckResult.thread_control` (interrupt and sleep-countdown origin). With
copyback enabled, this preserves the selected thread's control state alongside
its temporary registers and last stack exit. Separate EvaluateCheck results
leave this control field absent. Lists keep source front/back/index/count
access and signed-short count narrowing. TSO type attributes read zero/write
success as in the source; TS1 delegates actual type attributes. Room-by-temp0,
clock, neighborhood, career, object-definition and engine-query values use typed
memory addresses. Their backing providers and live-state limits are documented
in `runtime-memory.md`; having a scope address is not a claim that missing legacy
neighborhood metadata has been imported.

`Primitives/VMExpression.cs:59–265,278–328` is the arithmetic anchor. All source
operators 0–20 are represented, including TSO push/pop versus the TS1 OR/XOR
aliases. Integer arithmetic, shifts, flags, division, modulo, casts and square
root use the pinned numeric module. Increment/decrement comparison retains the
promoted i32 temporary, writes a narrowed i16 value, then reads RHS; an aliased RHS
therefore sees the narrowed value while LHS comparison retains the promoted one.
The source modulo is wrapping `((a % b + b) % b)`; zero leaves the original LHS.
A one-based sign-bit test uses `> 0`, preserving its unusual false result. TSO list
pops remove the item even when writing the destination fails. The room-value
comparison's 1024 compatibility adjustment uses the callee's floor offset.

The numeric source probe is documented in `numeric-source-notes.md`. The VM
expression tests are source-derived fixtures, not a claim that the complete
FreeSO VM ran as the reference process for these cases.

## Primitive behavior ledger

| Family | Source anchors under `TSOClient/tso.simantics/` | Implemented rules and provider responsibilities |
| --- | --- | --- |
| Sleep 0 / Idle 17 / Notify 49 | `Primitives/VMSleep.cs:11–48`, `VMIdleForInput.cs:11–66`, `VMNotifyOutOfIdle.cs:12–18` | Decrement first using source u32 elapsed-tick rules and signed-short narrowing. Sleep completion consumes `NextRandom(1)`; interrupted sleep and idle do not. Idle checks push, notification, interrupt, then countdown in that order. AllowPush is exactly 1; TSO action trees cannot push but still poll each tick when the operand is 1. Notify updates a current-owner interrupt directly and uses the host hook for temporarily removed owners/ancestors. |
| Grab 4 / Drop 5 / DropOnto 43 / Remove 18 | `Primitives/VMGrab.cs`, `VMDrop.cs:15–52`, `VMDropOnto.cs`, `VMRemoveObjectInstance.cs` | Slot presence and occupancy, source target/slot operand reads, drop order N/NW/NE/W/E/SW/SE/S relative to facing, centered tile placement, deletion flags and next-tick behavior. World performs checked movement/slot attachment and deferred entity deletion. |
| FindBestObject 14 / Functional 20 / Named 28 / Interacting 37 / Push 13 | `Primitives/VMFindBestObjectForFunction.cs:70–170`, `Primitives/VMRunFunctionalTree.cs`, `VMRunTreeByName.cs`, `VMTestSimInteractingWith.cs`, `VMPushInteraction.cs:14–69` | Exact entry-point/score-variable tables, source thresholds, stable first-ID ties, surface slot fallback, broken/disabled/lockout/in-use checks and post-check position reread. Functional call allocates exact args and changes callee/codeowner. Named subcalls preserve callee and pass first four temps with extra -1 values. B owns actual interaction offers, queue lifecycle, active-action identity and enqueue admission. |
| Relationship 24/26 | `Primitives/VMRelationship.cs:13–141,149–291` | Distinct old/new layouts, direction/local/parameter selection, null-target true behavior, TSO persistent versus local selection, TS1 raw neighbor IDs, resize/fail behavior, dirty marking before column failure, source f32 increment scaling then i16 narrowing/wrapping then clamp. Default multiplier is source `?? 1f`. Runtime supplies the category tuning and normalized neighbor projection. |
| Fire 9 | `Primitives/VMBurn.cs:14–123`; `Model/VMObjectQueries.cs:49–81,258–267` | TSO enabled tuning, spread RNG, caller-pool check, percent decrement/clamp, cardinal front offset, tile-centered source fire GUID 0x24C95F99, multipart BFS, same-level/burnable/fireproof/ghost/in-use checks. BurnBusyObjects is ignored by the source. The initial absent tile uses a detached empty list; an existing tile list observes creation side effects. Missing fire creation still counts as made-a-fire, matching source. |
| Create 42 / SetToNext 31 / Type 32 / Distance 11 / Direction 12 / Terrain 63 | `Primitives/VMCreateObjectInstance.cs:14–177`; `VMSetToNext.cs:23–248,252–327`; `VMTestObjectType.cs`; `VMGetDistanceTo.cs`; `VMGetDirectionTo.cs`; `VMGetTerrainInfo.cs` | Source local/entity selection, main IDs, source cardinal direction behavior, generation-aware cache assignment, same-tile duplicate checks, object/master GUIDs, ordered iteration, adjacency and multipart/family wrap. Distances retain source units/floor offsets; direction uses source atan2 and ties-to-even mapping. Create's callback/neighbor/persistence/underneath retry cases remain explicit unsupported; terrain 0/1 requires an unimplemented semantic provider, while fixed/coordinate cases are present. |
| Suit 6 / Refresh 7 / ShowString 21 / Balloon 41 / ActionString 50 | `Primitives/VMChangeSuitOrAccessory.cs:16–164`; `VMRefresh.cs`; `VMShowString.cs:13–38`; `VMSetBalloonHeadline.cs:12–54`; `Engine/VMChangeActionString.cs:12–42` | Correct resource and string indices; typed presentation requests; source null/ignored branches; headline signed-index wrapping/algorithmic local; check-tree action string collection. Suit temp lookup resolves a byte but body/decoration type and PD8 use the ORIGINAL selector. Default daywear update skips temp selector. Accessory strings remain exact. Decoration affects presentation, never stored decoration inventory IDs. Resource interpolation and legacy tables remain content-provider responsibilities. |
| Animation 44 / Motive 29 | `Primitives/VMAnimateSim.cs`; `VMSetMotiveChange.cs` | Operand decoding, parameter-sourced IDs, source synthetic stack-object animation scope 65536, event local/parameter writes, wait/false-event/complete outcomes. Motive rate then max reads preserve order; ClearAll skips invalid scope/index reads. W05 owns timeline/event/motive state. |
| TS1 budget 25 / inventory 51 / Gosub 30 | `Primitives/VMTS1Budget.cs:9–24`; `VMTS1InventoryOperations.cs:12–166,252–289`; `VMGosubFoundAction.cs:11–25`; `tso.files/Formats/IFF/Chunks/NGBH.cs:85–112` | Optional family budget projection; source checks OLD budget for negativity, allows negative new balance, wraps arithmetic and ignores IsCheck. Inventory types are i32, GUIDs u32, counts u16. Add/removal overflow, quantity-check differences, signed find count, aliased count/index temps, checked LINQ Sum overflow and follow-token GUID 0/1 lookup versus 10/11 append bug are preserved. Unknown inventory modes still perform source pre-switch reads. B owns legacy neighborhood import/save; runtime stores trusted normalized projections. |
| Autonomy 65 and TS1 3 | `Primitives/VMFindBestAction.cs:96–335`; `avatars/autonomy.rs` and `avatar-source-notes.md` | First higher-priority queued target short-circuits before offer lookup and RNG. B provides checked offers; shared W05 scoring/select consumes host RNG, preserves first-variant duplication and byte-ID narrowing, then typed enqueue uses priority 2/normal mode/args [param0, 0, 0, 0]. No VM-specific scoring clone or invented queue is added. |
| Transfer 25 TSO | `Primitives/VMTransferFunds.cs:18–207,209–419` | Source amount-owner/short-or-XL read happens before mode/check guards; supported request modes and mode 17 zero amount are preserved. Root/E owns account mapping, authorization, category scaling, durable operation validation and completion. This remains Partial. |

### Routing cases supplied by W04

All route requests include the raw operand, frame context, parameters, locals and
temps. The VM preserves check-tree false branches before handing off and rejects
source Reach modes above 1. `runtime_routes.rs` supplies actual provider behavior;
see `world-source-notes.md` for its source ledger and end-to-end tests.

| Opcode | Current provider behavior | Remaining limits |
| --- | --- | --- |
| 16 FindLocation | Modes 0–5, source reference/local choice, ring/direction/deferred occupied ordering, inverted PreferNonEmpty, smoke midpoint, along/lateral and 100 random X-then-Y attempts. | Semantic placement/world model; full content collision corpus is not proven. |
| 22 LookTowards | Head PD41–45, camera source no-op, body 2–5 including multipart average. | Direct control 255 is explicit unsupported; heading/movement is the W04 semantic model. |
| 27 GotoRelative | On-top point without RNG, Anywhere 16–32 and directional 16–24 ranges, resolution 16 and face-anywhere/no-failure-tree flags. | Full original route/smoothing/Bezier parity is not claimed. |
| 45 GotoRoutingSlot | Full u16 parameter/literal/global scope, source facing sentinels and typed no-choice failure. | Slot/content/portal providers and W04 approximation remain relevant. |
| 46 Snap | Modes 0–4, front/container/reference cases, occupancy, direction, slot selection, checked move/detach, source placement error temps. | Shoo when blocked by an avatar requires B's queued go-to interaction. |
| 47 Reach | Modes 0/1, imported animation names and height selection, speed 1, completion-before-event, xevt0 pickup/drop and occupancy. | Height 5 custom Z offset and mouth cases remain explicit; modes >1 are source throws. |

Asynchronous routes whose caller differs from the executing thread owner are
explicitly rejected by the runtime pending a separate continuation identity
contract. The VM never turns those provider errors into success.

## Complete registration inventory

The default statuses below are the executable registry in `primitives/registry.rs`.
Lines are in `TSOClient/tso.simantics/VMContext.cs`. Common opcode 30 is overridden
in TS1 by the subsequent mode-specific registration.

### Common registrations

| Opcode | Handler | Operand class | Status | Source line |
| ---: | --- | --- | --- | ---: |
| 0 | `VMSleep` | `VMSleepOperand` | `Implemented` | 94 |
| 2 | `VMExpression` | `VMExpressionOperand` | `Implemented` | 103 |
| 4 | `VMGrab` | `VMGrabOperand` | `HostAdapter` | 112 |
| 5 | `VMDrop` | `VMDropOperand` | `HostAdapter` | 119 |
| 6 | `VMChangeSuitOrAccessory` | `VMChangeSuitOrAccessoryOperand` | `HostAdapter` | 126 |
| 7 | `VMRefresh` | `VMRefreshOperand` | `HostAdapter` | 133 |
| 8 | `VMRandomNumber` | `VMRandomNumberOperand` | `Implemented` | 140 |
| 9 | `VMBurn` | `VMBurnOperand` | `HostAdapter` | 147 |
| 11 | `VMGetDistanceTo` | `VMGetDistanceToOperand` | `Implemented` | 156 |
| 12 | `VMGetDirectionTo` | `VMGetDirectionToOperand` | `Implemented` | 163 |
| 13 | `VMPushInteraction` | `VMPushInteractionOperand` | `HostAdapter` | 170 |
| 14 | `VMFindBestObjectForFunction` | `VMFindBestObjectForFunctionOperand` | `HostAdapter` | 177 |
| 15 | `VMBreakPoint` | `VMBreakPointOperand` | `SourceNoOp` | 184 |
| 16 | `VMFindLocationFor` | `VMFindLocationForOperand` | `HostAdapter` | 191 |
| 17 | `VMIdleForInput` | `VMIdleForInputOperand` | `HostAdapter` | 198 |
| 18 | `VMRemoveObjectInstance` | `VMRemoveObjectInstanceOperand` | `HostAdapter` | 205 |
| 20 | `VMRunFunctionalTree` | `VMRunFunctionalTreeOperand` | `HostAdapter` | 214 |
| 21 | `VMShowString` | `VMShowStringOperand` | `HostAdapter` | 223 |
| 22 | `VMLookTowards` | `VMLookTowardsOperand` | `Partial` | 230 |
| 23 | `VMPlaySound` | `VMPlaySoundOperand` | `RequestOnly` | 237 |
| 24 | `VMRelationship` | `VMOldRelationshipOperand` | `HostAdapter` | 244 |
| 26 | `VMRelationship` | `VMRelationshipOperand` | `HostAdapter` | 252 |
| 27 | `VMGotoRelativePosition` | `VMGotoRelativePositionOperand` | `Partial` | 259 |
| 28 | `VMRunTreeByName` | `VMRunTreeByNameOperand` | `HostAdapter` | 266 |
| 29 | `VMSetMotiveChange` | `VMSetMotiveChangeOperand` | `HostAdapter` | 273 |
| 30 | `VMSysLog` | `VMSysLogOperand` | `SourceNoOp` | 281 |
| 31 | `VMSetToNext` | `VMSetToNextOperand` | `Partial` | 288 |
| 32 | `VMTestObjectType` | `VMTestObjectTypeOperand` | `Implemented` | 295 |
| 35 | `VMSpecialEffect` | `VMSpecialEffectOperand` | `RequestOnly` | 306 |
| 36 | `VMDialogPrivateStrings` | `VMDialogOperand` | `RequestOnly` | 313 |
| 37 | `VMTestSimInteractingWith` | `VMTestSimInteractingWithOperand` | `HostAdapter` | 320 |
| 38 | `VMDialogGlobalStrings` | `VMDialogOperand` | `RequestOnly` | 327 |
| 39 | `VMDialogSemiGlobalStrings` | `VMDialogOperand` | `RequestOnly` | 334 |
| 40 | `VMOnlineJobsCall` | `VMOnlineJobsCallOperand` | `RequestOnly` | 341 |
| 41 | `VMSetBalloonHeadline` | `VMSetBalloonHeadlineOperand` | `HostAdapter` | 348 |
| 42 | `VMCreateObjectInstance` | `VMCreateObjectInstanceOperand` | `Partial` | 355 |
| 43 | `VMDropOnto` | `VMDropOntoOperand` | `HostAdapter` | 362 |
| 44 | `VMAnimateSim` | `VMAnimateSimOperand` | `HostAdapter` | 369 |
| 45 | `VMGotoRoutingSlot` | `VMGotoRoutingSlotOperand` | `Partial` | 376 |
| 46 | `VMSnap` | `VMSnapOperand` | `Partial` | 383 |
| 47 | `VMReach` | `VMReachOperand` | `Partial` | 390 |
| 48 | `VMStopAllSounds` | `VMStopAllSoundsOperand` | `RequestOnly` | 397 |
| 49 | `VMNotifyOutOfIdle` | `VMAnimateSimOperand` | `HostAdapter` | 404 |
| 50 | `VMChangeActionString` | `VMChangeActionStringOperand` | `HostAdapter` | 411 |
| 62 | `VMInvokePlugin` | `VMInvokePluginOperand` | `RequestOnly` | 422 |
| 63 | `VMGetTerrainInfo` | `VMGetTerrainInfoOperand` | `Partial` | 429 |
| 65 | `VMFindBestAction` | `VMFindBestActionOperand` | `HostAdapter` | 438 |
| 67 | `VMInventoryOperations` | `VMInventoryOperationsOperand` | `RequestOnly` | 448 |

### Mode-specific registrations and overrides

| Mode | Opcode | Handler | Operand class | Status | Source line |
| --- | ---: | --- | --- | --- | ---: |
| TS1 | 1 | `VMGenericTS1Call` | `VMGenericTS1CallOperand` | `RequestOnly` | 464 |
| TS1 | 3 | `VMFindBestAction` | `VMFindBestActionOperand` | `HostAdapter` | 457 |
| TS1 | 19 | `VMTS1MakeNewCharacter` | `VMTS1MakeNewCharacterOperand` | `Unsupported` | 471 |
| TS1 | 25 | `VMTS1Budget` | `VMTransferFundsOperand` | `HostAdapter` | 478 |
| TS1 | 30 | `VMGosubFoundAction` | `VMGosubFoundActionOperand` | `HostAdapter` | 485 |
| TS1 | 51 | `VMTS1InventoryOperations` | `VMTS1InventoryOperationsOperand` | `HostAdapter` | 492 |
| TSO | 1 | `VMGenericTSOCall` | `VMGenericTSOCallOperand` | `RequestOnly` | 502 |
| TSO | 25 | `VMTransferFunds` | `VMTransferFundsOperand` | `Partial` | 509 |

Opcode 49 intentionally registers `VMAnimateSimOperand`, not a notify operand.
Opcodes 24 and 26 intentionally use different relationship operand classes. Those
are source facts covered by the census test, not transcription mistakes.

## Continuations, identity and runtime boundary

A `VmThread` owns serializable frames, short/XL registers, raw and cached stack
identities, source countdown bookkeeping, last exit/status, check/action state,
tree advertisements, action strings, diagnostics and at most one suspended
primitive request. `VmStop` distinguishes Ready, Sleeping, Waiting, Completed,
Faulted and BudgetExhausted. The runtime supplies current u32 source tick values
and maps wake deadlines to its outer u64 accepted-tick scheduler.

`VmThread::resume` requires the exact pending request ID, accepts one resolution,
rejects another Pending response, validates register targets/counts before
admitting it, and rejects animation events for other request families. Restoring
a resolved-but-unconsumed continuation after a zero instruction budget is legal.
All outer/embedded resolution writes share a 1,024-write limit. A staged thread
receives register writes before they are committed, so a malformed later index
cannot leave half of the earlier writes. Synchronous `CompleteWithWrites` uses
the same rule for route providers whose running owner is temporarily outside the
runtime thread map. Durable effect completion remains subject to the root's
stricter accepted-response and operation/epoch/generation checks; the general VM
response type does not grant effect authority.

`HostRequest::validate_at` binds every saved request to its immutable BHAV
instruction, including family, opcode, raw Route/External operand and kind,
check mode, static animation/motive fields and constant transfer amounts. A
matching forged effect payload cannot legitimize a request attached to Sleep or
another unrelated instruction. Mutable world-memory amount/rate values remain
latched and are not read again during restoration. Animation parameter IDs are
checked against the suspended frame's actual parameter value. Captured route
parameters/locals and external parameters must equal that suspended frame;
shared temporary registers may change during a wait, so they are not equated
with their latched request values.

Hosts observe the live thread before each instruction. After primitive/host
dispatch returns, `take_thread_control(owner)` delivers a nested call's
interrupt, countdown origin and last-exit state to a removed ancestor. This
happens before handling the current primitive's result: a later parent return
or next-tick scheduling decision must supersede the child's older exit/origin.
Direct named calls return the same control through `CheckResult`.
After every instruction,
including a terminal one, `take_temp_writes(owner)` and
`take_temp_xl_writes(owner)` transfer writes made through entity-thread views.
Their combined batch is validated before either bank changes: at most 20 short
writes and two XL writes, with all indices checked. Then
`take_interrupt_request(owner)` transfers transient notifications into the
current thread, and `take_reset_request(owner)` handles a requested reset.
Reset clears frames and continuation and completes with Interrupt. The root cancels associated operations and queues the source Reset/Main
entry-point work at its deterministic boundary. This replaces source reentrant
reset with an explicit boundary; it is documented as a behavioral policy, not
hidden inside a setter. A frame's caller/callee/stack references must be
structurally valid, but stale/deleted generation references may remain in saved
frames so later access can fail without rebinding a reused ID. The root checks
live thread ownership and operation identities.

`RelationshipBook` stores matrices, changed-persistent markers and local reverse
bookkeeping. Historical marker/reverse supersets are valid: source
`Primitives/VMGenericTSOCall.cs:377–379` clears relationship matrices without
clearing those caches. All collections are bounded before mutation, while
snapshot validation adds live-entity checks. TS1 neighbor and inventory keys are
legacy signed NIDs, not local entity references. `Ts1InventoryBook` preserves
list order and optional absence; it is a logical normalized projection, not a
legacy neighborhood parser or a disk-save implementation.

## Explicit bounds and deviations

| Boundary/policy | Enforced behavior | Relevant evidence |
| --- | --- | --- |
| BHAV intake | At most 1 MiB body, 253 instructions, 1,024 declared locals, 255 declared arguments; reject unknown versions, dangling branches, truncation and trailing bytes. | `bounded_bhav_decoder_versions_and_branch_limits`; source reader otherwise tolerates unknown/trailing data and can hold unreachable extra instructions. |
| Runtime frames | At most 256 frames, exact local allocation, up to 255 actual args, indexed reads return typed faults. | `recursive_calls_and_temp_argument_substitution_have_explicit_bounds`; `zero_argument_frames_fail_parameter_operands_with_bounds_instead_of_panicking`. |
| Instruction work | Explicit caller-provided budget and BudgetExhausted stop. | Missing/empty/recursive routine tests; root sets accepted-tick limits. This is not the source's UI exception/reset suppression loop. |
| Numeric/runtime policy | Pin unchecked widths and Mono-derived non-finite conversion behavior; reject division/remainder overflow. | Numeric reference suite and expression alias/order tests. Full FreeSO execution is not the oracle for every VM fixture. |
| Identities | Positive live IDs with generations at authority boundaries; preserve raw signed IDs and stale cached references in frames. | `cached_stack_generation_does_not_rebind_until_stack_id_is_assigned`. |
| Provider direction | Canonical clockwise notch 0..7 (N=0, E=2, S=4, W=6), not C# Direction bit masks. | `drop_and_create_host_directions_are_canonical_notches_for_cardinal_and_diagonal_facing`; root/world contract. |
| Response/import payload | 1,024 resolution writes; at most 20 short/two XL host writebacks with joint atomic validation; strict register bounds, request/instruction/frame binding and 128 diagnostics. | Synchronous/resume atomicity and paired VM/effect forgery regressions. |
| Relationship state | 65,536 matrices, 65,536 changed markers, 65,536 reverse targets/edges, 256 columns; structurally valid keys, checked before admission. | `relationship_book_caps_are_atomic_and_historical_supersets_remain_valid`. |
| Fire work | Up to 1,048,576 tiles and 65,536 group visits per primitive; invalid tile index becomes Bounds. | Fire suite, including mutation-before-source-index-failure. |
| TS1 inventories | 65,536 total items and signed-i16 neighbor keys; checked before replacement. | `inventory_caps_and_checked_source_sum_are_independent_and_checkpointable`. |
| Text/check output | Up to 1,024 action strings and 64 KiB per string; no hidden interpolation fallback. | Presentation suite; imported string/template providers remain explicit. |
| Mode registry | Fresh isolated TSO/TS1 registrations, no source static-registry leakage across mode changes. | Exhaustive census test. |

## Tests and reproducible commands

The owned suite currently has 99 tests:

| Test target | Tests | Main evidence |
| --- | ---: | --- |
| `vm_behavior` | 18 | Header formats, calls, scope/codeowner, sentinel fallback, empty/missing distinction, yields, resume/restore, live short/XL writeback, nested control precedence, reset and interrupt hooks. |
| `vm_memory` | 5 | Scalar/XL width, caller/callee/stack/tuning/lead distinctions, indirection, generation caching and separate-check bank identity. |
| `primitives_basic` | 7 | Expression order/overflow/aliases, list mutation, RNG bounds and complete registry census. |
| `primitives_entities` | 12 | Ordered searches, same-tile behavior, canonical direction, slot occupancy/drop order, create flags/cleanup and zero-argument faults. |
| `primitives_external` | 13 | Animation/motive/transfer/route operands, check guards, synchronous/resumed writes, request binding and forged payloads. |
| `primitives_relationships` | 6 | Old/new layout, persistent/local/neighbor matrices, wrapping/clamping, dirty-mark ordering and caps. |
| `primitives_behavior` | 11 | Functional/name contexts and args, check copyback, best-object source scoring, idle ordering and queue request flags. |
| `primitives_presentation` | 8 | Refresh, strings, signed headlines, action-string collection, original versus resolved suit selector and exact accessory names. |
| `primitives_fire` | 6 | Chance/pool order, multipart traversal, flags/ghosts, creation failure, absent/existing tile-list aliasing and bounds. |
| `primitives_legacy` | 9 | TS1 budget edge cases, token count overflow/underflow, aliases, source follow-token bug and checked sum. |
| `primitives_autonomy` | 4 | Queue shortcut, shared scorer RNG, selected enqueue arguments, missing-action outcome and Gosub dispatch. |

The complete 98-test release suite passed with `--locked --offline` after the
register-alias and captured-frame binding fixes. The final named-thread control
change then passed all 29 focused debug tests (`vm_behavior`: 18;
`primitives_behavior`: 11), including the added control-order regression. Both
missing-control behaviors were observed failing before implementation. The
current owned inventory is therefore 99 tests. The root integration lane runs
the final complete package debug/release gates and records the final 99-test
VM coverage within its package-wide results.

An earlier focused 46-test debug run covered memory-bank identity, current-thread
selection, atomic short/XL writeback and captured-parameter/local binding. The
independent reviewer then replayed 78 VM/primitive tests after the final control
hook: vm_behavior 18, vm_memory 5, primitives_behavior 11, primitives_entities 12,
primitives_external 13, primitives_fire 6, primitives_legacy 9 and
primitives_autonomy 4. Its run also passed all 26 snapshot and four integrated
query-alias tests: 108 total, with no failures or warnings. The snapshot/query
tests belong to the integration lanes and are excluded from the owned 99 count.
The exact independent replay is recorded in the review artifact.

```sh
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo \
CARGO_TARGET_DIR=/workspace/scratch/cb8e2814717b/toolchain/target-vm \
cargo test --manifest-path crates/sim-core/Cargo.toml --locked --offline \
  --test vm_behavior --test vm_memory --test primitives_basic \
  --test primitives_entities --test primitives_external \
  --test primitives_relationships --test primitives_behavior \
  --test primitives_presentation --test primitives_fire \
  --test primitives_legacy --test primitives_autonomy
```

Add `--release` for the same optimized suite. Package-wide, native/WASM replay,
reference-process and content-replay results are root-owned gates; this document
does not claim them from VM unit fixtures alone. Rustfmt check passed on all
owned Rust files after the final implementation. A portable-registry path change required a clean of the
reproducible target-vm cache; the final source does not depend on the previous
registry path. One release attempt stopped before executing a generated test
binary because its executable mode was missing. Rebuilding that disposable
artifact produced the successful complete release run described above; no
source behavior was changed to resolve the artifact issue.

## Independent review and remaining integration gates

The independent VM review identified and the lane fixed:

1. Zero-argument SetToNext 6/Create 7 array reads now return Bounds instead of Rust
   panics. A focused regression covers both source operands.
2. Burn's initial empty-tile list differs from an existing mutable query list;
   conditional requery and a creation-side-effect fixture preserve that behavior.
3. Failed placement cleanup for a created object sets `cleanup_all=true`, so the
   runtime removes its full multipart group.
4. A saved Route/External request must originate at its immutable instruction;
   both VM and paired VM/effect forged-payload tests cover this binding. Captured
   parameters/locals must also match the suspended frame, while shared temp/XL
   values may legitimately change during a wait.
5. Outside-tick check banks and actual entity-thread banks remain distinct,
   including named calls whose two destinations share an EntityRef. The host
   alias policy and explicit current-thread flag preserve that distinction;
   named child control state returns to the reused thread.
6. Host writes to a removed active owner or ancestor now return to the live
   short/XL banks after each instruction. Both banks validate as one transaction
   before interrupt/reset handling, including terminal instructions.

The late register/binding regressions were observed failing before their final
implementation and passed in the focused 46-test debug run.

The repository review artifact `.superpowers/sdd/IMPLEMENTATION/vm-review.md`
and root handoff record the independent review status and any residual findings.

Remaining work is explicit rather than represented by successful no-ops:

- The ten `RequestOnly` slots per mode are generic platform calls, sound/special
  effects, private/global/semiglobal dialogs, online jobs, plugin invocation and
  TSO inventory. Their typed requests and durable completion plumbing exist;
  their full operand-case algorithms require the B/C/E providers. A request
  transport or successful fixture response does not close their handler status.
- TSO TransferFunds still requires exact account/budget/tuning authority semantics
  beyond the implemented amount/guard/request boundary. Durable writes are not
  executed by the VM.
- TS1 MakeNewCharacter 19 creates/saves character resources and chooses from
  imported avatar/texture catalogs in the source. It remains Unsupported rather
  than inventing neighbors, GUIDs, resources or persistence.
- Create 42 callback/neighbor/persistence and underneath intersection-retry cases
  remain unsupported before allocation. Terrain 63 cases 0/1 remain explicit.
- B must provide live checked offers, active-action targets, queue admission and
  AttemptPush/interaction return lifecycle. The core's functional trees, idle
  ordering and W05 autonomy math are executable without hardcoded object names;
  that is not a complete interaction framework or an imported object cohort.
- W04 route approximations and the listed direct-control/custom-Z/shoo cases stay
  visible. No full original routing continuation/Bezier equivalence is claimed.
- Imported BHAV/OBJD/TTAB/STR/tuning/neighbor and save-marshalling adapters remain
  B/F integration work. `RoutineStore` and normalized projections do not silently
  stand in for an FSOv or IFF importer.
- Large content-cohort conformance against a running FreeSO reference process,
  including asynchronous scripted world effects and exception/reset behavior,
  remains a separate parity gate. The present tests prove the named contracts
  and selected edge vectors, not complete gameplay parity.
