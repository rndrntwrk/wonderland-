# Casino and social EOD independent review

Date: 2026-10-05. Reviewer: `games_review`, under `eod_completion`.

## Disposition

The review found two material casino settlement defects, one Band recovery UI
defect, a missing social recovery dependency fence, and a bounded floor integer
overflow. The two casino defects and Band defect now have focused actual-host
regressions with observed RED and GREEN results. The coordinator inspected all
three production corrections independently. The native reviewer corrected the
floor overflow with an observed RED/GREEN component regression; the coordinator
implemented and tested the dependency fence in the host.

No additional confirmed blocking casino/social defect remains in the reviewed
paths after these corrections. This is a bounded source and native-host review,
not a qualification of the production account provider, VM adapter, graphical
client, or complete original runtime. The coordinator owns final aggregate
acceptance, strict Clippy, census, and public documentation. Its latest cleanup
and service additions were still under final verification when this report was
written; those final gates are not claimed here.

The initial assignment was review-only. After findings were delivered, the
coordinator expressly authorized the targeted casino/Band fixes and the bounded
original helper fixture below. I authored those corrections, so their final
independent source inspection belongs to the coordinator, not to this report.
No staging, commits, ref changes, original C# edits, or asset edits were made by
this reviewer.

## Reviewed authority and source basis

- `.superpowers/sdd/REMAINING/authority.md`.
- `docs/swarm-b/eod-casino.md` and `docs/swarm-b/eod-social.md`.
- The original author reports `eod-casino-report.md` and
  `eod-social-report.md` in this directory. Their 9/23 acceptance counts are
  historical author snapshots; the suites now contain 14/24 tests.
- `crates/eod-runtime/src/plugins/casino/**` and
  `crates/eod-runtime/src/plugins/social/**`, with emphasis on gameplay
  transitions, settlement, pending operations, private recovery, peer reads,
  callbacks, teardown, and hostile input bounds.
- Actual-host tests in `crates/eod-runtime/tests/casino_handlers.rs` and
  `crates/eod-runtime/tests/social_handlers.rs`; component tests beside the
  handlers; social oracle sources/adapters/manifest/expected trace.
- Original C# at commit
  `4c6b3e8f5835b228723caea3c9f683c62f244f73`, chiefly the corresponding
  `TSOClient/tso.simantics/NetPlay/EODs/Handlers` files, original deck helper,
  and source UI handlers where event order determines visibility. This was
  not an exhaustive execution or line-by-line audit of the entire original
  application.

The intended native corrections were evaluated against their documented
contracts. In particular, private hole-card views, debit-gated reveal, bounded
roulette geometry/capacity, native deterministic RNG, accepted-tick timers,
`WAR-LAST-PIECE-PROGRESS`, and retained settlement refusal are not reported as
original-equivalence defects. Preserved source quirks include odd-bet rounding,
ten-value splitting and split naturals, freshness's missing append, the buzzer's
first-three auto-enable loop, floor text-bank offset, source dance UID behavior,
and source integer wrapping.

## Findings and corrections

### G1 — High: Slots settled an unrevealed outcome after a late debit

**Affected path:** `casino/slots.rs`, `BET` receipt handling (currently around
line 438), `leave`, `shutdown`, and `validate`.

**Contract:** `docs/swarm-b/eod-casino.md`, settlement/teardown paragraph
(line 58 in the reviewed snapshot): a debit completing after teardown must
produce a durable refund. This is the declared native monetary policy.

Before the correction, leaving during `DEBITING` retained the provider request
and set the member disconnected. A successful late receipt entered `SPINNING`
and immediately finished the selected spin for that disconnected member. A
losing spin therefore consumed the stake after departure; a winning spin paid
the already selected outcome. Neither followed the native refund contract.

**Correction:** successful late debits now enter a distinct private `REFUNDING`
phase and request the exact original stake from object to avatar. No reel
result or normal outcome settlement is emitted. The refund remains immutable
across a terminal refusal, checkpoint, and controller-authorized retry. The
validation rules bind its kind, phase, recipient, and amount to the retained
member and original bet. A successful refund completes cleanup and permits
reuse when only the player departed.

**Actual-host regressions:**

- `slots_late_debit_after_teardown_refunds_instead_of_settling_the_selected_spin`.
  Native seed 0 supplies losing stops `[3, 1, 1]`; seed 2 supplies winning
  stops `[1, 5, 3]`. Each places a five-unit stake, loses the successful debit
  reply, tears down, checkpoints/restores, receives the late receipt, and
  verifies a checkpointed refund and restored wallet. The winning case also
  refuses the refund once, restores again, rebinds the controller, and retries
  the retained obligation. It asserts no spin/win/loss output after teardown.
