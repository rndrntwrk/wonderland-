# Native source terrain GoTo candidate

This uncommitted candidate extends the reviewed action-outcome recovery base
`af8e11b2b3a595cd8b7028c1d724a4032f5e7c23`. It has not been deployed. The source
implementation and authored-fixture qualification are ready for independent
review. **Actual imported-content menu evaluation and Walk completion have passed
in an isolated runtime with exact authority/replica agreement. Authenticated
owned-account and rendered gameplay remain separate qualification gates.**
No proprietary behavior listings, operand bytes, asset files or private import
reports are included in this evidence directory.

## Source contract

The reference is the repository's C# implementation:

- `TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetGotoCmd.cs`: caller and
  queue-limit checks, north-facing GUID `0x000007C4` creation at the signed source
  position, then the selected local interaction with `[Param0, 0, 0, 0]`.
- `TSOClient/tso.client/UI/Panels/UILotControl.cs` (Click and Update): tile-center
  selection, `SolidToAvatars`, ghost position and `GetPieMenu(false, true)`.
  The source creates a persistent UI ghost once, then repositions it.
- `TSOClient/tso.client/UI/Panels/UIPieMenu.cs`: terrain selections send the local
  interaction index, dynamic Param0 and position; there is no global-action bit.
- `TSOClient/tso.simantics/VMContext.cs` (`CreateObjectInstance`, `SetBirthTime`)
  and `Entities/VMEntity.cs` (`Init`, `ExecuteGenericEntryPoint`, placement):
  initialize out of world, run synchronous Init and optional dynamic entry 8,
  schedule Main for real objects, perform checked placement, run real placement
  entry 9 and set the source birth fields. A ghost does not run Main or entry 9.
- `TSOClient/tso.simantics/VM.cs` (`Sandbox`, `SandboxRestore`, `AddEntity`):
  each immediate ghost callback replaces entity/query registries, starts IDs at
  one and restores the RNG afterward. The ghost is temporarily in Entities and
  ObjectsById; AddEntity excludes ghosts from ObjectQueries.
- `TSOClient/tso.simantics/Engine/VMStackFrame.cs`: a direct StackObject reference
  is distinct from setting StackObjectID, which resolves the current registry.
- `TSOClient/tso.simantics/VM.cs` (`Init`, lines 226–229): fresh simulators in
  either mode initialize global 17 to 4, global 20 to 255 and global 25 to 4.
  These defaults apply only to new state; imported/restored state is preserved.

The native player has no teleport, route-command or spawn-command surface.
Authority creates the genuine helper and queues its genuine local action.
Avatar motion, cancellation and completion remain outcomes of source behavior
and the existing queue/router.

## Admission and timing

The UI query/preparation applies the source tile-center and solidity gates,
queries a detached source ghost and chooses an exact local action/Param0 offer.
No temporary ghost identity is serialized or returned as a world target.

Authority validates the authenticated principal and controlled live avatar,
observed world/actor/queue revisions, monotonic nonzero sequence, queue count
below 20 and available local behavior content. It does **not** rerun the ghost
menu or impose the UI's tile-center/solidity rules. Coordinates and signed
Param0 pass through unchanged. Real placement runs before enqueueing; the
ordinary queue runs CheckAction at its normal deferred time.

Missing GUID, creation metadata, local definition/routine, placement support or
required executed behavior fails closed. Work is staged in the existing tick
transaction, so a failed source command cannot leave allocated helpers, consumed
IDs/RNG, ownership records or partial placement behind. The new transient helper
gets the source OwnerID/ObjectFlags zero defaults only; imported entities are not
rewritten and persistent ID zero is not registered as a persistent group.

**Explicit native difference:** an unknown local action or unavailable behavior
is atomically refused. The C# GetAction-null path may return an accepted command
without adding an action. This candidate intentionally does not reproduce that
accepted/no-op result. A command receipt proves acceptance, not successful route
completion; deferred checks may still decline the action.

## Ghost support boundary

Ghost callbacks run in detached state with empty registries and ID one, and the
RNG is reset after each callback. Main, world query registration and placement
callbacks are suppressed. Direct ghost access and numeric ID re-resolution are
covered separately; ordinary VM hosts retain the previous raw ID-read behavior,
including stale saved IDs.

The implementation currently reconstructs a singleton ghost for each menu query.
It refuses callback movement, containment, additional objects or asynchronous
effects that cannot be represented faithfully by this helper. This is not a
claim of generic persistent-ghost equivalence. The private original destination
Init and direct-call TTAB check closures have been inspected; no direct check
reads birth/time/RNG or writes the helper. Dynamic named-tree calls still require
runtime branch evidence. Nonzero actor CurrentObject (PersonData 76) is refused
by the terrain query with an unsupported-selection-mode diagnostic: source
selection checks may mutate SocialTargetID (75), which detached UI evaluation
cannot yet preserve. This is an explicit remaining source-support gap, not an
authority admission restriction or a claim that the mode is editor-only.
Primitives that expose entity references outside the memory hooks also need to
be considered against the actually executed original scripts.

## Append-only ABI and recovery

- `PlayerAction::Goto` is appended as tag 2; Invoke/Cancel tags are unchanged.
- `AcceptedCommand::SourceGoto` is appended as tag 30; older tags and snapshot
  migrations remain covered by the existing suites.
