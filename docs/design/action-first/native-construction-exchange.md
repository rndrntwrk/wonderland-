# Native construction exchange and client transaction state

## Delivery boundary

Apply above PR #52, commit `ab28f56e36bdbf542f71233a6904493be992c1f8`.
This adds a binary exchange, server-session dispatcher, client transaction model,
and a byte-only Fetch adapter around the existing construction implementation.
It does not replace `ConstructionSession`, the native Build engine, or the
operator's durable accounting/ownership providers.

**The native Buy/Build button remains disabled.** There is no new map placement
panel, mounted application controller, HTTP route, production journal, or payment
provider in this patch. The Fetch module is provided for the mounting step; it
is not yet imported by the application or copied by its Trunk entrypoint.

## Implemented flow

1. The player selects edits. `client::Client::begin` emits a scoped Quote call.
   The request contains the existing restricted `Selection` values, never prices,
   owners, permission grants, reserved object IDs, or durable receipts.
2. An authenticated server route obtains its current `ConstructionGrant`, one
   retained `ConstructionSession`, and an operation ID from its journal. It calls
   `exchange::dispatch` on the serialized lot executor. The existing pure quote
   leaves the simulation, live allocator and balance untouched.
3. The reply echoes the entire binary call and exposes a quote hash, exact cost,
   operation identity and server-monotonic expiry. The client checks that echo
   and scope before entering Reviewing. Expiry is not a browser wall-clock value.
4. After explicit review, `Client::confirm` requires exactly the shown consent.
   Dispatch delegates to the existing `ConstructionSession::submit`, which
   revalidates the source quote and can admit BeginBuild once. An admitted request
   is Pending, not a completed geometry/accounting change.
5. The host processes the **actual `RuntimeEvent::BuildRequested`** in the admitted
   outcome and broadcasts its accepted TickFrame. Only the real trusted provider
   may later supply CompleteBuild. The exchange never manufactures that response.
6. The client reports Committed/Rejected/NeedsReconciliation only after the same
   admitted replica contains the matching operation/hash/cost/status. A server
   result arriving before that state stays AwaitingReplica. A generic checkpoint,
   tick advancement or unrelated operation cannot claim completion.

The executable probe uses a floor edit to demonstrate these steps through the
real native quote, admission, completion and replica engines. The grant, material
price, operation reservation and trusted completion are explicitly authored test
fixtures. It proves neither a database transaction nor production authentication.

## Reusing the parent contract

The two parent modules were recovered from live repository reads. Their exact
original Git blob identities were checked before the final local rerun:

| Parent file | Git blob |
| --- | --- |
| `construction.rs` | `cfc3233e0116beaa4e5da3f91b92f7e4b8059d9a` |
| `construction/session.rs` | `35b474f01656033310a9045c828747b0e8ea4f3f` |

The patch changes only `construction.rs` by exporting `exchange`. The session
file is not modified or included as a replacement.

## Protocol version 1

All integer fields stay in Rust as fixed-width little-endian values. JavaScript
passes bytes only; it must not decode u64 identifiers through `Number`.

The envelope is eight magic bytes and a little-endian u64 payload length, followed
by exact fixed-integer bincode. Calls use `WLBQ\x01\r\n\x1a`; replies use
`WLBR\x01\r\n\x1a`. Existing gameplay/checkpoint formats are unchanged.
Canonical streaming reserialization rejects trailing data and representations
that would otherwise normalize to different values. The nested decoder enforces
work, allocation-credit and depth limits before exposing values.

| Command | Operation |
| --- | --- |
| `Quote(ConstructionRequest)` | Obtain a pure quote; the separate dispatcher argument supplies the server-reserved operation. |
| `Confirm { operation, consent }` | Confirm the retained quote's exact request/hash/cost. |
| `Cancel { request_id, operation }` | Cancel only an exact unsubmitted quote. |
| `Status { request_id, operation }` | Read the exact session operation; operation may be absent only before the client received a quote. |

A Reply contains the full call bytes and an optional Snapshot. A missing Snapshot
is legal only for Status. It does not prove that an attempted operation failed or
never executed. An unrelated stored operation is a correlation error, not an
empty status response. Server-generated validation errors are not fabricated
commits; a host may report a sanitized error and the client must reconcile any
uncertain submission.

Maximum call size is 131,200 bytes; maximum reply size is 262,272 bytes. The
existing edit limit also applies. Nested decode limits are 1,048,576 work items,
64 MiB conservative allocation credit and depth 64. These are input admission
limits, not an RSS, network-buffer or crash-proof allocation guarantee.

## Server mounting obligations

`dispatch` is not authentication or an HTTP service. Before invoking it, the
actual route must authenticate the account, validate CSRF and Origin, authorize
the admitted Sim/lot, enforce request limits before buffering, serialize access
to the lot runtime, and resolve current account/catalogue/material revisions.
The non-deserializable ConstructionGrant must come from those trusted providers.