- `slots_late_debit_after_player_departure_refunds_before_reusing_the_machine`.
  Covers participant-only departure, the delayed refund, and admission of a
  replacement player after completion.

The first regression observed RED as zero pending refund preparations where
one was required. Both tests are GREEN in the 14-case acceptance run. These are
native deterministic seeds; no original `System.Random` seed parity is implied.

### G2 — High: late split/double/call debit invalidated a closing table checkpoint

**Affected path:** `casino/table.rs`, `abort` (currently around line 3203), late
`SPLIT`/`DOUBLE`/`CALL` receipt handling, and `MOVE_PENDING` validation.

Before the correction, table teardown marked the game closing but retained
`phase == MOVE_PENDING`. A successful late extra-debit receipt consumed the
last pending move request and prepared the additional refund. The table still
required a pending split/double/call operation for `MOVE_PENDING`, so host
validation returned `InvalidCheckpoint`. This prevented the committed debit
from reaching the durable refund path.

**Correction:** `abort` now enters `CLOSED` and clears the queued gameplay
phase immediately. Existing immutable financial requests and per-player
contributions remain retained. The existing closing-state validation permits
their late receipts; the ordinary `MOVE_PENDING` invariant remains strong for
an active game. No later gameplay move or animation is resumed after close.

**Actual-host regressions:**

- `blackjack_late_split_debit_after_teardown_keeps_the_refund_checkpoint_valid`.
- `blackjack_late_double_debit_after_teardown_keeps_the_refund_checkpoint_valid`.
- `holdem_late_call_debit_after_teardown_keeps_the_refund_checkpoint_valid`.

These use actual authenticated players, a paid initial bet, a committed extra
debit with a lost reply, controller teardown, two checkpoint/restore boundaries,
late receipt, and final settlement. They verify a valid checkpoint, restoration
of the original wallet, retained exact transfers, and absence of new gameplay
animation after close. All three observed RED with `InvalidCheckpoint` at the
late receipt and are GREEN after the phase correction.

### G3 — High: retained social dependencies did not fence silent recovery ticks

**Source-family anchors:**

- `social/club.rs`: `Controller.floor`, `Controller::tick`, `Dj.club.controller`,
  `Dj::tick`, `Platform.club.controller`, and `Platform::tick`.
- `social/buzzer_host.rs`: retained selected contestant slots and `Host::tick`.
- The peer readiness check originally ran only when an emitted action addressed
  another group, while local readiness covered the current group only.

The smallest observed source path needs a nightclub controller and floor in
one cluster, one NPC dancer, and a saved active round at tick zero. After
restore, rebind only the controller. `Controller::tick` sees its saved floor
reference and advances RNG/ticks and emits a `ForceInteraction`. Its round-view
messages target DJ/platform groups, not the detached floor, so an emission-only
floor fence cannot catch that transition. DJ freshness/rating timers, platform
freshness/timers, and the first 29 buzzer countdown subframes have analogous
private transitions with no necessary peer message.

**Disposition:** delivered early to the coordinator and native reviewer. The
native reviewer added retained-dependency getters. The coordinator's
`native_host::ready` now checks the transitive retained dependency closure with
a visited set, including each peer's native controller and recorded members.
Unrelated groups may progress independently, including unrelated groups in the
same cluster. This implements the clarified native policy.

The coordinator reported GREEN for these three actual-host regressions in
`tests/native_recovery.rs`:

- `retained_floor_fences_silent_club_rng_ticks_and_native_callbacks_after_restore`.
- `buzzer_countdown_subframes_wait_for_required_player_controller_and_member`.
- `transitive_club_dependency_fences_dj_but_unrelated_same_cluster_property_progresses`.

I inspected the dependency getters, closure, and test paths. I did not run
these Cargo tests myself; their integrated execution and the subsequently added
authoritative teardown case belong to the coordinator/native review handoff.

### G4 — Medium: Band rebind hid the saved active game and omitted skill setup

**Native anchor:** `social/band.rs`, `Band::rebind` (currently around line 405).

**Original source anchors:** `UIBandEOD.cs` lines 506/525 (`UIInitHandler`),
693 (`GotoWaitForPlayerPhase`), 334/336 (`ShowGameHandler`), and 744
(`GotoBandGame`); `VMEODBandPlugin.cs` lines 174 (`Band_Show`) and 410
(`Band_Game_Reset_Skill`).

The original UI initialization returns to the lobby and hides all game
buttons. `Band_Show` is the source event that enters the game display. The
native rebind emitted `Band_UI_Init` for every phase, but replayed `Band_Show`
only in preshow and never sent the saved combined skill. Rehearsal,
performance, electric, and intermission messages do not reveal the hidden
controls. Consequently a restored active game remained visually in its lobby.

