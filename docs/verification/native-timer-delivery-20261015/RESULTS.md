# Native Timer and owned event delivery qualification

Base: `1151d9ca1033a7e3a457529a8347d220f130613b`. Isolated source increment; frozen departure release and v3 package remain unchanged. Original content bytes, account fixtures and private handler checkpoints are not included in this source tree.

## Executed source journey

The separately pinned private Timer release uses binding `8921d56fa9261bdfc0f96c740eaa9a1ee240c60784916d5f704c3ef3fe5a5da4`. The original Timer object is GUID `0x39D04C9C`, original Be Timekeeper is TTAB index 5 / BHAV 4113, and the source plugin is `0xAA65FE9E`. Intake hashes are in `source-release.json`.

Actual native two-account/NativePlayer/WebSocket run `flow5` passed original Be Timekeeper, source opcode 62 association, private Show retry and authenticated ack, Timer_Set 1:07 (public event 2, packed value 263), Start (3), Pause (4), Update (0), private Update 1:08, two process takeovers without an intervening simulation tick, and source leave (-1), account save and control release. The observer received no private Timer output. Recovered Show was exactly `00010108`, preserving paused/stopwatch/1:08. Both takeover results were at tick 213, epochs 2 then 3. Evidence SHA256: `76f4443b3503cb8e5bf07384ee38de1f277dafa5261a6afa4eda89c3a6f0949b`.

The earlier flow4 recovery claim was withdrawn after review found non-durable resync and scratch-register Show reconstruction. The accepted correction stages the successor private checkpoint before the coordinator's atomic takeover commit, anchors its revision/hash in the immutable recovery receipt, and gates activation/output on that exact committed hash. Public event receipts use the reducer's actual attached-at-application outcome, not a post-tick inference.

No rendered original Timer panel, Timer artwork, new external service or deployment is claimed.

## Executed gates

- Source owned-leave/Timer suite: 25/25, including same-tick accepted-event then reset disposition.
- Private Timer transaction suite: 17/17, including retained Show state, packed source minutes and repeated restore without a source tick.
- Actual SQLite coordinator suite: 74/74, including rollback, stale paired predecessors, archive replay, consecutive recovery, complete-lane receipt/delete atomicity and 65,537 ordinary commits without departure-history growth.
- Native whole-lane tests: 2/2, proving append-only wire layout/real ordinals and complete-known-clear versus active/pending-private/stale-owner refusal.
- Authenticated private runner tests: 4/4.
- Native host source built successfully. TypeScript no-emit, JavaScript syntax and diff whitespace checks passed.

Earlier whole eod-runtime suite execution passed 236 unit/integration tests plus one doctest before the reviewed resync correction. Current focused private transaction coverage is the 17 tests above; do not describe the entire earlier suite as rerun against the final correction. Other newly authored primitive/schema regressions and a full workspace build are not yet claimed.

## Owned generic lane semantics

The only supported consumer kinds are OwnedEodDisconnectRequested, OwnedDepartureCaptured and OwnedDepartureDetached. Capture/detach are observations of already committed source effects, matched to their exact dedicated save/detach receipt and original batch/tick/ordinal. EOD is a request: complete source coverage and terminal exact association/public/private receipts are required. Socket transmission, actor absence, current actor ID alone, or an unrelated private ack never proves it complete.

A private verified reader checks the original committed live/archive payload and decision. Independent native canonical and coordinator fixed-layout decoders must agree on every member. One immutable whole-lane receipt and deletion of its exact events outbox row share a transaction. Unsupported members, unresolved peer ownership, missing source proof, effects lanes and unknown historical EOD remain pending. Exact retry remains available after journal compaction. Receipt retention is capped at 8,192 lanes; exhaustion stops retirement rather than dropping evidence.

Frozen historical rows are not migrated into known-clear. This source increment does not retire the six historical rows from the earlier frozen journey.

Actual new-stream whole-lane acceptance (`flow6`) passed. Two consecutive takeovers shared completed tick 214; recovered Show remained `00010108`. After the Timer owner left, all of that owner's generic lanes retired. The second actual owner, who never opened an EOD, then left through the same consumer. Final generic and dedicated outboxes both contain zero rows. Five immutable whole-lane receipts cover six event members: one exact-session-terminal EOD request (session 1), one complete-known-clear EOD request (no sessions), two captures and two detaches. No acknowledgement was based only on absence. Evidence SHA256: `6fae9e4bc1647c789ff1c2a2710986f1d30ee620b05d542454e625c2eaa424d9`. Independent SQLite inspection is recorded in `flow6-event-retirement-summary.json` in the private qualification evidence directory.

## Limits

See DESIGN.md for exact session, private-output, envelope and receipt caps. The earlier 9936 producer retained every no-op Timer poll and could exhaust 4,096 source-request receipts. The handlerless-poll follow-on below removes that producer-side growth without deleting receipts or raising caps. Real UI requests, public/private outputs and retained associations still have bounded lifetime histories; this is not an unbounded production retention solution. Original Timer has no generic inactivity teardown; the native 24-hour transport guard is explicitly additional policy. Unsupported plugins, joinable/controller variants, detached check-thread calls and caller/thread-owner mismatch remain explicit unsupported cases.


## Handlerless-poll follow-on above 9936

The production change is confined to `runtime_eod::invoke`: do not allocate an EodSimanticsRequested ID/event for the supported Timer association. Original VMEODHandler initializes an empty SimanticsHandlers map, VMEODTimerPlugin adds none, and VMEODServer::SimanticsDeliver only invokes a matching registered handler. This is a proven no-op for every local event code. Source connect/disconnect requests, public nonnegative FIFO including Update 0, Ended handling and snapshot receipts remain unchanged. The append-only event variant and old private receipts remain readable and are not pruned. Old already-exhausted histories are not automatically repaired.

Executed against the follow-on source:

- One continuous authored-source regression: 8,192 connected polls, midpoint snapshot restore, exact replica hashes/events, no request-ID growth, real Update 0 and duplicate retry, then real source close with the next exact request ID. Passed in 33.73 seconds after a 20.32-second focused test build.
- One actual-SQLite capacity regression: at 8,192 whole-lane receipts, generic retirement fails and retains its exact row; matching dedicated account release and a subsequent ordinary paired commit still succeed. Passed. This cap limits notification retirement, not ordinary tick execution.
- Matching native host and NativePlayer/read-only inspector helpers linked successfully. No full suite rerun is claimed for this follow-on.

Extended original-source qualification is incomplete. Flow7 was stopped after its initial 15-minute observation budget was found insufficient for measured throughput; its durable source reached tick 566 with one request receipt and a 3,187-byte private envelope. Flow8 was then authorized with a 60-minute window. Its initial poll observer incorrectly compared VmThread.last_exit with GotoFalseNextTick: that field records frame-pop exits, so the counter missed real source polls. Read-only original source/control-flow inspection established a prospective exact observer using original BHAV4113's unique opcode62-to-PC4 branch and a newly scheduled sleep at the current tick. No corrected 4,097-poll result is claimed, and elapsed simulation ticks are not substituted for polls. All partial journals/checkpoints remain private and preserved. The earlier flow6 Update/recovery/leave evidence belongs to immutable 9936 and has not been silently relabeled as a follow-on beyond-cap pass.

The follow-on changes deterministic state/event production. Hosts and replicas must use matching source; it is not a mixed-version hot-upgrade contract. Schema remains seven, with no historical receipt migration or accepted-tail hash bypass.
