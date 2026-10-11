# Independent native host and service review

Date: 2026-10-05. Reviewer: `eod_completion/native_review`.

This began as an independent, read-only review of the coordinator's native host,
checkpoint, provider, and service implementation. Findings were sent to the
coordinator immediately; fixes in progress are distinguished from verified fixes.
The coordinator subsequently assigned this reviewer the small immutable social
dependency getters needed for NR-7 and the focused Floor overflow regression and
two wrapping additions in NR-8. Those are the only implementation changes made
by this reviewer. No host/service implementation, commits, or ref changes were
made. The reviewer ran only the focused Floor RED/GREEN checks after receiving
the shared Cargo target, then released it to the coordinator.

## Authority and scope

Read `authority.md` and `eod-kernel-contract.md`. Examined
`crates/eod-runtime/src/native_host.rs`, `native_checkpoint.rs`,
`native_provider.rs`, the native integration in `host.rs`,
`plugins/service/{mod,types,wire,simple,draw,wardrobe,trade}.rs`, and
`tests/service_handlers.rs`. Compared service behavior with the C# handlers and
supporting serializers at original pin
`4c6b3e8f5835b228723caea3c9f683c62f244f73`.

The coordinator's already-known RED tests for frozen trade operation equality and
wardrobe pending stage/object/writer identity are excluded from the independent
findings below. Production providers and the authoritative VM adapter remain
outside this implementation; the review assumes the specified whole-request,
whole-result durable deduplication and current-host fencing contract.

## Findings

### NR-1 — P1: detached closing groups could consume VM command continuations

At review start, `drive_native_provider` deliberately allowed closing groups to
finish their financial and persistence bookkeeping without requiring `ready`.
`Emission::drain` guarded detached `Action::Object { target: Controller, ... }`,
but its `Action::Command` arm admitted commands without the corresponding
controller attachment guard.

Concrete path: a dresser lists two outfits in the same category, queues deletion
of the current default, and records its alternate default in stage 13. The member
then disconnects; the group closes and detaches while preserving the pending
delete. After checkpoint and restore, returning successful `DeleteOutfit` emitted
`NativeCommand::SetOutfit`, consumed the immutable provider request, and allowed
the group to be reaped before any controller rebind.

This is a native VM continuation, not bookkeeping-only completion. It must remain
replayable until its authoritative controller can accept the command. Relevant
implementation anchors: `native_host.rs::drive_native_provider`,
`Emission::drain`'s `Action::Object` and `Action::Command` arms, and
`wardrobe.rs::reply` stage 13. Source semantics: `VMEODDresserPlugin.cs`,
`DeleteOutfit`, especially the `VMNetSetOutfitCmd` at lines 49–61 and provider
deletion at line 64.

**Fixed and regression verified.** The `Action::Command` arm now rejects a
detached controller with `PluginNotReady`, preserving the original prepared
request. The reviewer inspected the guard and
`pending_dresser_default_delete_reserves_avatar_and_fences_detached_vm_commands`.
The coordinator's `service-review-green.log` records that actual-host regression
passing, including replay of the exact request after controller rebind.

### NR-2 — P2: impossible PropertySelect pending state could panic restore

At review start, `Simple::restore` accepted a pending record when its `kind`
matched the enclosing simple handler, the optional seat was bounded, and the
callback was unique. `Simple::validate` bounded pending counts and callback IDs
but did not reject PropertySelect (`kind == 2`) having a pending operation.

`native_checkpoint::validate_state` then called
`Simple::pending_operations`, whose match handled only kinds 0, 1, 3, and 4;
kind 2 reached `unreachable!()`. A bounded but impossible private PropertySelect
record therefore panicked instead of returning `InvalidCheckpoint`.

Reproduction: encode a PropertySelect simple record with `next = 2`, one pending
callback 1, and pending kind 2. Decode through `NativeHost::restore_from` (or call
the private restore and pending-operation validation path). No oversized input is
needed. An unknown pending kind must fail closed before operation derivation.

