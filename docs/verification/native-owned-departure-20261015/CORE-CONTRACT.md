# Native owned source departure: candidate contract

Status: actual original-content source departure, durable account save/release, host takeover and genuine re-entry are qualified by the fresh v5 witness. Final targeted native regressions, strict Clippy, matching formatting and the current wasm32 web-shell check passed. See `ORIGINAL-ROUNDTRIP.md` for the bounded result and remaining gaps.

## Authority and retained observations

A private `RequestOwnedLeave` command appends at discriminant31. It binds an authority ID to the exact installed `OwnedControl`, including actor generation, account/avatar, control generation, session, admission operation, reservation, and frozen export. Its operation is derived from authority/avatar/control generation. The actor loses interaction authority before source leave begins; ownership remains installed until actual source removal. Exact retries do not repeat initialization or change the immutable capture.

The first source leave tick (`KillTimeout == 1`) captures the source portable values and outgoing dirty persistent relationships after avatar tick mutations, before end-of-tick removal. The host exports only the reviewed database whitelist. A prior committed capture survives later ticks and recovery. Save observation precedes detach observation in the actual runtime event stream. Native capture is speculative until its containing proposal commits.

The removal hook requires a retained first-leave capture, source removal reason, known-clear EOD proof, and no unresolved external effects. It does not turn a generic Delete into source leave. Source mode25 deletion is `source_leave`; actual source deadline cleanup is separately `forced_cleanup`. Missing source action/check behavior is never treated as successful departure.

## Source behavior

Pinned source references: `VMAvatar.UserLeaveLot`, `VMAvatar.Tick`, `VMGenericTSOCall` mode25, `VMEntity.Delete`, and `VMScheduler`.

- Resolve actual self local interaction173, including nonzero action/check routine bindings.
- Recognize action routine8373 only in the active prefix or first pending position (`index <= active_len`).
- Snapshot queue IDs before cancellation. Use the original user-driven priority50 and SkipPermissions, without inventing a privileged maximum-priority action.
- Preserve actual deferred check timing, including SkipPermissions bypass unless RunCheckAlways is set. Source false is distinct from provider failure.
- Mode25 cancels peers' queued target/icon references and waits for active references. It inspects all peer frames, including detached currently executing thread views, for callee/direct stack-object references.
- Once source mode25 allows departure, request source UserLeaveLot and schedule deletion at end of tick. Do not remove the actor while its executing frame still owns it.
- Tick lifecycle continues to use the source reducer. Generic external waits have no qualified BlockingState timeout adapter and therefore block owned departure rather than receive fabricated replies.

## Explicit native safety boundaries

The original VM catches exceptions and resets/clears selected stacks; it does not roll back world/RNG. Native ordinary owned execution remains whole-tick failure-atomic.

For an exactly fenced, installed departing actor only, typed unsupported create/terrain adapter failures (opcodes42/63) restore the complete pre-dispatch SimState and TickWork. That includes peers, world, RNG, scheduler, queues, effects, events, detached-thread ownership, and any speculative capture. The durable departure records `source_execution_fault` and the opcode. Other actors can continue. No successful save/detach is inferred. Deliberate source reach-mode throws, generic host strings, invariant/identity/content errors, and capacity/budget errors are not absorbed by this boundary. Final post-scheduler snapshot validation still runs before commit.

Queue check/provider errors use a broader type that can mix unsupported behavior with invariants/limits. Any RuntimeFailed queue event for a departing actor therefore rejects the whole tick. A plain false check still follows source retry/deadline behavior. A future typed check-error boundary is a remaining support item, not silently approximated here.

## EOD and external-effect proof

A durable lot-wide `eod_known_clear` fact starts true only at fresh native genesis. Legacy snapshots migrate false/unknown. Any InvokePlugin request, including one originating on another object/invoker, permanently invalidates this conservative fact until a real session-association/reconciliation adapter is supplied. An empty current outbox does not establish absence of a session.

Queue and lifecycle EOD disconnect requests are retained once per exact owned actor/control generation. They are requests, not teardown acknowledgements. Ordinary cancellation and mode25 peer cancellation can carry these consequences without pretending those peers departed.

Generic effects do not expose full participant associations. Any pending external effect therefore blocks owned removal, even when its target differs from the departing actor. No universal success, cancellation reply, or unrelated-session disconnect is manufactured.

## Persistence and remaining qualification

Departure state appends after stable route schema5 as schema6. Legacy migration preserves original state and creates empty departure records with unknown EOD history (17-byte empty append). A schema5 regression compares the entire migrated state and the subsequent resave. Already-departing legacy avatars without a first-leave capture require authentic journal recovery or repair; no later state is guessed as their first save.

The actual sealed-original leave/save/detach/re-entry and takeover gates are recorded separately below. Final targeted Rust regressions, matching formatting/lint and current wasm32 checks passed. The exact committed source identity is recorded with this verification set. The complete rendered two-account journey and general external/EOD reconciliation remain outside this bounded proof. Authored harnesses alone do not qualify those gates.

## First genuine placement gate (2026-10-15)

The shipping sealed loader and fresh schema6 genesis used the existing original-content binding. Two genuine account admissions completed through the private runner. The accepted checkpoint at tick3 showed actor1 at `(56,56,1)` and actor2 at the source out-of-world sentinel. Both original avatars retained ObjectFlags264 and KillTimeout=-1. Source globals `[4,255,4]` and fresh EOD proof were verified. The harness failed before issuing leave.

The source join path uses a matching mailbox when present; without one it checks placement at tile `(3,3,1)`. The authored empty lot had no mailbox, so the second placement collided. This requires source-valid lot initialization and another fresh proof. Original flags/positions were not changed. Earlier two-control API evidence did not prove two in-world actors. Private checkpoint and source-content payloads are retained only in the approved private workspace.

## Placement reporting and original mailbox anchor

The additive private Inspect projection reports actual committed actor positions and computes `in_world` from lot bounds. It is joined to the exact actor generation, principal, avatar and control generation. It never grants ownership or changes placement. The runner withholds action tickets when a valid admitted actor is out of world and retains custody while returning `admitted_awaiting_placement`.

The focused Inspect regression passed, and all nine owned recovery tests passed, including placement suspension with an unknown action across unchanged and newer host incarnations. The actual original no-mailbox HTTP gate also passed: the second avatar's enter and reconnect both returned awaiting-placement without a ticket; the checkpoint, registry and actor placements stayed unchanged.

A separate fresh authored lot now includes the genuine original mailbox object, initialized and positioned through the source lifecycle. The original two-account admission gate passed: avatar1/actor2 was at `(24,24,1)` and avatar2/actor3 at `(24,56,1)`, with distinct authenticated ownership, exact committed placement rows, and both enter/reconnect responses ready. No original avatar flags or coordinates were rewritten. The first leave HTTP request then failed before source drain. A fresh retry followed the corrected exact-registry identity path.

A second fresh anchored run reached actual source leave. Its immutable first-leave capture at tick7 was committed and applied to the account authority with all 24 whitelisted columns, then survived host takeover. At tick9, source global routine472 instruction2 requested GenericTSO mode40 (`TestStackObject`). That source operation is a synchronous direct-reference null test; its missing native adapter incorrectly produced an external continuation. The runtime safely blocked departure as `unreconciled_external_effect`. Retained storage contains one committed/applied source save and zero detach or release receipts. The candidate now implements the exact synchronous predicate; it does not synthesize a response for the already-retained effect or rewrite that blocked fixture. The later fresh v4/v5 runs passed after the exact synchronous adapter correction; the old blocked fixture remains unchanged.
