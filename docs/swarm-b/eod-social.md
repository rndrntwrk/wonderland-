# Native social and music EOD handlers

This slice implements eight registered FreeSO EOD handlers behind the real
`wonderland-eod-runtime::NativeHost`. Games, timers, controller callbacks,
participants, peer claims, private random streams, and output admission run in
the same native host transaction. The buzzer host and player are separate
registered instances; the nightclub controller, floor, DJ station, and dance
platform likewise communicate through host-resolved, cluster-scoped peers.

| Registered handler | Plugin ID | Native module |
|---|---|---|
| WarGame | `0x2D642D39` | `social/war.rs`, `social/war_rules.rs` |
| Band | `0x8ADFC7A2` | `social/band.rs` |
| GameshowBuzzerPlayer | `0x00001005` | `social/buzzer_player.rs` |
| GameshowBuzzerHost | `0x00001006` | `social/buzzer_host.rs` |
| DJStation | `0x6C5C7555` | `social/club.rs` |
| NCDanceFloor | `0x6D113845` | `social/floor.rs`, `social/font.rs` |
| NightclubController | `0xCCC5BC43` | `social/club.rs` |
| DancePlatform | `0xEC55D705` | `social/club.rs` |

## Source authority and evidence

The original-source revision is
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. All eight handlers, the abstract
buzzer handler, the shared lobby, source enums, event model, string serializer,
freshness tracker, font, and associated UI handlers were inspected before the
translation. The source files and assets are unchanged.

[`fixtures/eod/social/sources.json`](../../fixtures/eod/social/sources.json)
records SHA-256 values and pinned Git blob IDs for 23 original inputs:
16 C# files compiled by the oracle, the shipped MonoGame assembly used for its
actual graphics math, and six source UI handlers. The main source paths are:

- [`TSOClient/tso.simantics/NetPlay/EODs/Handlers`](../../TSOClient/tso.simantics/NetPlay/EODs/Handlers):
  `VMEODWarGamePlugin.cs`, `VMEODBandPlugin.cs`,
  `VMEODAbstractGameshowBuzzerPlugin.cs`,
  `VMEODGameshowBuzzerPlayerPlugin.cs`, `VMEODGameshowBuzzerHostPlugin.cs`,
  `VMEODDJStationPlugin.cs`, `VMEODDancePlatformPlugin.cs`,
  `VMEODNightclubControllerPlugin.cs`, `VMEODNCDanceFloorPlugin.cs`.
- [`Handlers/Data`](../../TSOClient/tso.simantics/NetPlay/EODs/Handlers/Data):
  `VMEODFreshnessTracker.cs`, `VMEOD3x5Font.cs`, and
  `VMEODGameCompDrawACardData.cs` for the original BinaryWriter string encoding.
- [`Utils/EODLobby.cs`](../../TSOClient/tso.simantics/NetPlay/EODs/Utils/EODLobby.cs)
  and [`UI/Panels/EODs`](../../TSOClient/tso.client/UI/Panels/EODs) for seating,
  handler names, byte widths, callback shapes, and the original button encodings.

The executable oracle uses the unchanged C# handlers. Its small adapters model
only transport, VM queries, and command recording. Unsupported adapter I/O
throws. No adapter reports a fabricated provider result or claims to execute a
complete lot. In particular, it links the original shipped MonoGame assembly
instead of replacing its vector/matrix implementation.

The literal source trace covers all 25 WarGame pairings; the unequal-final-piece
source stall; Band's payout milestones and decimal midpoint rounding; the actual
freshness-history bug and float bits after 30 ticks; floor fill, blink, heart,
rainbow, and font bytes; the buzzer's first-three auto-enable behavior; and its
29th/30th tick lock transition. The verifier runs the trace twice and rejects
six independently changed expected results. See
[`report.json`](../../fixtures/eod/social/report.json).

## Native interface and trust boundary

Create a controller with `connect_native(NativeCreateRequest { object, cluster,
invoker, input: NativePluginInput::Social(config) })`. The returned
`NativeControllerTicket` authorizes only that native controller, plugin, object,
host scope, and epoch. Admit a UI participant using `join_native`, its
authenticated `ConnectionAuthority`, and a `NativeJoinRequest` containing the
controller's instance address and native member snapshot. Normal `receive`
uses the resulting `SessionTicket`, plugin ID, and sequence counter.