**Fixed and regression verified.** The coordinator added pending-domain checks,
cooldown mode/time/identity checks, and host-context checks. The reviewer inspected
those changes and the successful coordinator-run log for
`property_snapshot_cannot_invent_an_unhandled_provider_kind` and
`cooldown_snapshot_validates_mode_datetime_and_account_identity`.

### NR-3 — P2: denied initial wardrobe data load left a permanent partial dialog

`Wardrobe::start` creates stage 0 `LoadPluginData` using avatar 0 and seat 0.
The original `ProviderDenied` branch in `Wardrobe::reply` removed the callback,
set `failed` only for stage 14, and closed only when the pending avatar still
matched an active roster member. Stage 0 can never match an active member because
native avatar IDs must be nonzero.

Consequently, denying the initial name load left `name_loaded = false`, no
pending load, no retry marker, and an open rack/dresser group. Rack name
initialization never occurred; an owner rename remained `PluginNotReady`
indefinitely. The typed terminal denial had neither terminal cleanup nor a
recovery path.

Reproduction: create and join a wardrobe group, checkpoint its initial requests,
return `Reply::ProviderDenied` to `LoadPluginData`, then attempt normal rack name
initialization or owner rename. The group should close or enter an explicit
retryable state. This follows the frozen kernel contract's terminal-denial rule;
source initial loading is in `VMAbstractEODRackPlugin.cs`'s constructor and
`Tick`.

**Fixed and regression verified.** The live branch now closes on
`p.stage == 0 || active`. The actual-host test
`dresser_name_load_denial_terminates_instead_of_leaving_an_unrecoverable_dialog`
passes in `service-review-green.log`; it drains both prepared load/list requests,
observes terminal `eod_leave`, and confirms the group has been reaped.

### NR-4 — P2: delayed default replacement can overwrite a later dresser choice

`Wardrobe::reply` stage 12 freezes an alternate outfit asset in `p.default` before
dispatching `DeleteOutfit`. Stage 13 unconditionally applies that frozen default
after successful deletion, including after the original member left. While the
original delete was pending, `join_native` permitted that avatar to join a
different dresser group because it only checked live instance/roster identities.

Reproduction: avatar has default A and alternatives B and C. Dresser X queues
deletion of A and freezes B; its provider response is delayed. Disconnect X and
join dresser Y. Set the default to C, or delete B while retaining C. Complete X's
deletion and rebind its controller as required by NR-1. X's late `SetOutfit(B)`
overwrites the newer default C and may select an outfit already deleted by Y.

The original `VMEODDresserPlugin.cs` changes the default before dispatching the
delete, so later dresser changes win. Moving replacement after the deletion
acknowledgement avoids a default change on failed deletion, but needs a matching
concurrency rule.

**Fixed within the native host/adapter contract; regressions verified.** Stage 12
now freezes the original outfit and a replacement for every clothing deletion.
Stage 13 emits `SetOutfitIfCurrent` with the deleted asset as `expected`.
`ObserveDefaultOutfits` provides a trusted current-default observation. A pending
default-changing deletion reserves that avatar against all new native group
joins; the reservation is derived from the retained private pending record and
therefore survives restore. The source/private pending validator requires the
replacement presence to match the clothing category.

The reviewer inspected these changes and the two successful actual-host tests
`pending_dresser_default_delete_reserves_avatar_and_fences_detached_vm_commands`
and `late_dresser_delete_compares_original_asset_and_preserves_newer_vm_default`.
This closes the second native dresser race. The latter test explicitly applies
the conditional adapter predicate; there is no implemented production VM adapter
or cross-database atomicity claim. The accepted VM barrier must compare the actual
current default and validate replacement ownership. Independent inventory writers
remain subject to that provider/adapter contract. NR-9 below records a separate
same-asset inventory edge in the strengthened validator.

### NR-5 — P2: remaining reachable-state checks for restored cached service data

Two narrower fail-closed gaps were reported while the coordinator was extending
the validators:

