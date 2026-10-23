# Native source Timer delivery increment

Base: 1151d9ca1033a7e3a457529a8347d220f130613b. This working tree is isolated from the frozen owned-departure release. This file is a design, not qualification evidence.

## Scope

Connect the existing native Timer handler (plugin 0xAA65FE9E) to source VM InvokePlugin and a durable, authenticated delivery boundary. Limit the first active adapter to nonjoinable Timer sessions with an owned avatar participant. Preserve explicit unsupported state for other plugin IDs, joinable/controller sessions and unknown historical EOD state. Do not manufacture an original authored object or treat a source-derived test BHAV as evidence of an original authored Timer asset.

No additional external service, deployment, account grant, or financial provider is needed. Existing private native process, LotCoordinator SQLite, private checkpoint storage and owned WebSocket transport are the intended boundaries. The existing private intake contains the original Timer behavior. Actual original Be Timekeeper execution is qualified separately from a rendered Timer panel or object artwork; no rendered Timer UI claim is made.

## Original semantics

Original source is available in the sibling wonderland-action-outcome-20261015/TSOClient/tso.simantics tree.

- NetPlay/Model/Commands/VMNetSimLeaveCmd.cs: Execute calls UserLeaveLot on a live avatar; it does not itself certify EOD teardown.
- Entities/VMAvatar.cs:631: UserLeaveLot cancels queued actions, enqueues source leave, and starts the kill timeout. The commented direct ForceDisconnect call is not active source behavior.
- Engine/VMThread.cs:824: cancelling Queue[0] on an avatar calls VMEODHost.ForceDisconnect. This call does not use the separate CanBeActionCancelled guard.
- NetPlay/EODs/VMEODHost.cs:119: ForceDisconnect resolves the avatar-to-server map and exact client. Missing association is a no-op only because the host owns the complete map.
- NetPlay/EODs/VMEODServer.cs:144: Disconnect queues object event -1, removes the client, executes OnDisconnection, sends private eod_leave, and unregisters both avatar and invoker.
- NetPlay/Model/Commands/VMNetEODEventCmd.cs:16: object event delivery requires an existing thread/EODConnection, appends the event and marks Ended for -1. Commands are server-only.
- Primitives/VMInvokePlugin.cs: operand locals identify person, object and event. The caller is the invoker, independently of stack object. -1/-2 first force-close and clear an existing invoker connection. With no connection, -2/-5 initiates. Nonnegative FIFO events (including Timer Update code 0) are returned before Ended, writing event-local and temp registers; negative events are skipped. Empty live polls clear event-local/temp0 and return GOTO_FALSE_NEXT_TICK. Ended returns GOTO_TRUE after events drain.
- NetPlay/EODs/Model/VMEODPluginThreadState.cs: connection object/avatar, joinable, Ended and FIFO events are snapshot state.
- VM.cs:379: EODHost.Tick occurs before object/scheduler simulation.

## Frozen 1151 contracts and gaps

NativeHost in crates/eod-runtime already provides authenticated ConnectionAuthority, NativeHost.connect, disconnect, disconnect_invoker, tick, private take_private and public take_public_events. Timer is provider-free. Private UI cannot be converted to a public VM event. NativeHost's current checkpoint_to explicitly requires the VM/UI output barrier and rejects nonempty output queues; its saved format excludes transport connections and queues. Draining take_* alone proves neither delivery nor VM application.

Frozen 1151 sim-core opcode62 instead issues an ExternalRequest with operand/parameters/temps but without frame locals. VmThread has no EODConnection. A generic effect completion is not a replacement for the source polling/association state machine. Every InvokePlugin permanently invalidates the legacy lot-wide known-clear proof.

Generic coordinator outbox is (batch_key, lane, proposal_digest), where lane is effects or events. A single events row can contain several serialized EventEnvelopes. Existing captures and detaches are already projected to the dedicated save/release path; their generic events are observations. OwnedEodDisconnectRequested is a request, with no association or completion proof. Its permanent (actor, control_generation) coalescing is unsuitable for successive EOD sessions under the same ownership generation.

## Required identities

Keep four identities distinct:

1. Committed source delivery identity: lot lineage, producing incarnation, batch ID, proposal digest, lane and actual runtime tick/ordinal. Identity survives retry and takeover.
2. Installed owner identity: authority/account/avatar/principal/session-ref/admission operation/control generation plus actor EntityRef (object ID and generation). Derive from committed owned registry or retained historical installation proof, never a client event.
3. Source EOD association: monotonically allocated invocation/session generation; exact invoker, object and avatar EntityRefs; plugin ID; joinable flag. New association must receive a fresh generation even within one ownership generation.
4. Native transport binding: current host scope/epoch, NativeHost SessionTicket and authenticated connection incarnation. New connection/host epoch cannot inherit authorization merely by presenting an old ticket.

A stale request may resolve only its original association. It cannot close a newer association. Duplicate exact request returns its original durable outcome; conflicting reuse is rejected. An active close cannot be acknowledged from absence alone: absence must be an authenticated complete-map result at the exact source boundary or a retained same-association close receipt.

## Public VM adapter

Append a bounded EOD map to SimState and append accepted-command/host-request/runtime-event variants; preserve existing tags. Schema migration must retain legacy unknown state. Snapshot only public source connection state and public object events, never private Timer handler or UI payloads.

Typed InvokePlugin requests latch the actual frame locals and exact entity generations. Timer connections and source teardown requests get stable IDs. Accepted object event delivery targets the exact association, retains FIFO order and marks Ended on -1. Replay/duplicates cannot add an event twice. An authoritative terminal object event does not immediately bypass the nonnegative-event-before-ended rule.

The old generic disconnect event remains a legacy request. New active requests must be association-bound. Owned departure cleanup may proceed only with complete association knowledge and actual terminal-close proof for the relevant owned actor; historical unknown/plugin-unsupported state stays blocked. Peer cancellation creates no peer departure.

## Private native candidate and durable outputs

An EOD change is a candidate, just like the existing simulation candidate. Its committed predecessor is the exact coordinator-held private EOD revision/hash, bound to the sim cursor and active host binding. Candidate work must never mutate committed EOD state before the coordinator decision.

A durable private transaction envelope must include the exact resulting private handler checkpoint, association mapping, stable source-request receipts, pending public object-event deliveries and pending private UI deliveries. The envelope is stored privately and atomically with its sim/journal decision. A separate output envelope or checkpoint format must retain pending queues; the existing checkpoint_to contract must not be silently weakened.

Public VM events remain pending until an accepted simulation batch applies their exact event IDs to the exact association. Private messages remain pending until the authenticated intended control/session acknowledges their exact delivery IDs, or a durable authenticated revocation/obsolete-association disposition makes delivery impossible. Such disposition is recorded separately from delivered. Sending a socket frame is not an acknowledgement. No private payload goes into shared ticks, replica snapshots, public status or another player's output.

The adapter must preserve source tick ordering: existing plugin tick before simulation, new source requests from the simulation then handler processing, synchronized object events delivered through ordered subsequent accepted commands. Private UI output is released only after the producing candidate commits.

Recovery must load the private envelope paired to the same committed sim cursor. NativeHost restore uses successor host epoch and exact expected private checkpoint stamp; active participant rebind requires current authenticated ownership. Pending old transport output may not be silently delivered to a different connection or discarded. Rebind/resync records a durable suppressed disposition for obsolete private output, not delivered. The replacement private checkpoint is staged before the takeover commit and persisted in the same transaction as the successor VM checkpoint. Activation requires the exact committed candidate hash. Consecutive recovery without an intervening simulation tick must restore the retained successor handler epoch. The generic input request may have a durable processed receipt while its produced UI/object-output queues remain pending; status must expose that distinction.

## Generic lane delivery

Add a private bounded page reader which reads live or archived committed batch bytes, verifies proposal digest and receipt, decodes the whole lane with bounded canonical native decoding, and preserves real ordinals. Never accept a browser-supplied decoded event list as evidence.

Retain per-event consumer dispositions. Capture/detach observation disposition requires the existing matching committed source-save/detach receipt and exact event ordinal/identity. EOD disposition requires the native transaction receipt described above. Other supervisor event/effect kinds remain pending unless a real handler exists. Only after every member of the original lane has a matching durable terminal disposition may the lane row retire, atomically with a tombstone that makes retry exact. No high-watermark bulk acknowledgement of mixed lanes.

## Qualification gates