The following configuration is native input. None of it is parsed from an EOD
UI payload.

| Configuration | Source interpretation and admission |
|---|---|
| `WarGame` | Two unique roles: 0 blue, 1 red. |
| `Band { seed, names }` | Four unique instrument roles: 0 trumpet, 1 drums, 2 guitar, 3 keyboard. Names are authoritative avatar-ID lookups. Skills 0/1/2 are charisma/body/creativity in integer hundredths, each 0–1000. Trumpet uses charisma, drums body, guitar and keyboard creativity. |
| `BuzzerPlayer { names }` | One participant, role 0. Native register 0 supplies the initial signed 16-bit score. The host's edits and judgments apply the source's 0–9999 bounds. |
| `BuzzerHost` | One participant, role 0. Claims independently registered player instances in the same trusted cluster. |
| `DjStation { seed, group, tile_x, tile_y }` | Group 0–3 and station location come from the native object. One optional role-0 UI participant. |
| `NCDanceFloor { seed, tiles }` | Native tile snapshot: positive unique object IDs and unique coordinates; at most 4096 tiles in a 64×64 bounding box. One optional role-0 viewer. |
| `Nightclub { seed, portals }` | No UI participants. Native portal IDs are positive and unique, with at most 4096. Round-start callbacks supply at most 128 positive, unique NPC dancer IDs. |
| `DancePlatform { group }` | Group 0–3; one optional role-0 participant. The native avatar queue supplies interaction observations. |

Names have at most 16 entries, positive unique avatar IDs, at most 128 UTF-8
bytes per name, and no NUL. Native object IDs, avatar IDs, invokers, skills, role,
and cluster membership are authoritative inputs. Member seats are host routing
indices and are never treated as instrument or team numbers.

`deliver_native_event` accepts the typed `NativeVmInput::Social` variants below.
It validates the controller ticket and plugin before dispatch. A callback is
never synthesized by calling the handler's own completion path after an output.

| Native input | Original Simantics callback |
|---|---|
| `WarNextRound`, `WarNextGame` | 10, 11 |
| `BandAnimationsFinished` | 4 |
| `BuzzerPlayerSync` | 100 |
| `BuzzerHostJudgmentFinished`, `BuzzerHostDeclareWinner` | 101, 102 |
| `FloorDiscover`, `FloorAnimation`, `FloorRatings` | 1, 2, 4 |
| `NightclubRoundStart`, `NightclubRoundEnd` | 2, 3 |
| `DanceQueueObservation` | Native queue observation; deliberately has no source event code |

UI allowlists retain exact source event names and binary/text modes. Examples
include the one-byte `WarGame_Piece_Selection`, `Band_Note`, and `Band_Decision`;
the buzzer's little-endian 32-bit seat indexes and 16-bit numeric settings; and
the DJ/dance `press_button` text commands. String arrays use the source
BinaryWriter-compatible 7-bit UTF-8 length prefixes. VM argument arrays remain
signed 16-bit values in their source order.

### Actual effects

Source object events appear in `take_public_events()` as
`PublicVmEvent::NativePlugin { invoker, plugin, code, args }`. They address the
specific native controller or participant that owns the source interaction.
They carry no secret future notes, random state, answers, claims, or checkpoint
bytes.

Floor updates and portal hiding are real typed
`NativeCommand::BatchGraphics { objects, graphics }` values. NPC dance hints
are real `NativeCommand::ForceInteraction { caller, callee, interaction }`
values. `take_native_commands()` associates each command with the actual group,
plugin, and invoker. Host admission includes these commands and all UI/object
events in the same accepted transaction. Empty raster cells are omitted from
the object list, matching the original VM batch command's behavior for object
ID zero, while preserving the internal rectangular screen.

The production VM adapter is responsible for consuming these native outputs
and supplying genuine completion/queue observations. These tests prove the
registered handlers and host command boundary; they do not claim full original
client rendering, NPC animation execution, or a deployed live lot. The social
family has uninhabited external-provider operation/reply types: Band's source
payment path requests the controller's payment events, and does not pretend that
a bank transfer has succeeded.