- Wardrobe cached `outfits` was restored using individual `Outfit::valid()` only.
  Its enclosing validator did not require the expected owner namespace or unique
  outfit IDs. Mutating a cached outfit owner to another avatar/object therefore
  survived restore and the cache was emitted by `rebind`. This concerns retained
  reply data, not just pending request equality.
- Draw allowed `mode == 4` with a recorded member, and allowed `member = None`
  with a nonempty host roster. The first case was accepted by restore but its
  `rebind` reached `initialize`'s `InvalidPluginInput` arm, leaving a supposedly
  valid snapshot unrecoverable. Mode 4 is only the pre-join sentinel; joined
  native input selects modes 0–3.

The coordinator has now tied a live dresser cache to the exact member avatar,
clears the cache on departure, and only fills it from active list replies. The
live Draw/Wardrobe validators require member/roster cardinality agreement; Draw
also rejects joined mode 4 and inconsistent load/save initialization state. These
changes have been inspected. The coordinator's 73 library tests and 26 service
host tests pass. That log does not contain a dedicated corrupt-cache or joined
mode-4 test, so direct adversarial regression coverage of these narrower guards
is not claimed.

### NR-6 — P2: a final revision could commit but remain impossible to acknowledge

Draw and Wardrobe accept a loaded revision of `u64::MAX - 1`. A subsequent save
at that expected revision can receive `Saved { revision: u64::MAX }`, which
passes the reply's `checked_add(1)` equality check but fails the family validator
requiring `revision < u64::MAX`. The provider effect has already committed, but
every replay produces an acknowledgement that the host rejects.

This is a low-likelihood counter boundary, but it is reachable through a bounded
provider reply and should be rejected before enqueueing the write. Alternatively
the terminal maximum revision could be supported consistently. Relevant anchors
are `Draw::queue_save`, `Wardrobe::queue_name`, their `Saved` reply arms, and
their revision validators.

**Fixed; Draw host regression verified.** Both enqueue functions now reject a
next revision equal to `u64::MAX` before creating a provider operation.
`terminal_revision_is_rejected_before_a_durable_write_is_dispatched` passes in the
coordinator's service log. It covers Draw at revision `MAX - 1`; the identical
Wardrobe enqueue guard was source-inspected but has no separate direct test in
that log.

### NR-7 — P1: retained peer dependencies could advance while still detached

Jointly identified with `eod_completion/games_review`. `native_host::ready`
checked only a group's own controller and members. The peer emission guard
checked readiness only when an `Action::Peer` actually targeted a peer. That
left output-silent state transitions and transitions that read a retained peer
without emitting to it able to advance during partial cluster restoration.

Minimal actual-host reproduction: create one NC_FLOOR and one NIGHTCLUB in the
same cluster, start a nightclub round with `dancers = [400]`, drain and
checkpoint, restore under a newer epoch, then rebind only the NIGHTCLUB native
controller and tick once. `club::Controller.floor` still references the detached
floor. `Controller::tick` sees the retained nonempty link, advances its RNG and
tick, and can emit `ForceInteraction`. Its only VIEW peer targets are DJs and
platforms; neither exists in this reproduction, so no peer action checks the
floor. Relevant family anchors are `club.rs`'s `Controller.floor`,
`Controller::tick`, and `Controller::broadcast`; the host anchors are `ready`,
`tick_groups`, and the peer arm of `Emission::drain`.

A second reproduction uses a READY BuzzerHost restored with its own controller
and member rebound while a recorded contestant remains detached. The first 29
ticks increment private `tock` without emitting any peer action. After the player
rebinds, one tick consumes a full countdown second. DJ and platform ticks also
read cached controller views while mutating freshness/timing without necessarily
emitting to their recorded controller.

The gate must consider required retained dependencies before family
tick/message/native-event execution, independent of whether that particular
transition emits a peer action. Unrelated clusters can remain independent. This
is stronger than the rejected all-host readiness concern. Reported to the
coordinator, who confirmed the issue and is implementing the host gate and
actual-host nightclub/buzzer regressions.