**Correction:** for every non-lobby phase, rebind emits the saved skill and
`Band_Show` after UI initialization and before phase-specific controls. The
existing skill string formatting was extracted into a shared helper. No phase,
song, note index, timer, RNG, or payout transition is changed by rebind.

**Actual-host regression:**
`band_rebind_shows_saved_game_before_phase_controls_without_advancing_it`
constructs and restores all eight phases: lobby, preshow, rehearsal,
performance, electric, intermission, minimum payment, and finale. It checks
the source-required event ordering for each participant, exact saved skill,
absence of rebind-generated payout/native commands, and equality with an
uninterrupted host's next tick UI/object outputs. It observed RED on missing
skill replay and is GREEN with the other four Band tests.

This qualifies the replay messages and server continuation. A graphical
production UI recovery walkthrough, including a client resuming partway through
a wallclock rehearsal animation, remains outside this test boundary.

### G5 — Medium: valid extreme floor frame could overflow the circle radius

**Native anchor:** `social/floor.rs`, the two offset circle radii (currently
lines 348/353). **Original anchor:** `VMEODNCDanceFloorPlugin.cs` lines 410/411,
which use C# unchecked integer addition for the same radii.

Private validation accepts an integer frame up to `i32::MAX`. A restored
64-by-64 floor with diagonal 91, `frame = i32::MAX - 27`, discovered state,
circle random animation 2, and zero tock passes validation. Its next draw
previously evaluated `frame + diagonal / 2` with ordinary Rust addition and
panicked in the debug build. The nearby source circle arithmetic already
intentionally preserves wrapping.

**Disposition:** the native reviewer changed only the two additions to
`wrapping_add`. Its `restored_circle_animation_wraps_radius_near_frame_limit`
regression covers `MAX - 27` and `MAX - 44`, both selection branches, a private
save/restore, full expected raster, frame advancement, and the emitted native
graphics command. The reviewer reported observed RED at the addition and
GREEN for all three focused floor component tests. I inspected the correction
and regression; I did not rerun that shared-target turn.

This is a bounded private-state robustness failure, not a claim that ordinary
short game sessions reach the extreme counter.

## Bounded original casino helper oracle

The coordinator requested this additional evidence after the findings. It is
implemented in `fixtures/eod/casino/`:

- `sources.json`: all eleven original whole-file SHA-256 hashes and pinned
  commit, plus exact source slice bounds/hashes and disclosed scaffolding.
- `SourceHelperOracle.cs`: calls the original helper implementations.
- `expected-helper.txt`: 35 checked-in literal trace lines.
- `verify.py`: validates original hashes and exact pinned Git bytes, extracts
  verified source slices, compiles/runs twice, compares literals, and exercises
  eight changed-expected comparator controls.
- `helper-report.json`: generated verification report and explicit exclusions.
- `README.md`: reproduction, scope, source extraction, controls, and limits.

The complete original `AbstractPlayingCardsDeck.cs` is compiled unchanged.
From `VMEODBlackjackPlugin.cs`, two verbatim slices are compiled: the value
dictionary at byte interval `[1156, 1698)` (source lines 36–51), and the complete
`BlackjackPlayer` plus following enums at `[91444, 111143)` (lines 1783–2274).
Their whole-file and slice hashes are in the manifest and generated report.
The temporary wrapper contains imports/namespace/static-class containers and
an empty opaque `VMEODClient` field type. It does not alter method bodies or
provide simulated VM, transport, account, or UI behavior.

The trace covers circular/discard deck invariants, six-deck multiplicity,
card names/copy/shorthand, soft/ace/natural/split/double state, win/loss/push,
odd natural and insurance arithmetic, double settlement, exact split ordering,
split natural/double settlement, and split aces. Deck random order is deliberately
unobserved. Two executions matched all literals; all eight altered expected
results were rejected. These are comparator controls, not mutated-handler
execution controls.

Fresh command executed by this reviewer:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 fixtures/eod/casino/verify.py \
  --report fixtures/eod/casino/helper-report.json
```

Result: PASS, 11 verified original files, two identical executions, 35 lines,
eight controls. Trace SHA-256:
`004a4d28c1913dad4747c6a10ae6dd00d2b45e6555a871f12fb9301f1e146cc9`.

This does not execute complete casino handlers, the original `HoldemHand`
evaluator, Roulette or Slots, the graphical client, production VM/provider, or
original-versus-native RNG sequences. It is not described as full runtime
differential qualification. Native poker combinatorics and host acceptance are
separate evidence.

## Verification evidence and ownership

Commands use Cargo 1.90.0 with incremental compilation and debug info disabled.
My shared-target turns were explicitly coordinated and released; no root
aggregate or competing Cargo build was started.

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  /root/.cargo/bin/cargo test --manifest-path crates/eod-runtime/Cargo.toml \
  --test casino_handlers \
  --target-dir /workspace/scratch/378e4c36af7b/eod-remaining-coordinator-target

CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  /root/.cargo/bin/cargo test --manifest-path crates/eod-runtime/Cargo.toml \
  --lib plugins::casino \
  --target-dir /workspace/scratch/378e4c36af7b/eod-remaining-coordinator-target

CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  /root/.cargo/bin/cargo test --manifest-path crates/eod-runtime/Cargo.toml \
  --test social_handlers band_ \
  --target-dir /workspace/scratch/378e4c36af7b/eod-remaining-coordinator-target

PYTHONDONTWRITEBYTECODE=1 python3 fixtures/eod/social/verify.py
```