## WarGame

Each player begins with one artillery, cavalry, command, infantry, and
intelligence piece. The byte order is exactly 0 through 4 in that order.
Artillery defeats infantry and cavalry; cavalry defeats intelligence and
command; command defeats artillery and intelligence; infantry defeats cavalry
and command; intelligence defeats infantry and artillery. The source oracle
checks every ordered pair, including equal-piece ties.

The lobby initializes each participant's color and avatar-object ID, publishes
the opponent, resets the board, and emits remaining blue/red counts. Once both
players have selected, the handler removes the losing piece and emits the exact
victory/defeat binary pair. Round completion asks native controller event 1 to
animate the result; callback 10 checks game completion and opens the next round.
Game-over event 2 retains the source winner packing. Event 3 resets win counts.

An equal-piece selection emits stalemate and enters a 150-accepted-tick wait.
Early selections cannot bypass that wait. Invalid or already removed nonempty
selections choose the first remaining piece, preserving the source's fallback.
An empty payload is rejected safely. Disconnect resets the game rather than
leaving a half-selected turn. Rebind reconstructs the board's removed pieces and
only resumes a participant whose current choice is still open.

**`WAR-LAST-PIECE-PROGRESS`** is an explicit native bug fix approved for this
slice. Original callback 10 emits no UI or controller output when both players
have one different piece left. The source oracle reproduces that zero-output
stall. Native callback 10 emits `WarGame_Resume` for those remaining pieces.
An actual-host test plays a legal eight-round path to that state, completes the
deciding ninth round, and verifies one final game result. Piece ordering,
remaining counts, result packing, and the equal-piece timer are unchanged.

## Band

The four-instrument lobby starts only when all four native roles are present.
The state machine implements lobby, preshow, rehearsal, performance,
intermission, electric animation, minimum payout, and finale. A queued state
transition occurs on the next accepted tick, which matters to the source's
timer and native-callback ordering.

Preshow, per-note performance, and intermission use ten-second timers. The
source's rehearsal `System.Timers` wait becomes `45 × (sequence length + 2)`
accepted 30-Hz ticks. Creation of the rehearsal consumes its first tick;
performance starts on the following queued transition. Wallclock pausing,
process speed, and an unbound recovered controller cannot advance the song.

Each game privately generates 25 notes. Source buzz eligibility retains
`Next(1, 32767) <= 72`, and ordinary notes retain values 1–8. Buzz and ordinary
note draws have separate private replay streams. Only the current prefix is
sent in `Band_Sequence`; future notes remain private. A participant may play
buzz or its instrument's pair. Another instrument's note is ignored, while a
legal wrong note triggers the source failure. Successful notes synchronize the
UI and controller in their original order.

At intermission, two affirmative votes continue; more than two negative votes
expire the timer; missing choices become affirmative at timeout. Decisions sent
by admitted roles outside intermission retain the source's behavior and are
cleared when the next rehearsal starts. Failure before five completed notes
loses the round. Failure after a five-note milestone waits for the real
animation callback before paying the rounded-down milestone.

| Completed notes | Base payout |
|---:|---:|
| 0 | 0 |
| 1 | 40 |
| 5 | 624 |
| 10 | 1454 |
| 15 | 2684 |
| 20 | 4564 |
| 25 | 7344 |

The source's skill payment uses decimal-style midpoint-to-even rounding,
implemented from integer hundredths to avoid float drift. Native winning
outputs retain controller event 5 (length), event 6 (skill payout), then event
3 (base payout). Failure requests controller event 2. A complete actual-host
25-prefix game verifies the final 7344 payout and skill output.

**`BAND-PAYOUT-CALLBACK-ONCE`** prevents a duplicate animation callback from
issuing minimum-payout events twice before the already queued finale transition
executes. The regression first observed a second successful callback, then
passed after admission checks the queued transition. A rejected duplicate
produces no UI, payout request, random draw, or sequence consumption.

## Gameshow buzzer host and players