At the coordinator's request this reviewer added
`social::State::required_peers(&self) -> Vec<InstanceId>` as seven small leaf or
dispatch methods in `social/{mod,buzzer_host,buzzer_player,club,floor}.rs`:

| Family state | Required retained peers |
| --- | --- |
| War / Band | None; all participants already belong to the same native group. |
| Buzzer player | Its recorded host. |
| Buzzer host | Occupied player slots, excluding unselected discovered contestants. |
| Nightclub | Its recorded floor. |
| Floor | Its recorded nightclub controller. |
| DJ / dance platform | The controller in its retained club view. |

The host should walk the transitive closure with a visited set because
host/player and controller/floor links form legitimate cycles. Rebind and cleanup
must remain possible while peers are detached. Getter source was inspected and
compiled during the focused Floor checks. The coordinator implemented the
cycle-safe closure and recorded three observed RED tests followed by three GREEN
results in `native-recovery-red.log` and `remaining-host-green.log`. They cover
silent nightclub RNG advancement, buzzer countdown subframes, transitive DJ
dependencies, rejected native callbacks, and an unrelated PropertySelect group
continuing in the same cluster. The reviewer inspected the implementation and
test results.

The first closure implementation also called transitive `ready()` before every
peer action. After restoring a linked floor and
club, rebind only the club and invoke authoritative `disconnect_invoker` for the
unbound floor. Its shutdown sends `DETACH` to the bound club, but the gate follows
the club's retained floor dependency and rejects delivery before the handler can
clear that dependency. This blocks abandonment of the unavailable object and
worked with the previous recipient-local gate. Reported immediately to the
coordinator. The club's own shutdown emits an ending VIEW before DETACH, so
classifying only the literal DETACH signal would not cover all source cleanup.

**Cleanup follow-up fixed with observed RED then GREEN.** The peer admission arm
now captures `closing_source` from the native group. Trusted shutdown actions use
recipient-local readiness, allowing them to clear the detached dependency;
ordinary peer transitions still require the full transitive closure. The
recipient itself must have its controller and all members bound. The reviewer
inspected this narrow change and the successful coordinator-run test
`authoritative_teardown_can_clear_a_detached_dependency_from_its_bound_peer`.
It covers both detached Floor-to-bound Club and detached Club-to-bound Floor,
confirms the retired group is reaped, and checkpoints after cleanup. The logs
`native-recovery-cleanup-red.log` and `native-recovery-cleanup-green.log` record
the expected initial `PluginNotReady` failure and the subsequent four-test
recovery suite passing.

### NR-8 — P2: a valid restored circle frame overflowed before drawing

Found by `eod_completion/games_review`; the coordinator assigned the focused fix
to this reviewer. A 64 by 64 floor has diagonal 91. `Floor::validate` accepts
`frame = i32::MAX - 27`, `animation = 2`, `random_animation = 2`,
`discovered = true`, and `tock = 0`. The next tick enters circles without choosing
a new animation. The plain `frame + diagonal / 2` in the first-radius calculation
overflows before the modulo. `MAX - 44` exercises the corresponding second-radius
branch. The unchanged pinned source is
`VMEODNCDanceFloorPlugin.cs::DrawAnimation`, Circles lines 405–411; its integer
addition has unchecked wrapping semantics.

**Fixed with observed RED then GREEN by this reviewer.** The new component test
`restored_circle_animation_wraps_radius_near_frame_limit` saves and restores both
valid states, verifies each complete 4096-pixel native graphics raster, and checks
the next frame/tock. The first run failed at `floor.rs:348` with
`attempt to add with overflow`. The only production edits replace both radius
additions with `wrapping_add`. All three focused floor tests then passed.

Logs: `eod-remaining-logs/floor-overflow-red.log` and
`eod-remaining-logs/floor-overflow-green.log`, under
`/workspace/scratch/378e4c36af7b/`. The final command used
`cargo test --manifest-path crates/eod-runtime/Cargo.toml --lib plugins::social::floor::tests`
with the authorized no-debug/no-incremental environment and shared target.

### NR-9 — P2: accepted same-asset outfit records conflict with delete validation

