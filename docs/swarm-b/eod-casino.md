# Native casino EOD handlers

The casino family implements the four original handlers in the native EOD host: Slots (`0xCB2819CB`), Roulette (`0x0B2A6B83`), Blackjack (`0x2B2FC514`), and HoldEmCasino (`0x1001`). It owns each game's state, source UI messages, dealer callbacks, cards or reels, betting decisions, and durable settlement obligations. The host owns authenticated membership, controller capabilities, 30 Hz ticks, atomic output admission, private checkpoints, and prepared provider dispatch.

This is a source translation with explicitly named native corrections. It does not claim the original `System.Random` sequence, a running production account database, or an end-to-end VM/UI deployment. The test ledger is an explicit in-memory test provider.

## Source and implementation map

All original files were read at source pin `4c6b3e8f5835b228723caea3c9f683c62f244f73`. The [source manifest](../../fixtures/eod/casino/sources.json) records SHA-256 values for all four handlers, all four UIs, the original circular deck, the shared lobby, and card-name data. The original files and assets are unchanged.

| Handler | Primary source | Native implementation |
| --- | --- | --- |
| Slots | [VMEODSlotsPlugin.cs](../../TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODSlotsPlugin.cs), [UISlotsEOD.cs](../../TSOClient/tso.client/UI/Panels/EODs/UISlotsEOD.cs) | [slots.rs](../../crates/eod-runtime/src/plugins/casino/slots.rs) |
| Roulette | [VMEODRoulettePlugin.cs](../../TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODRoulettePlugin.cs), [UIRouletteEOD.cs](../../TSOClient/tso.client/UI/Panels/EODs/UIRouletteEOD.cs) | [table.rs](../../crates/eod-runtime/src/plugins/casino/table.rs), [roulette.rs](../../crates/eod-runtime/src/plugins/casino/roulette.rs) |
| Blackjack | [VMEODBlackjackPlugin.cs](../../TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODBlackjackPlugin.cs), [UIBlackjackEOD.cs](../../TSOClient/tso.client/UI/Panels/EODs/UIBlackjackEOD.cs) | [table.rs](../../crates/eod-runtime/src/plugins/casino/table.rs), [cards.rs](../../crates/eod-runtime/src/plugins/casino/cards.rs) |
| HoldEmCasino | [VMEODHoldEmCasinoPlugin.cs](../../TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODHoldEmCasinoPlugin.cs), [UIHoldEmCasinoEOD.cs](../../TSOClient/tso.client/UI/Panels/EODs/UIHoldEmCasinoEOD.cs) | [table.rs](../../crates/eod-runtime/src/plugins/casino/table.rs), [cards.rs](../../crates/eod-runtime/src/plugins/casino/cards.rs) |

[mod.rs](../../crates/eod-runtime/src/plugins/casino/mod.rs) defines the typed native configuration, account namespaces, requests, replies, controller input, and bounded original string encoding. [finance.rs](../../crates/eod-runtime/src/plugins/casino/finance.rs) retains immutable requests and denied settlement obligations.

## Native admission and account authority

The authoritative object adapter creates a `NativePluginInput::Casino(Config)` group using `NativeHost::connect_native`. Both the request and the casino configuration identify the same persistent object. `State::trusted_object` allows the host to enforce this identity on creation and after checkpoint restore. A user cannot choose the bank account through a UI frame.

`join_native` supplies the actor, avatar object ID, persistent avatar ID, trusted role, source registers, and owner permission. The host assigns an opaque member seat for private output routing. The game's chair and role remain separate from that routing index.

| Game | Role values | Source chair | Capacity and owner admission |
| --- | --- | --- | --- |
| Slots | `1`: player; `2`: owner | None | One connected player or owner. An unsettled old player retains the machine until the financial obligation resolves. |
| Roulette | `0`: player; `1`: croupier; `2`: owner | First free native player position | Four players and one croupier. Owner management requires an empty, financially quiescent table and explicit owner permission. |
| Blackjack | `0`: player; `1`: dealer; `2`: owner | `registers[3]`, one through four | Four players, up to four hands per player, and one dealer. Owner admission is exclusive. |
| HoldEmCasino | `0`: player; `1`: dealer; `2`: owner | `registers[3]`, one through four | Four players and one dealer. Owner admission is exclusive. |

The native four-player roulette cap is a named correction. The original list is unbounded, but its bankroll reserve assumes four simultaneous players. Reusing a persistent avatar within the same game, conflicting native seats, malformed chairs, and unauthorized owner roles are rejected.

`VmInput` has `SourceEvent { code }`, `SetBroken`, `SetEnabled`, and `RetrySettlements`. Only the current trusted controller capability can deliver these inputs. No UI event accepts a provider receipt, balance, seed, deck, hidden card, native callback, or owner authorization.