The host discovers at most 16 potential player instances and selects four
slots. Discovery descriptors use the host-resolved source roster for avatar ID
and avatar-object ID. A player claim belongs to an actual host group, and only
that host can change the player. Signals address the receiver's host-resolved
destination group; an unrelated descriptor cannot substitute its own routing
identity. Search uses a bounded private visit revision in place of source
random/time-based session stamps. Left/right movement wraps around four slots.

The master state machine is Disabled → Ready → Engaged → Locked, with Expired
and explicit judgment/reset paths. The initial buzzer timer is ten seconds and
answer timer twenty seconds; native accepted ticks provide the 30-Hz clock.
First buzz identifies the answerer, immediately asks the host controller to
acknowledge that avatar, and opens a 30-tick late-buzz window. Late buzzes in
that window receive the original two-byte false value. Once locked, remaining
enabled contestants receive the original one-byte false value and their native
other-buzzer event. Timeout and answerer-disconnect follow their source paths.

Judgments preserve auto-disable, auto-enable, auto-deduct, clamping, and source
callback ordering. The initial global score is 100; editable scores are
0–9999; time limits are 2–120. The source's **first-three auto-enable loop** is
preserved: slot 3 is not automatically enabled by a correct answer. Source
configuration restrictions apply while Ready, not as a generalized UI lock.
The no-change error is the original 32 zero bytes; no-new-player error is text
`31`. Numeric underflow/overflow and callback payloads keep their exact event
names and widths.

Player score UI updates occur when the score changes. Source player controller
event 5 occurs immediately for direct host score edits, or when native callback
100 synchronizes the player controllers. Other-answer reactions and loser
reactions remain privately queued, at most 32 per player, until that callback.
The answerer's own judgment reaction is immediate. Declaring a winner updates
the host/other players and asks the host controller to animate; callback 102
then issues the actual winner's event once. Callback 101 sends the round-restart
UI event.

Controller teardown releases every selected player's claim before the host
group is removed. Replacement hosts can discover and claim the surviving
players. Player departure removes its descriptor and slot; departure of the
answerer immediately makes the remaining active players and host react to the
missing answer and disables the round. Native recovery restores score, master,
timer, buzz state, host options, slots, and pending reactions without inventing
completion callbacks.

## Nightclub controller, DJ, and dance platform

The nightclub controller hides native portals with graphics value 255 and
discovers one floor. It receives a native NPC snapshot at round start, privately
shuffles those NPCs, chooses four DJ target patterns, resets station/platform
state, and broadcasts its current round view through the peer channel. Floor
discovery works in either creation order and binds the original controller.

Every 1800 accepted ticks, the controller chooses three dances from 0–23. The
source's exclusion check includes both newly changed entries and old entries
that have not yet been changed in this pass. NPC hints choose those dances with
20/35/45 weights. Their interval grows from five ticks to thirty ticks over the
section. A hint emits a genuine `ForceInteraction` with the NPC as caller and
callee and interaction ID `dance + 4`. The round percentage retains the source
`ticks / 9000f` calculation. Round end is an explicit native callback. A round
has a bounded maximum of 128 sections; exhaustion is rejected before mutation.

DJ patterns are four categories of three digits, each 0–3. Reset draws the
patterns, marks ratings dirty, and staggers the first rating by `group × 45`
ticks. Subsequent ratings preserve the source post-decrement interval: 151
ticks. Pattern indexes are `16a + 4b + c`. Closeness uses the original squared
distance; rating combines closeness, freshness, and round percentage with
midpoint-to-even rounding and a ceiling of 100. Correct patterns send directional
arrow particles to the actual floor, using the station-to-floor-center angle;
warming/cooling sends the source line/colder feedback.

The DJ `press_button` text is category, digit index, and value. Categories 0 and
1 retain the source attribute swap in controller events 10/11; categories 2/3
retain their order. The emitted `dj_active` string preserves four pipe-separated
three-digit patterns. Digit index 3 is ignored safely: the original check
accepts it and then indexes outside a three-element array.

The dance platform observes the native avatar's actual active interaction UID.
It accepts only observations for its admitted avatar, and counts a completed
dance when a previously observed UID changes. Valid dance interactions are
6–29 on this platform in an active queue. The source's implementation counts
UID changes even though a comment claims cancellations are excluded; the native
port preserves the executable behavior. No UI payload can submit an interaction
observation or a correctness/rating result.