- Original Timer handler invoked from typed source VM producer, not a manually invented association.
- Correct caller vs stack, locals, object/avatar generations, FIFO nonnegative events before close, source polling, reset/first-action cancellation/forced leave.
- Same owner opens/closes/reopens; an old request cannot close the later session. Peer cancellation remains generation-fenced and does not create departure.
- Two accounts cannot read or acknowledge each other's private output; forged ticket/control/association/epoch denied.
- Crash before commit, after commit before response, after public apply before ack, after private send before client ack, and after ack before reply. Restart must replay exact effects/messages with no duplicate VM application or false acknowledgement.
- Mixed generic lane retains unresolved member; archived batch still validates; conflicting duplicate rejected; stale owner cannot acknowledge after takeover.
- Legacy unknown and unsupported plugins remain fail-closed; no marker or acknowledgement changes that to known-clear.
- Final tests/builds must be explicitly coordinated with the single heavy-build owner. No qualification claims before those checks run.

## Bounded new-stream whole-lane consumer

The implementation target is only the three owned supervisor event kinds (wire tags 4, 5 and 6). The existing generic outbox row identifies a whole serialized events vector, not an individual event. A private page reader must read live/archive bytes, verify the accepted proposal digest and immutable decision, and retain the entire original vector. A strict bounded native decoder and an independent fixed-layout owned-event decoder must agree on all members, real ticks and ordinals. Any unsupported member, effect lane, missing proof or malformed bytes keeps the whole row pending.

Native classification is read-only against the committed paired VM/private checkpoint. Capture and detach require retained exact source owner, capture/detach tick and removal reason. A legacy EOD marker additionally requires its exact retained actor/control request, complete known-clear coverage, no nonterminal source association, and no pending native public/private output for any same-owner association. A later same-control session blocks the older marker until it too is terminal; no old marker is allowed to close a newer session. Peer markers without retained full owner proof remain pending.

Coordinator retirement independently matches the original vector to native classifications and rechecks current VM cursor, private checkpoint hash, full installed/historical owner identity, and original committed save/detach receipts (including producing batch/ordinal). Only a complete lane gets one atomic immutable receipt plus deletion of that exact outbox row. Exact retry returns that receipt; different original digest or proof identity is rejected. Receipt retention is bounded and fail-closed, with no high-watermark or blind bulk deletion. Existing immutable batch/archive anchors continue to prove the origin after compaction. Frozen schema-six rows retain their unknown-history blocker; this path does not turn migration into known-clear evidence.

## Explicit operational bounds

This is a bounded qualification increment, not an unbounded production retention policy. The private envelope is limited to 192 KiB, 64 retained source/native associations, 512 public and 512 private native ledger outputs, 4,096 source-request receipts and 4,096 UI receipts. Settled outputs still count. The follow-on producer omits Timer-only SimanticsDeliver notifications because original VMEODTimerPlugin registers no SimanticsHandlers for any event code. Polls still drain the public FIFO and preserve Ended/false-next-tick behavior; they allocate neither request IDs nor private receipts. Existing legacy no-op receipts remain untouched, and connect/disconnect requests still retain exact receipts. There is no per-poll lifetime growth. Real UI operations, outputs, associations and transport recoveries still consume the explicit retained-history bounds; safe archival for those histories remains a separate requirement. Exhaustion fails the candidate without changing committed state. The 24-hour native transport inactivity guard is an explicit native policy; original Timer has no equivalent timeout.

Transport resync is not source OnConnection. It reconstructs running/mode and last display from retained handler state, normalizing the original Timer_Set packed-minute representation until a genuine source Update replaces it. Arbitrary current invoker scratch temps cannot supply Show flags. Recovery does not synthesize an Update or advance source gameplay.


## Handlerless polling follow-on

Commit 9936 and its flow6 evidence remain the immutable earlier qualification. The producer optimization changes deterministic tick state because handlerless polls no longer advance the request ID; host and NativePlayer must use the matching implementation. It is not a mixed-version hot-upgrade protocol. Old accepted tails must retain their original runtime or be restored from their exact compatible paired checkpoint; this change does not rewrite historical decisions or bypass replay hash verification. Existing source snapshots/private receipts retain their representation and identities.

The separate 8,192 whole-lane retirement cap only rejects the consumer receipt/delete transaction and leaves its events row pending. It does not gate ordinary paired ticks or the independent dedicated save/release workflow. Continued unresolved-notification growth still needs operational handling; this is not evidence eviction or unbounded retention.