## Financial progression and recovery

`Account` distinguishes `System`, `Object(u32)`, and `Avatar(u32)` even when numeric IDs coincide. `QueryBalances { source, target }` expresses the original `testOnly` one-dollar transaction without moving money. `Transfer { source, target, amount }` requests a checked positive amount between the authorized object and avatar accounts. Real money transfers involving the System account are invalid.

The typed reply contains success, both exact accounts, the exact requested amount, and both resulting balances. A typed `ProviderDenied` is a terminal business refusal. Provider transport failures remain pending and never invoke game completion. Every successful request and terminal refusal, including queries, needs an immutable, transactionally retained provider result.

The required sequence is:

1. The family freezes its request and callback correlation in private state.
2. The host admits state and outputs atomically, preserving the same operation identity if admission fails.
3. The adapter applies VM outputs and delivers private UI outputs at the checkpoint barrier.
4. A successful private host checkpoint prepares the exact request for dispatch.
5. The native provider authorizes the current host and every account, deduplicates the entire immutable request, and atomically retains the effect and exact result.
6. The host checks current receipt scope, epoch, operation identity, and the typed family reply. The family verifies accounts and amount before progressing.

The family exports the complete `pending_operations` map. The host compares this map with its journal before checkpoint admission, restore, and any effect dispatch. A missing, extra, changed-account, or changed-amount journal entry therefore cannot execute before the mismatch is detected. Table callbacks are consumed in request order because the original balance replies have no account version. Dispatch must block later operations in the same group behind an earlier unprepared or retryable operation.

A debit must succeed before the player sees a paid deal or spin. Insurance, a split, a double, and a HoldEm call are also debit-gated. Payout messages indicating success appear only after the payout receipt. UI animation acknowledgement can request progression; it cannot complete a transaction.

**Settlement freeze policy:** a refused payout or refund remains an obligation with the original account and amount. It stops new rounds and keeps `can_close()` false. An attached or rebound native controller can request `RetrySettlements`, which allocates a fresh operation for that terminally refused obligation. Retrying a transport failure uses the existing operation instead. No automatic loop silently retries a terminal refusal.

Disconnect keeps paid stakes and pending operations private. A debit completing after teardown is followed by a durable refund. Before a deal, a departed player's accepted stake is refunded. After a deal, Blackjack resolves abandoned hands, and HoldEm folds an uncalled hand while retaining a committed call. Refunds use that player's net contribution minus money already received; they do not accumulate another player's refund. Already journaled closing-group operations may dispatch without UI rebind, and closing groups remain retained until money obligations finish. Restored active groups require all recorded bindings before gameplay or provider progression.

## Preserved game rules

### Slots

Known source object GUIDs select five machine variants. Unknown GUIDs use the validated native machine-type register.

| Machine index | Source GUID (decimal) | Denomination | Minimum bank | Maximum bank, exclusive for play |
| --- | ---: | ---: | ---: | ---: |
| 0 | 2448255364 | 1 | 2,500 | 15,000 |
| 1 | 3106786481 | 5 | 12,500 | 37,500 |
| 2 | 2906162829 | 10 | 25,000 | 75,000 |
| 3 | 82792878 | 25 | 62,500 | 187,500 |
| 4 | 82792879 | 100 | 250,000 | 750,000 |

Wagers are one through five denominations. The source payback control accepts 80 through 110. Wheel construction preserves 32-bit float arithmetic, ties-to-even rounding, the original first-symbol adjustments, and the special 110-percent blank correction. At 100 percent, stop counts for values zero through eleven are `[12, 42, 12, 49, 12, 36, 12, 24, 12, 12, 15, 6]`, totaling 244.

Source payout multipliers are 500, 150, 75, 50, and 25 for the five highest matching triples; the descending third/second/first combination pays 10; two first symbols pay 5; a first symbol on the first reel pays 2. An unmatched result pays zero. The provider amount retains its full checked value; the VM event's original signed-short argument preserves the source's wrapping representation.

`slots_execute_bet` commits no outcome before debit success. `slots_wheels_stopped` can only finish the current spin once. Source game-over callbacks 5 or 6 resume play only after settlement. Owner odds, on/off, deposit, and withdrawal events preserve the source channels. Management queries actual funds before showing the bank balance.

### Roulette

The wheel is American roulette: zero, one through thirty-six, and double zero represented as `100`. The fourteen source bet types and chip denominations 1, 5, 10, 25, and 100 are preserved. A stack holds at most twenty chips. Total wagers are bounded by the configured maximum, itself at most 1,000. Returned payouts include the stake.