Reserve operation IDs idempotently against the admitted account and exact
request before quote dispatch. Repeated Quote calls must receive the same
reservation. A client-supplied request ID or call ID is not a durable operation ID.
The session must outlive socket replacement; process-restart recovery must restore
its quote/consent/operation correlation from a durable journal, not create a
fresh empty session and infer that nothing happened.

`Handled` deliberately contains both `admitted: Option<AdmittedBuild>` and a
separate `reply: Result<Vec<u8>>`. Route and journal the admitted outcome even when
reply construction or network delivery fails. In particular, do not use `?` on
`handled.reply` first and accidentally discard its BuildRequested event/frame.
The host must reconcile an execution exception against the existing operation,
not allocate a replacement operation on a retry.

Duplicate suppression in the parent RAM session is not crash-safe exactly-once
accounting. Durable accounting/ownership, operation completion and reconciliation
remain provider work. No journal implementation or persistence format is supplied
by this patch.

## Client mounting obligations

Create one `Client` for one admitted NativePlayer binding, lot and authority epoch.
Capture `Prepared.connection` when starting an asynchronous request and pass that
same token to `accept`. Never read a new token from inside an older callback.
The full echoed request additionally binds the response to its selections and
revisions. Only pass `RuntimeProjection` values from that admitted NativePlayer's
validated stream to `observe`; the public projection type is not authentication.

The important display states are Reviewing, Sending, Pending, Unknown,
AwaitingReplica, Committed, Rejected and Reconciliation. Quoting does not place
anything, and confirmation does not optimistically change geometry or balance.
A Reconciliation state blocks another quote until the host resolves its obligation.
Terminal outcomes cannot be erased by an absent later status record; cancelled
or expired quotes cannot be reopened by a contradictory late reply.

On interruption, call `disconnect`, replace the transport, reconnect the same
client, and explicitly request `poll()`. Poll prepares only Status, never Confirm.
An uncertain operation blocks a new quote. A trusted same-session status that
says the original quote was never submitted can return the user to explicit
review; it is not permission for an automatic resubmission. This depends on the
server's durable correlation after restart. Closing this client is not cancelling
a debit: persist its unresolved journal/recovery record independently first.

The optional Fetch adapter takes an absolute configured endpoint and its exact
allowed origin, a server-issued CSRF token and a bounded timeout. It issues one
credentialed POST, refuses redirects/foreign origins, and checks response MIME,
URL, byte count and a 4,096-read work ceiling. It copies WASM-owned bytes before
awaiting. Any failure after dispatch closes that channel with DELIVERY_UNKNOWN.
Late responses, stalled streams and uncooperative cleanup cannot trigger retries.

The expected request MIME is `application/vnd.wonderland.construction-call`;
response MIME is `application/vnd.wonderland.construction-reply`. Cross-origin
use requires the real server to allow only the configured credentialed Origin
and the Content-Type/X-Wonderland-CSRF headers. The helper does not substitute
for server-side authentication, CSRF checks, buffering caps or rate limits.

## Verification and reproduction

From an actual checkout of the pinned #52 parent with this patch applied:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-game-runtime --target wasm32-unknown-unknown \
  --lib --locked -- -D warnings
node --test --test-reporter=tap apps/web-shell/scripts/native-*.test.mjs
node --test --test-reporter=tap crates/audio-runtime/browser/*.test.mjs
cargo run -p wonderland-game-runtime --example construction_exchange_probe \
  --locked > /tmp/construction-native.json
cargo build -p wonderland-game-runtime --example construction_exchange_probe \
  --target wasm32-unknown-unknown --release --locked
node crates/game-runtime/scripts/verify-construction-exchange.mjs \
  /tmp/construction-native.json \
  target/wasm32-unknown-unknown/release/examples/construction_exchange_probe.wasm
```

Add `--offline` to Cargo commands when the locked dependencies are already cached.
The added read-only CI workflow reproduces these tests after publication. It has
not run remotely for this unpushed patch.

The delivered evidence records 25 integration tests, three client-unit boundary
tests and 15 Fetch tests. Fetch testing includes actual loopback Node HTTP with
a lost reply after the server received the request; other transport edge cases
use declared test adapters. The ordinary-WASM probe executes the same runtime,
preserves identifiers above JavaScript's safe integer range, and compares the
complete output record plus eight negative controls. It is not a rendered-browser
test or distinct-player test.

The current local verification checkout is reconstructed PR47 plus the exact
parent construction modules and this new exchange. It is **not** the complete #52
tree: the #49 checkpoint patch and #51/#52 test files were not recovered into that
checkout. Its broader workspace count must not be presented as the full #52 suite,
or as proof those omitted cases passed. Run the full pinned-parent gate before
merging. The patch includes no reconstruction changes or original resource pack.

## Remaining game work

Mount the action-focused placement/cost/confirmation UI, publish authenticated
routes and real capability/catalogue/account providers, and integrate the durable
journal. Then verify real accounts, object ownership, inventory, construction,
reconnect and reload together in the built player. Full original Build families,
inventory storage/selling, property/social/object dialogs, content/contact-animation
acceptance and original FSOv/VMNet translation are not completed here. No merge,
deployment, original artwork certification or independent review is claimed.