Correct dances set three section flags with weights 55/25/20. The platform uses
the source square-root percentage, freshness weighting, historical section
average, and round percentage, with rating capped at 100. Each new correct
dance sends floor rectangle feedback; completing all three adds the two
delayed rectangles. Periodic ratings preserve the source 151-tick interval.
The dance UI's buttons send controller event IDs 2–25: the original UI adds one
to its internal 1–24 dance button number. The handler preserves the source
byte-sized text parser and avatar-object argument.

### Freshness compatibility

The source freshness tracker starts each category with four `-1` entries. On a
new command it removes the first entry when needed but never appends the new
command. Therefore subsequent real commands continue to count as new and add
0.15. This executable bug is preserved, including the three-element `-1`
history, internal cap 1.1, exposed cap 1.0, and per-tick decay. Two repeated
commands followed by 30 ticks produce little-endian float bytes `c9 8f 94 3e`
in both the unchanged C# oracle and the native component test.

## Nightclub floor raster

The rectangular screen is reconstructed from native tile geometry. Discovery
consumes the first eligible tick without a drawing command; drawing then occurs
every three ticks, exactly ten frames per 30-Hz second. Each frame increments
the private source frame counter and updates only actual tile objects.

All source drawing paths are implemented: off, fill, dissolve, rainbow tunnel,
circles, matrix, scrolling text, heart, diamond, star, and rectangle/line/arrow/
colder particles. Frame blink uses `(frame / 10) % 2`. Bonus images preserve the
source post-increment timer and return to random animation after the 77th bonus
frame. Source animation codes 3/4 leave the previous animation selected while
updating color only and preserving direction, as in the original handler.

The port retains the source's text-bank offset bug (`Next(4) + bank`), circle
integer wrapping, source glyph bytes, clipped Bresenham lines, midpoint-to-even
vector rounding, outline order, and particle lifetimes. Random algorithms use
the native replay stream described below; identical `System.Random` outcomes
are not claimed. Matrix and colder divisors are bounded to one for tiny floor
dimensions that make the original source divide by zero. Particle storage is
limited to 256; kinds/groups, finite angle bounds, frames, and negative delayed
rectangle frames are validated before mutation and on restore.

## Private state, admission, and recovery

All handlers serialize only through the private checkpoint codec. Serialization
includes precise phase/queued phase, role agreement, remaining pieces, current
choices, timers, partial notes, votes, private song, RNG counters, contestant
claims, queued reactions, DJ targets, freshness float bits, native queue UIDs,
section ratings, raster state, and particles. Public VM events have no state
serialization path. Debug output redacts configuration, seeds, names, replies,
and family state.

Length prefixes are checked before allocation. Bounds include 16 names,
16 potential contestants/four selected slots, 32 queued player reactions,
25 Band notes, 128 sections/NPCs, 4096 tiles/portals, a 64×64 screen, and 256
particles. Reachability validation checks legal phases, role/roster agreement,
phase-specific timer/choice constraints, finite float ranges, and reconstructed
geometry. Peer validation checks cached group references against actual plugin,
cluster, object, roster, and avatar identities, including a host's cached own
group. It permits the legitimate state before peer discovery.

The shared host clones every mutation and admits all resulting UI, native
events, peer effects, and commands together. Output-pressure failure retains
the original sequence counter and private state. A rejected controller callback
likewise cannot advance payment, a round, a random stream, or a peer. Restore
uses a new epoch and leaves controllers and recorded UI sessions detached.
Partial cluster rebinding cannot silently drop a peer score/animation operation;
the host rejects it until all required retained recipients are ready. Ticks
cannot advance a recovered game before its required native/member bindings are
restored.

The private RNG is a native-seeded SplitMix64 replay stream with bounded draw
rejection and checked counters. It is never seeded from a UI message, current
wallclock, member timing, or a public event. This is an explicit deterministic
native policy rather than a claim of byte-identical `System.Random` streams.
The source probability bounds and state-machine uses of randomness are
preserved.

### Explicit native policies