Native validation checks the complete physical layout: legal adjacent splits, aligned streets, corners and six-number lines, the special zero-area lists, full dozens and columns, and unique full odd/even or red/black selections. This deliberately closes source validators that accepted crossing rows, omitted the last list element, or admitted duplicate outside-bet numbers. The original unusual zero-area ordering is retained where it identifies a real source bet.

The source `%` text protocol, descending chip order, and base-255 two-byte limit/balance packing are preserved. Empty neighbor synchronization sends `0%0` so clients clear stale chips. Betting lasts thirty authoritative seconds, the spin lasts 360 ticks, and intermission lasts 180 ticks; provider delay freezes the relevant payment boundary. The source minimum bankroll is `140 × maximum bet`.

### Blackjack

The game uses the source six-deck circular shoe, with two shuffles in the waiting flow. The dealer hits soft seventeen. Players may split any two equal-value ten cards, play split aces, and create up to four hands. The source permits a natural on a split hand; a natural pays 3:2 with integer truncation. Two-card eligible hands can double, doubling the amount at risk and taking one card. A normal win returns twice the wager, a doubled win four times, and a push returns the committed stake.

The insurance prompt lasts ten seconds. Its charge is `floor(original bet / 2)`. If the dealer has Blackjack, the source's insurance payout adds the original bet, including the source's odd-bet behavior; it is not replaced by a conventional alternate formula. A one-unit bet's zero-cost insurance declaration is retained without creating an illegal zero-amount transfer.

Initial betting lasts thirty seconds, pending initial debits have the source two-second delay, a player's decision lasts fifteen seconds, and the finale lasts two seconds. An initial observer is warned; repeated observation is removed as in the source. Native animation callbacks distinguish ordinary animation `100`, dealer check `101`, split `102`, and queued dealer/collection completion `103`. A wrong or duplicate callback cannot advance the phase.

The bank reserve remains `8 × maximum bet`, with the source maximum balance 999,999. Failed extra debits preserve the already-paid hand. When an actor leaves while a split or double is pending, successful payment resolves the additional cards without awaiting that departed actor's animation callback.

### HoldEmCasino

This is the source banked casino game. Players compete against the dealer using private two-card hands and shared community cards. The initial deal contains two private cards per accepted player, two dealer cards, and a three-card flop. The optional AA+ side bet is evaluated using the player's five cards at the flop. A player calls by paying twice the ante or folds. Two final community cards are dealt when at least one player has called.

The dealer qualifies with a pair of fours or better. A nonqualifying dealer returns the call and pays the ante chart. A tie returns the committed ante and call. A qualifying dealer loss pays the ante chart and the call at 1:1. The ante ratios are 25 for a straight flush, 12 for four of a kind, 3 for a full house, 2 for a flush, and 1 otherwise. The side bet pays 7:1 for a pair of aces, two pair, three of a kind, or a straight; a flush or better pays 25:1. Side returns include the side stake.

The native evaluator checks all five-card subsets of five to seven cards, handles the ace-low straight, and compares category and rank without using suit to break ties. The source card names and original BinaryWriter seven-bit UTF-8 length encoding are preserved.

The table uses a single circular deck, thirty-second betting, the two-second pending-debit delay, fifteen-second decisions, and a three-second hold only when there are no side winners to animate. Its reserve is `84 × maximum ante + 104 × maximum side bet`, with maximum table balance 999,999. A refused call keeps the accepted ante and permits another decision. Negative side bets are rejected before arithmetic or provider admission.

## Privacy and named native corrections

Seeds, RNG position, decks, hidden cards, private wagers, balances, account requests, and settlement obligations exist only in native family state and the private checkpoint. `Config`, `State`, `Account`, `Operation`, and `Reply` use redacted debug output. The generic VM projection does not carry this state. This crate is native-only and rejects a browser/wasm authoritative build.

Two original disclosures are intentionally corrected. Blackjack's first deal masks the dealer hole card, matching the privacy intent already present in the source resynchronization flow. HoldEm sends each player only their own hole cards until showdown; opponents and dealer are masked. Rebind uses the same recipient-specific rules, and folded opponents remain hidden. Public dealer animations carry source event arguments, never the private deck.

Additional corrections are checked owner arithmetic, the roulette geometry and capacity limits, the settlement freeze policy, monotonic provider correlations, net per-player refunds, the HoldEm initial-bet/call failure distinction, and avoiding a stale side-winner value being reused for an observer. Each change follows a concrete source defect or the required native authority/recovery boundary. Source channel names, source event codes, documented game rules, and source timer units remain the compatibility target.

The RNG is a private, portable SplitMix64 stream with rejection sampling and Fisher–Yates shuffling. It is deliberately not presented as `System.Random` replay. Bounded rejection, checked counters, fixed maximum deck/card sizes, four player positions, bounded chip sets, bounded strings, and a maximum of sixty-four financial records prevent input-sized unbounded allocation or loops.