The strengthened stage-13 validator rejects a replacement asset equal to the
deleted outfit's asset. However, the admitted `Reply::Outfits` domain requires
unique outfit IDs, not unique assets, and stage 12 chooses its replacement by
different outfit ID only. A valid list with avatar-owned outfit IDs 11 and 12,
both asset 101 and category 0, therefore queues a stage-13 record that its own
validator rejects. Applying the prepared list reply rolls back and every exact
replay encounters the same inconsistency.

The pinned source `VMEODDresserPlugin.cs::DeleteOutfit`, lines 38–59, counts outfits
in the category and chooses a different `outfit_id`; it permits the same asset.
Replacing default 101 conditionally with the still-owned asset 101 is harmless.
The implementation should support that source-valid case or declare and enforce
a stricter unique-asset inventory contract consistently at the input boundary.

**Fixed with coordinator-observed RED then GREEN.** The coordinator agreed with
the source behavior, removed only the asset-inequality predicate, and added
`dresser_distinct_owned_records_with_the_same_asset_keep_a_valid_default`.
The reviewer read `dresser-duplicate-asset-red.log`, where the accepted list reply
failed with `InvalidCheckpoint`, and `dresser-duplicate-asset-green.log`, where
the same actual-host test passes. The conditional 101-to-101 command preserves
the default and ownership remains through outfit record 12.

## Confirmed design decisions and non-findings

- The coordinator clarified that independent groups may progress after restore;
  an all-host restoration gate is not required. Within a cluster, required
  detached dependencies must remain fenced. NR-7 identifies why an emission-only
  peer gate is insufficient for that narrower requirement.
- Provider execution may finish bookkeeping-only operations for closing groups
  while detached. NR-1 is specifically about VM output continuations.
- Provider requests are prepared only after a successful private checkpoint;
  request IDs retain their origin epoch while execution separately receives the
  current host identity. Reply application clones host state and commits after
  output admission. A rejected reply application leaves the same immutable
  operation available for provider replay.
- Checkpointing rejects pending native commands, public events, or private UI
  outputs. The documented adapter must apply these at the matching VM barrier;
  merely draining queues is not a durable VM commit.
- Participant identity and roster matching, native object/plugin matching,
  callback uniqueness, request equality with family pending state, private size
  bounds, origin epochs, and current receipt identity are checked at the host
  boundary. Strengthening family reachable-state validation complements these
  host checks.

## Minor source-parity/documentation note

The new secure-trade `m0` behavior clears the money offer and resets acceptance.
The pinned C# `VMEODSecureTradePlugin.cs`, case `'m'`, only performs an update
when `amount != 0`, so zero is a source no-op. The new regression intentionally
asserts the improved behavior. Record this as a deliberate approved deviation,
alongside property slot-zero duplicate rejection, bounded malformed inputs,
atomic financial transactions, and replacement of dresser defaults only after
successful deletion. No request was made to reintroduce the source bug.

## Verification boundary

The reviewer read the coordinator's `service-review-green.log`: 73 library tests
and 26 actual-host service tests passed. The subsequent
`remaining-host-green.log` records 14 casino, 3 recovery, 26 service, and 24 social
host tests passing. The final inspected service run, `service-final-host.log`,
records 28 service tests passing, including the same-asset regression and the
initial-service-read recovery matrix. `native-recovery-cleanup-green.log` records
all four recovery tests passing after the teardown change. The same-asset and
teardown regressions each have observed RED results in their dedicated logs.
These are coordinator-run results; they are not represented as an independent
reviewer rerun. The reviewer independently ran the Floor acceptance RED and the
subsequent three-test GREEN result. The shared target was then released to the
coordinator. No aggregate workspace suite was run by this reviewer.

All actionable findings from this review are addressed in the inspected working
tree. Narrower untested NR-5/NR-6 guards are explicitly qualified above. The
production provider and accepted-barrier VM adapter remain implementation
boundaries; the tests establish native host behavior and the specified adapter
predicate, not real database/VM integration. Final integration gates remain with
the coordinator.
