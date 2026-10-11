# Cooperative native EOD host

PaperChase (`0xCA418206`), PizzaMaker (`0xEA47AE39`) and
TwoPersonJobObjectMaze (`0x4A245A22`) run through the actual native `NativeHost`.
Each game owns one private shared state, one recorded no-avatar controller and
one authenticated UI session per occupied role. These are source translations
with native-host acceptance evidence. The original FreeSO application, original
UI runtime, production VM adapter and production restart path have not been
executed for these three handlers.

The source baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`. See the
[exact registration census](eod-coverage.md) for all 30 server registrations.
The separate [unchanged-C# component oracle](eod-source-oracle.md) covers the
existing five handlers; that evidence does not qualify this family or the full
original application.

## Trusted native API

The types below are exported by `wonderland_eod_runtime`.

| Operation | Authority and result |
| --- | --- |
| `connect_game_controller(GameControllerRequest { object, invoker, input })` | Trusted no-avatar invocation. `GameControllerInput::{PaperChase, PizzaMaker, Maze}` supplies a `PrivateSeed`. Returns `GameControllerTicket`, emits the source controller connect event and creates no UI session. |
| `join_game(authority, GamePlayerRequest { connection, game, invoker, avatar_object, input })` | Native role/avatar/tuning data plus transport authentication. `game` is the controller's scoped `InstanceAddress`. Returns an existing-format participant `SessionTicket` and private enter/show outputs. |
| `deliver_game_event(ticket, invoker, input)` | Trusted SimAntics callback. Checks scope, epoch, recorded controller invoker, controller lifetime and the complete recovery barrier. Only `GameVmInput::{PizzaRespondPhone, PizzaAllContributed, PizzaRespondBake}` exists, corresponding to source events 6/7/8. |
| `rebind_game_controller(address, invoker)` | Reattaches exactly the controller recorded in a restored snapshot and returns a new-epoch ticket. Emits no second connect event. |
| Existing `receive`, `tick`, `disconnect`, `disconnect_invoker`, `take_private`, `checkpoint_to` and `rebind` | Serve game participants through the existing native session, output, timeout and recovery boundaries. |

`GamePlayerInput::PaperChase { slot }` records source temp0: 1..3 selects that
role, while 0 takes the first free role. Other values reject atomically.
`GamePlayerInput::PizzaMaker { station, tuning }` records temp0 0..3 and signed
temp1..7 through `PizzaTuning`. `GamePlayerInput::Maze { role }` uses
`MazeRole::Logic` for source temp0 equal to 1 and `MazeRole::Charisma` otherwise;
that conversion belongs to the native VM invocation adapter.

Controller object/invoker, avatar ObjectID, role, tuning and seed never come
from a UI envelope. A controller ticket is distinct from every UI ticket, and
its invoker argument is checked against recorded private state. These methods
are trusted integration APIs, not remote endpoints. Public Rust fields do not
authorize an untrusted caller to act as the native VM adapter.

There is at most one shared game per object. An invoker cannot also belong to
another controller or participant. Existing global actor and connection
uniqueness applies; avatar ObjectIDs are unique across game and DanceFloor
participants. Occupied roles reject replacement. Invalid admission does not
emit temporary connect/disconnect events, change tuning or consume random state
or identity counters.

Inbound native protocol version 1 retains its 57-byte scoped header. It carries
no controller callbacks, role/tuning/seed setters or alternate recipient. Event
names and text/binary kinds are allowlisted. Malformed source choices are no-ops
that consume an accepted UI sequence; rejected envelopes do not consume it.

## Source behavior and authoritative timing

**COOP-30HZ-GATE:** the adapter must call `NativeHost::tick` exactly once per
authoritative 30 Hz simulation tick. A game advances once regardless of its
participant count. The host does not infer elapsed wall time, start background
timers or advance phases because UI messages arrived. Participant idle deadlines
use the host clock and continue while a restored game's phase is paused.

### PaperChase

The translation follows
[`VMEODPaperChasePlugin.cs`](../../TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODPaperChasePlugin.cs),
[`EODLobby.cs`](../../TSOClient/tso.simantics/NetPlay/EODs/Utils/EODLobby.cs) and
the original [`UIPaperChaseEOD.cs`](../../TSOClient/tso.client/UI/Panels/EODs/UIPaperChaseEOD.cs).

| Phase | Preserved behavior |
| --- | --- |
| Lobby (1) | Three avatar ObjectIDs separated by newlines, with no trailing newline. Joining sends `paperchase_show`, the roster and player-invoker Idle event 4. A full lobby immediately starts a round. |
| Start (2), Waiting (3) | Selects from the source-ordered 27 combinations, immediately enters Waiting, broadcasts the seven-field letter/history view and emits controller Idle event 4. |
| Provided (4), Checking (5) | A first valid choice 1..3 immediately emits event 2 containing `letter \| (one_based_role << 8)`. The third choice additionally re-emits all three packed letters, then event 1 with the match count, and enters Checking. Duplicate choices retain the first choice. |
| Result (6) | Checking uses `ticks > 420`: ShowResult event 3 occurs on tick **421**. Result uses `ticks > 90`: tick **91** returns to Waiting after a miss, or starts a new random combination after three matches. |

The last delivered letter view is checkpointed separately from current internal
guess arrays. Source transition to Checking clears/reassigns those arrays before
another UI broadcast; rebuilding from those arrays alone would lose the view a
reconnecting player had seen. Text `close` disconnects its participant. An
ordinary leave resets the remaining lobby.

### PizzaMaker

The translation follows
[`VMEODPizzaMakerPlugin.cs`](../../TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODPizzaMakerPlugin.cs)
and [`UIPizzaMakerEOD.cs`](../../TSOClient/tso.client/UI/Panels/EODs/UIPizzaMakerEOD.cs).

The constructor populates its deck before any invocation tuning is read. Its
fixed pool remains **120 cards**, even if temp4..7 later contain zero, negative
or very large values. Each base ingredient has 16 small, 10 medium and 8 large
cards; each bonus ingredient has two of each size. Construction/reinsertion
retain source iteration order and random insertion; draws take the first card.
An absent station preserves its previous hand for its next occupant.

| Phase | Preserved behavior |
| --- | --- |
| Lobby (0) | A full roster starts Phone on the next tick. Leaving an active game clears contributions, emits Restart event 5, sets timer -1 and returns to Lobby. |
| Phone (1) | Fills missing hand slots and sends each hand only to its station. Positive countdowns decrement every 30 ticks. At zero, RingPhone event 1 contains station 1's avatar ObjectID and the timer becomes -1. Native callback 6 enters Contribution. |
| Contribution (2) | Text `ingredient` selects hand slot 0..2 once per station. Event 2 packs ingredient kind with `zero_based_station << 8`. The card returns to the pool and its hand slot becomes empty. Manual contribution broadcasts contributions but sends no hand UI, matching the source UI's local button removal. |
| Bake (3) | Callback 7 or the contribution deadline supplies missing choices using a random hand-slot draw and random reinsertion. Automatic choices also send the affected private hand. The recipe clamps bonus kinds to bit 3, evaluates size groups in ascending order and lets the last matching size win. Event 3 carries the computed result. |
| Break (4) | Callback 8 sends result UI, emits PayoutResult event 4 and starts the restart countdown. At zero it returns to Lobby; the following tick may start Phone. |

Text `close` is deliberately a no-op, matching the commented-out source
disconnect. Zero and negative signed timer tuning remains valid: zero may wait
until the next phase tick; negative values do not count down. Callbacks retain
source phase checks, including valid early Phone responses.

`PayoutResult` is an object event for the VM. The C# handler makes no direct
funds or payout persistence call; this does not establish a durable payout
implementation or qualify whatever effects the object's SimAntics performs.

### TwoPersonJobObjectMaze

The translation follows
[`VMEODTwoPersonJobObjectMazePlugin.cs`](../../TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODTwoPersonJobObjectMazePlugin.cs),
[`AbstractMazeGenerator.cs`](../../TSOClient/tso.simantics/NetPlay/EODs/Utils/AbstractMazeGenerator.cs)
and [`UITwoPersonJobObjectMazeEOD.cs`](../../TSOClient/tso.client/UI/Panels/EODs/UITwoPersonJobObjectMazeEOD.cs).

The maze is 8 rows by 36 columns. Recursive backtracking and BFS retain North,
West, East, South neighbor order. Color lists retain final-cell processing
order. The twelve source color pools total 282 cells; once only one pool remains,
its count is no longer decremented, so the extra six cells retain that pool's
color. Wall codes use the source enum, not a bit mask. The exit's Charisma cell
color becomes blue while its original color-list membership remains unchanged.

Logic receives 144 staggered wall codes, four color coordinate lists and exit
coordinates. Charisma receives only the current two-byte wall/color cell. Only
Charisma can move, using exactly one binary direction byte. An unknown direction
byte keeps the current cell and still sends its update. A solved round reveals
the solution only to Logic: origin color followed by path coordinates excluding
the exit.

Pending phases apply at the beginning of the next tick. Ready initializes Logic
and Charisma views on separate source steps as needed. Its cooldown is 6 seconds
when Logic joined last, or 8 seconds when Charisma joined last. Solving counts
300 seconds; Reacting counts 10. Reaching zero does not immediately change phase:
the following tick queues the transition and the next tick applies it. With both
fresh roles joining before any game tick and Charisma joining last, Solving
starts on native tick 244. Later rounds retain the consumed source cooldown;
they do not invent another join delay.

## Explicit native policy differences

These guards are intentional and are not original-runtime parity claims.

| Policy | Native behavior and source difference |
| --- | --- |
| `PAPER-PHASE-GUARD` | A valid parsed choice outside Waiting rejects with `PluginNotReady`. The original UI disables it, but the server accepts it and can prefill a later round into a deadlock. Malformed values remain no-ops. |
| `MAZE-INIT-GUARD` | Initial Waiting generation completes during controller creation, before a join can overwrite it with Ready. The original can otherwise reach Ready with an uninitialized maze and shut down. The initial Waiting notification consequently precedes any UI recipients. |
| `MAZE-PHASE-GUARD` | Charisma movement stops while a phase change is queued, including after an exit, timeout or leave. The source checks only its current Solving phase and permits this between-ticks race. |
| `MAZE-REJOIN-GUARD` | A replacement cannot overwrite a queued leave/reset with Ready and revive the old map or solution. The reset executes first. |
| `COOP-ADMISSION-ATOMIC` | Rejected native invocations leave state and outputs unchanged. In particular, the original Pizza handler assigns tuning before rejecting an occupied station; native role validation rejects before assignment. The original framework can also emit a connect/disconnect pair around rejected handler admission. |
| `COOP-CONTROLLER-GUARD` | A recorded controller must exist before UI roles join. Only its scoped, epoch-fenced native ticket and recorded invoker can deliver Pizza callbacks; the original callback methods check phase without checking their `client` argument. Native controller teardown closes every seat rather than preserving references to a detached controller. |
| `COOP-RECOVERY-ABORT` | Losing a retained seat during partial restore/rebind aborts the whole group, so it cannot change private phases behind a detached controller. This is a native recovery policy, not an original restart implementation. |

## Private random policy

`PrivateSeed` has redacted `Debug` and comes from a trusted native seed source.
UI messages and public VM snapshots cannot set it. Games use the bounded native
SplitMix64 stream in `src/games/rng.rs`. Every stream retains its 64-bit state and
checked draw counter. The maximum counter value is reserved and rejected before
consumption, so a successful draw cannot create an unrestorable checkpoint.
Bounded rejection sampling permits at most 32 attempts,
with an accepted upper bound no greater than 4096. Draw exhaustion rejects the
whole operation without committing partial state.

Maze has separate streams for handler decisions and carving. Handler
selection/color/origin decisions use the supplied seed; carving uses that seed
XOR `0x4d415a455f425549`. Both stream states and consumption counts are separately
checkpointed. This preserves distinct source random consumers, not
`System.Random` sequences. SplitMix64 is not a cryptographic RNG. This slice
makes no legacy RNG sequence-parity or casino qualification claim.

## Private checkpoint and rebind

Timer-only format 1 and non-game generalized format 2 remain supported. A
snapshot containing a cooperative game uses private format 3, schema 1. Existing
participant and persistence records remain; shared game records precede the
existing bounded write journal. Each game records plugin, object, controller,
four bounded seat references and exactly one private state. Participants point
back to the matching game and role.

Private state includes all PaperChase guesses, scores and last-delivered view;
Pizza tuning, phases, fractional countdown, deck order, hands and contributions;
and Maze graph, color ordering, solution/position, phases, counters and
initialization flags. All random state stays private. Each shared-state blob is
capped at 8192 bytes before parse; schema-1 maxima are 47 bytes for PaperChase,
177 for Pizza and 840 for Maze. Card, graph, color and path allocations have fixed
upper bounds independent of input lengths.

Restore checks counts, sizes, schema/version, trusted stamp, strictly newer epoch,
unique identities and complete bidirectional seat references. Handler checks
include the fixed deck totals, contributions present in the pool, computed Pizza
result and reachable deadlines; reciprocal connected acyclic maze walls, source
color counts, BFS-derived solution, unchanged pre-play origin and legal queued
phases; and PaperChase previous guesses matching their scores and incomplete
Lobby rosters. Full PaperChase rosters must already be in an active phase.

No transport connection or controller attachment is restored. Participant
`rebind` authenticates its recorded actor, rotates session generation, resets
inbound sequence to 1 and reconstructs only that role's previously initialized
private view. A Charisma-only Waiting checkpoint therefore cannot reveal an
initial cell through rebind while Logic is absent. Rebind does not deal cards,
generate a maze, reset a phase or emit a second VM connect event.

Game input, callbacks and phase ticks stay paused until the recorded controller
and every retained participant reattach. The trusted adapter may abort through
`disconnect_invoker`; retained idle deadlines also bound incomplete recovery.

Controllers and participants count toward `max_instances` and
`max_participants`; each shared game counts once toward `max_timers`. Messages,
rates, output queues, checkpoint bytes and idle deadlines use existing host
limits. Fixed-size handler work occurs on a bounded transactional candidate;
admission checks output totals before replacing live state. Rejected operations
do not publish partial effects or consume random state, phases, sequences or
identity counters.

Checkpoint creation still requires empty delivery queues at the VM barrier.
The adapter must have applied VM events and delivered UI outputs, not merely
dropped them. `PrivateCheckpointStore` must authenticate, integrity-protect and
atomically persist private bytes while retaining a trusted latest stamp. A
production store and complete VM restart integration remain outside this slice.

## Acceptance evidence and remaining work

[`cooperative_games.rs`](../../crates/eod-runtime/tests/cooperative_games.rs)
drives actual `NativeHost` instances. Coverage includes real three/four/two-role
gameplay; exact source event order and timer edges; an actual maze solved through
Charisma messages; Logic-only solution delivery; private phase/RNG/hand/map
recovery; wrong actor/role/scope/epoch/controller attempts; corrupted checkpoints;
source no-op close and constructor-pool tuning; timeout/revocation/teardown;
replacement roles; and join/callback/tick pressure with exact retry checkpoint
equality. The deadline, contribution-multiset, pre-initialization rebind, full
PaperChase Lobby, RNG exhaustion and pre-play Maze position regressions were each
observed failing before their fixes. Positive cases retain incomplete lobbies,
signed timer tuning and legitimate Maze movement through Solving and Reacting.

The provider conclusion is narrow: these three C# handlers have no direct
durable provider calls. Their object-event effects remain the VM's responsibility.
Original handler/UI execution for this family, a real native VM adapter,
authoritative 30 Hz integration and private restart rehearsal remain open.
WarGame's wall-clock callback and casino/inventory/cross-plugin families are
outside this increment.