## Verification and integration boundary

The [component tests](../../crates/eod-runtime/src/plugins/casino/tests.rs) and focused tests beside the card, roulette, slot, and table implementations exercise literal source payout cases, source timing, privacy, duplicate admission, provider refusal, owner permissions, departure, and private save/restore invariants. Every component harness transition validates both live and restored state and compares private bytes after the round trip.

On 2026-10-05, the actual crate's casino component suite passed **30 tests**. The card suite enumerates all **2,598,960** five-card hands and compares all nine category counts with independent rank/suit combinatorial formulas; it also checks seven-card selection, ace-low straights, full-house selection, and ties. Private checkpoint tests reject changed payouts, pending wagers above configured limits, impossible animation gates, and inconsistent idle phases. Financial conservation checks reconcile money already received with the exact remaining obligation, including a persisted distinction between refunds and normal winnings.

The [real-host acceptance suite](../../crates/eod-runtime/tests/casino_handlers.rs) runs through `NativeHost`, the prepared private journal, `PrivateCheckpointStore`, authenticated member tickets, current native controller tickets, and typed `NativeProvider` receipts. It exercises crash replay after a committed transfer with a lost reply, binding requirements, receipt mismatch, closing-group refunds, retained denied payouts, ordered multiplayer roulette debits, and complete Blackjack/HoldEm rounds with recipient-specific private views.

The real-host suite passed **14 tests** on the same date. Both suites use the actual host crate; the initial isolated kernel harness was used only while the shared host interfaces were being implemented. The fresh independent review corrected late Slots debit refunds and late split/double/call teardown recovery; each correction has observed RED→GREEN host cases.

The owning lane must still supply the production authenticated account provider, durable transactional request/result deduplication, a trusted latest checkpoint stamp and atomic private store, the VM object adapter that supplies source registers and animation callbacks, and private UI delivery at the VM checkpoint barrier. The implementation does not infer these services from UI messages or replace them with the test ledger. Original-runtime differential verification and a production VM/UI walkthrough remain separate qualification work.


### Original helper execution and exact policy anchors

`python3 fixtures/eod/casino/verify.py --report fixtures/eod/casino/helper-report.json`
verifies all eleven pinned whole files against Git bytes, compiles the unchanged
original deck and two exact Blackjack helper slices, and compares 35 literal
trace lines twice. Eight changed-expected comparator controls must be rejected.
The [fixture README](../../fixtures/eod/casino/README.md) lists source byte ranges
and scaffolding. This is original deck/Blackjack helper evidence; it excludes
complete casino handler execution, the original HoldemHand library, provider/VM
qualification and System.Random equivalence. Native poker combinatorial tests
remain independent evidence.

| Named policy | Source anchor and regression |
| --- | --- |
| `CASINO-DEALER-HOLE-PRIVATE` | `VMEODBlackjackPlugin` initial deal/resynchronization; `blackjack_real_host_masks_dealer_card_and_settles_a_complete_round` masks the dealer hole before reveal and after restore. |
| `CASINO-OPPONENT-HOLE-PRIVATE` | `VMEODHoldEmCasinoPlugin` initial deal/player synchronization; `holdem_real_host_keeps_each_hole_private_then_settles_side_and_called_game` preserves recipient-only holes until showdown. |
| `CASINO-DEBIT-BEFORE-REVEAL` | Source bet transaction callbacks and the native provider boundary; spins/deals advance only after confirmed debit and output admission. |
| `CASINO-UNREVEALED-DEBIT-REFUND` | `VMEODSlotsPlugin.OnDisconnection` and bet/winnings transaction callbacks (source lines 194, 365 and 511); late successful debit after departure retains a stake-sized refund, independent of the preselected reels. Tests cover win/loss, denied refund, restore/retry and machine reuse. |
| `CASINO-CLOSED-FINANCIAL-CONTINUATION` | Blackjack split/double and HoldEm call debit callbacks during `OnDisconnection`; native teardown stops gameplay while immutable late debit/refund obligations remain valid. Three dedicated late-debit host regressions cover split, double and call. |
| `CASINO-ROULETTE-FOUR-SEATS` | Source roulette list/reserve calculation; the native limit matches the four-player bankroll reserve instead of admitting unbounded simultaneous wagers. |
| `CASINO-IMMUTABLE-SETTLEMENT-RETRY` | Native adaptation of source transaction callbacks; exact amount/account tuples and retained denied obligations require checkpointed retry and current host authority. |

These policies describe native corrections and authority/recovery boundaries;
they are not assertions that the original application had the same behavior.