| Policy | Reason and scope |
|---|---|
| `WAR-LAST-PIECE-PROGRESS` | Resume a deciding final unequal-piece round; paired original stall and actual native progress regression. |
| `BAND-PAYOUT-CALLBACK-ONCE` | Reject a duplicate native callback while the same minimum payout's finale is already queued. |
| `SOURCE-WALLCLOCK-30HZ` | Convert rehearsal/tie timers to accepted native ticks; pause on rejected/unbound execution. |
| `PRIVATE-NATIVE-RNG` | Private seeded deterministic replay with source draw ranges, without claiming original RNG bit parity. |
| `NATIVE-CLUSTER-CLAIMS` | Replace process-wide static buzzer events and random/time session stamps with actual group identity and bounded visit revisions. |
| `NATIVE-RESTORE-UI` | Reconstruct current UI state from private state without treating rebind as a fresh source game join. |
| `BOUNDED-SOURCE-INPUTS` | Reject malformed indexes, payloads, geometry, aliases, nonfinite private values, impossible roles and exhausted counters before committing. |
| `TINY-FLOOR-DIVISORS` | Define matrix/colder behavior when source dimension arithmetic would divide by zero. |
| `EMPTY-NPC-SNAPSHOT` | A club with no NPCs emits no hint instead of indexing an empty source list. |

The freshness miss bug, first-three buzzer auto-enable, no-change error bytes,
post-increment/post-decrement timing, circle wrapping, text-bank offset, and
source dance UID semantics are preserved. Their presence is intentional and
must not be silently changed by cleanup or generalized validation.

## Reproduction

From the repository root, run:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 fixtures/eod/social/verify.py
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  /root/.cargo/bin/cargo test --manifest-path crates/eod-runtime/Cargo.toml \
  --test social_handlers --target-dir /tmp/eod-social-target
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  /root/.cargo/bin/cargo test --manifest-path crates/eod-runtime/Cargo.toml \
  --lib plugins::social --target-dir /tmp/eod-social-target
```

The oracle requires Python 3, Git, Mono `mcs`/`mono`, and the original MonoGame
assembly already in the repository. It creates and cleans its small temporary
executable. It compares checked-in expectations and never regenerates them.

[`tests/social_handlers.rs`](../../crates/eod-runtime/tests/social_handlers.rs)
uses actual `NativeHost` controllers, separately authenticated UI sessions,
native callbacks, private checkpoint stores, and real emitted commands. The
suite covers complete gameplay, source literals, rejected input/output rollback,
exact tick boundaries, cross-cluster isolation, partial recovery, teardown,
numeric settings, peer discovery, and geometry bounds. Component checks add the
full matchup graph, exact decimal payout rounding, source float/font/rainbow
bytes, tiny-floor safety, and private peer-reference integrity.


### Independent review recovery regressions

`band_rebind_shows_saved_game_before_phase_controls_without_advancing_it` covers
all eight Band phases. `NATIVE-RESTORE-UI` follows the original
`UIBandEOD.UIInitHandler` (lines 506–525), `GotoWaitForPlayerPhase` (693+) and
`GotoBandGame` (744+): replay UI initialization, retained skill, `Band_Show`, then
phase controls. It never draws another sequence, changes phase or issues a
payout; its next accepted tick matches uninterrupted state.

`NATIVE-RETAINED-DEPENDENCY-READINESS` uses actual retained links and their
cycle-safe transitive closure. The host's four `native_recovery.rs` cases freeze
silent nightclub RNG and buzzer subframes until all required bindings return,
while permitting unrelated same-cluster property work. Trusted closing-source
cleanup can remove a detached link from a bound peer; ordinary peer work keeps
the full readiness gate. These are native restart policies, not original
application recovery equivalence claims.

`SOURCE-CIRCLE-WRAPPING` retains the original unchecked circle radius arithmetic
at `VMEODNCDanceFloorPlugin.cs:409–411`. The private component regression
`restored_circle_animation_wraps_radius_near_frame_limit` round-trips valid
64×64 state near both arithmetic boundaries, checks the complete 4096-pixel
output and next frame/tock, and had an observed overflow RED before the two
`wrapping_add` corrections. Social acceptance now includes 24 real-host tests
and seven native component cases, including that regression.