Observed by this reviewer:

| Verification | Result |
|---|---|
| Actual-host casino acceptance after fixes | 14 passed, zero failed; 0.19 seconds execution |
| Casino components after fixes | 30 passed, zero failed; 5.50 seconds execution |
| Five-card category enumeration within that suite | All 2,598,960 distinct five-card hands checked against independent combinatorial counts |
| Actual-host Band subset after fix | 5 passed, zero failed; 0.43 seconds execution |
| Bounded casino helper oracle, final rerun | PASS; two identical traces, 35 lines, eight comparator controls |
| Existing social original C# oracle, independently rerun | PASS; 16 unchanged compiled original source files and original MonoGame assembly; two identical traces; six comparator controls |
| `rustfmt --check --edition 2024` on my five owned Rust files | Exit 0 |
| Python AST parse and whitespace check on owned fixture/Rust files | PASS |

The independently rerun social trace SHA-256 is
`9bb8d1f387947be047ed0ee813877d876494cdd4a8059ecd0c3bb64d5163d4ab`.
Its six controls cover the combat graph, original final-piece stall, payout,
freshness bytes, raster, and buzzer window. Its adapter limitations remain as
described in the fixture; this is original handler semantics evidence.

Separately, the coordinator reported its combined run GREEN for casino 14,
social 24, service 26, and the first three recovery tests before subsequent
service/cleanup additions. The native reviewer reported floor 3/3 GREEN.
Those are attributed integration results, not my independent Cargo execution.
An interim Band run had expected unused dependency-getter warnings while the
host consumer was being implemented. No global Clippy claim is made here.

## Review boundaries and documentation follow-up

The account/provider boundary remains explicit: production must implement
authenticated accounts, durable request/result deduplication, the private
checkpoint barrier and trusted latest-stamp rollback protection, actual VM
registers/callbacks/queue observations, and recipient-private UI delivery. The
native tests use clearly labeled test ledgers and inspect real emitted commands;
they do not turn a requested VM payment or animation into a fabricated success.

Social peer validation checks cached identities against public peer metadata.
It does not independently reconcile every reciprocal private claim or cached
host/player score scalar inside a hostile rewritten checkpoint, because that
context does not contain the other handler's private state. Atomic peer
admission and trusted private checkpoint storage remain required, as the author
already disclosed. The new dependency fence addresses readiness, not a claim
to have removed this trust boundary.

The reviewed privacy paths preserve recipient-specific Blackjack/HoldEm views,
mask dealer/opponent cards at the documented stages, retain private song/RNG
state, and bind callbacks to native authority. Pending financial operations
are validated against immutable accounts/amounts and canonical contributions;
terminal payout/refund refusal freezes progression. The corrected paths above
were the material discrepancies found in that contract.

Two public-doc refreshes were sent to the coordinator: update the casino host
acceptance count from 9 to 14 and describe the bounded helper evidence; change
the floor animation-code 3/4 sentence to say only **color** updates. In original
`VMEODNCDanceFloorPlugin.S_SetAnimation` (lines 136–145), the early return occurs
before `Direction` assignment. Native code already matches that behavior.

## Reviewer-authored files

Production and actual-host regressions:

- `crates/eod-runtime/src/plugins/casino/slots.rs`.
- `crates/eod-runtime/src/plugins/casino/table.rs`.
- `crates/eod-runtime/tests/casino_handlers.rs`.
- `crates/eod-runtime/src/plugins/social/band.rs`.
- `crates/eod-runtime/tests/social_handlers.rs`.

Evidence and report:

- `fixtures/eod/casino/SourceHelperOracle.cs`.
- `fixtures/eod/casino/expected-helper.txt`.
- `fixtures/eod/casino/verify.py`.
- `fixtures/eod/casino/sources.json`.
- `fixtures/eod/casino/helper-report.json`.
- `fixtures/eod/casino/README.md`.
- `.superpowers/sdd/REMAINING/eod-games-review.md`.

The native host dependency closure, social dependency getters, floor wrapping
fix, and `tests/native_recovery.rs` are coordinator/native-reviewer work and
are intentionally not included in this authorship list.