- Goto payload is 61 bytes, WLC1 envelope 77 bytes. Its fixed-width fields are
  principal u64; actor slot u32/generation u32/revision u64; world u64; queue u64;
  sequence u64; x i16; y i16; level i8; interaction u16; Param0 i16.
- Absolute WLC1 offsets: variant 16, principal 20, actor slot 28/generation 32/
  revision 36, world 44, queue 52, sequence 60, x 68, y 70, level 72,
  interaction 73, Param0 75.
- The existing exact-byte pending/Unknown/durable receipt/recovery path is reused;
  no speculative avatar movement or command resubmission is introduced.

Rust and edge tests share this encode/decode vector: principal 1; actor
(slot 2, generation 3, revision 4); world 5; queue 6; sequence 7; position
(24, 40, 1); local interaction 7; Param0 -2:

```text
574c43310d0a1a0a3d000000000000000200000001000000000000000200000003000000040000000000000005000000000000000600000000000000070000000000000018002800010700feff
```

## Verification

`sim-core-tests.log` records `cargo test --offline -p sim-core` with one build
job: **472 passed, 0 failed, 1 ignored**. The ignored test is the existing Mono
avatar oracle, which requires mcs/mono. All twelve authored terrain regressions
pass, including ghost registry/RNG/direct-ID isolation, real lifecycle order,
zero-ID ownership/snapshot restoration, identity/stamp failures, unknown/missing
content, placement atomicity, queue 20, cancel/replay, source-timed deferred check
and exact negative Param0 with zero argument tail. The suite also covers the
selection-mode guard, new-runtime defaults in both VM modes, preserved imported
and restored globals, and owned-admission startup defaults. Shared VM, routing
and prior snapshot suites also pass. `sim-core-source-sha256.txt` identifies the files.

The final settled-core/content boundary rerun passes 24 native-player, 14
admission, 8 recovery, 12 host and 4 terrain-pick tests. All 11 owned-admission
tests also pass, including the fresh source-global startup assertions. Logs are
retained in `boundary-tests.log`, `host-tests.log`, `web-terrain-tests.log` and
`owned-startup-tests.log`. `boundary-source-sha256.txt` identifies the boundary
files. `wasm-check.log` records the successful web-shell wasm32 check. The
existing proc-macro-error2 future-compatibility warning is unrelated.

Matching Rust 1.99 tooling passes formatting for all eight changed packages,
strict Clippy (`--all-targets -- -D warnings`) across those packages, and strict
web-shell WASM Clippy. `format-check.log`, `clippy-check.log` and
`wasm-clippy-check.log` retain the results. The only core lint correction replaces
Option::is_none_or with equivalent map_or(true, ...) to respect sim-core's
existing Rust 1.75 MSRV. `final-terrain-tests.log` records all twelve regressions
passing after that correction. Other lint fixes are a cooker type alias, a
loader test initializer and web module ordering; no behavior was changed.

`source-query-tests.log` retains the earlier focused pass for 15 external-primitive
and 11 terrain tests. The final full-core run includes those additions plus the
startup regression. Exact synchronous TSO generic queries 48 (null/dead object)
and 51 (temporary persistent identity) no longer incorrectly create asynchronous
effects while evaluating the original global menu checks.

## Private original-content progress

The private importer has supplied actual TemplatePerson/destination behavior,
effective local/global interaction tables, OBJD/SLOT metadata and tuning.dat.
Actual TemplatePerson initialization and its next Main tick run. With
CurrentObject zero, the full terrain query (including global checks) returns
Walk Here, Run Here, Face Here and Point Here. Two queries produce identical
offers and leave the complete authoritative state unchanged.

An actual Walk command creates and initializes the real destination helper and
queues its source action at tick 3. The source idle behavior yields, Walk starts
at tick 6, and the avatar moves from (24, 24, 1) to (56, 56, 1). At tick 17 the
runtime reports RouteFinished Success and the Walk action finishes Succeeded.
The original idle action resumes. Authority/replica outcomes and complete state
match at every tick through tick 302, with no external effects. The loop continues
past Walk completion because the source idle action remains in the queue.

The preceding failed probe used native all-zero fresh globals. Comparing the
pinned VM.Init established the missing source defaults above; global 25 enables
the genuine idle action that accepts ordinary queued input. Correcting new-state
initialization made the original behavior progress without editing the script,
forcing dispatch, ending Main or skipping checks. Fresh genesis state hashes
therefore change with this source correction. Release/bootstrap identities must
identify the candidate, and existing snapshots must not be rewritten.

The first fresh probe constructs SimRuntime::new; a second loads the sealed
935-resource release through PreparedRelease::load and GameRuntime::from_cooked.
Both create the actual TemplatePerson and assign interaction authority. It is an isolated behavior probe, not proof
of authenticated owned-account admission. GameRuntime and native startup use the
same SimRuntime::new path; an authored owned-admission test checks the defaults.
The raw artifacts, decoded behavior reports and detailed execution trace remain
in the private import workspace. See ORIGINAL-RUNTIME.md for sanitized evidence.

Pending gates: nonzero selection-mode side-effect support; proper original-content
owned-account admission; source walking/turn animation timeline dispatch; final
independent exact-candidate review. A complete original-content two-account
rendered journey is not established by the authored tests or by these isolated
successful movement probes.
