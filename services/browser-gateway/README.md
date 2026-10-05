# Original-service browser gateway

This native Rust process connects the browser to the original account HTTP
service and original city/lot Aries TCP servers. It does not host an account
database, simulate a city server, or turn an outbound socket write into a
successful game action.

`wonderland-game-services` supplies the shared browser DTOs, source HTTP/XML
parsers, typed directory routes and original packet encodings. This process adds
HTTP transport, in-memory account sessions, destination admission, native socket
lifecycle, operation correlation and server-observed EOD authority.

## Run

From the workspace root:

```sh
cargo run -p wonderland-browser-gateway
```

The default listener is `127.0.0.1:8787`. With no account service configured,
`GET /health` returns `configured: false`, and account operations return the typed
`not_configured` error. No deployed server or credentials are assumed.

An operator supplies these settings for their own original server deployment:

| Environment variable | Meaning |
| --- | --- |
| `WONDERLAND_GATEWAY_BIND` | Native HTTP listen address; default `127.0.0.1:8787`. |
| `WONDERLAND_API_BASE_URL` | Original account/directory HTTP base. HTTPS is required for a remote host; loopback HTTP supports local development. No username, password, query or fragment is allowed in this URL. |
| `WONDERLAND_BROWSER_ORIGINS` | Comma-separated exact browser origins, including scheme and any non-default port. The default allows no browser origins. |
| `WONDERLAND_TCP_ALLOWLIST` | Comma-separated final city/lot `host:port` destinations. DNS is resolved and pinned at startup. The default allows no TCP destinations. |

The legacy client literally appends `101` to each server-selected connection
address. For example, a source address `city.example:2` names final destination
`city.example:2101`; the allowlist must contain that final destination. This
behavior comes from the original city and lot connection controllers. The
browser never supplies a destination host, TCP port, source ticket or actor ID
for authentication. City destinations come from the authenticated shard
selector; lot destinations and tickets come from the authenticated city's
`FindLotResponse`.

For a remote browser, terminate HTTPS/WSS in an operator-managed reverse proxy
and forward the gateway routes to the native listener. The process does not
generate TLS certificates or publish itself. Configure the exact page origin,
not a wildcard. Native clients without an `Origin` header are supported.

Library users can call `app(GatewayConfig)` and
`DestinationAllowlist::from_pinned` to integrate the router and already-resolved
operator endpoints. The gateway session resource quota is `max_sessions` in
`GatewayConfig` (default 128); this is a process resource budget, not a game
account/avatar/property limit.

## Browser HTTP contract

All responses use `Cache-Control: no-store`. Session credentials appear only in
JSON request/response bodies or the `Authorization` header; gateway query
strings are rejected. An error has `{ "code": "...", "message": "..." }`, with
bounded, non-secret text. No request-body, password, token or source-ticket
logging is installed.

| Method and path | Input | Successful result |
| --- | --- | --- |
| `GET /health` | None | `GatewayHealth`, including protocol version 1 and capability availability. |
| `POST /v1/sessions` | `LoginRequest { username, password }` JSON | `SessionCreated { session_token, session, roster, shards }`. |
| `GET /v1/session` | `Authorization: Bearer <gateway-session-token>` | `SessionProjection` directly. |
| `DELETE /v1/session` | Same bearer header | HTTP 204. Revokes that session and cancels its native connections and active browser socket. |
| `GET /v1/roster` | Same bearer header | `Vec<RosterEntry>` directly, refreshed from original authenticated XML. |
| `GET /v1/shards` | None | `Vec<Shard>` directly. |
| `POST /v1/query` | `DirectoryRequest { query }` | `DirectoryResult { query, data }`; `data` preserves original JSON fields, pages and totals. |
| `GET /v1/ws` | WebSocket upgrade | Authenticate in the first text frame, below. |

The upstream login is the original form POST to `/userapi/oauth/token` with
`username`, `password`, and `permission_level=1`. JSON OAuth failures are rejected
even when the HTTP status is 200. Redirects are disabled, including redirects
that could forward credentials. The bearer token is retained only in the native
session. Roster, shard-list and shard-selector XML are decoded using their exact
source roots and fields; malformed documents, DTDs and trailing documents fail
closed.

Roster `head_key` and `body_key` are decimal strings representing the complete
64-bit source resource key. Optional home fields are a single coherent group.
The source roster does not contain live money or motives: those fields remain
`null`. Login success never manufactures a live HUD or admitted lot.

Directory queries cover original avatar/profile, avatar name/page/batch/online,
lot name/page/batch/location/online/top-category, neighborhood, election and
bulletin reads. Query enum variants are defined in
`crates/game-services/src/directory.rs`; they cannot specify an arbitrary URL.
Source pages start at 1. Per-response limits reject oversized requests rather
than truncate a roster or hide additional pages.

## Browser WebSocket contract

The first message, within five seconds of upgrade, is:

```json
{"type":"authenticate","session_token":"<gateway-session-token>"}
```

The gateway grants one active browser socket per account session. Each new
socket lifetime advances `SessionProjection.epoch` and starts with authenticated
account state and no selected city/lot. Use the epoch in the first `session`
event, rather than assuming it equals the epoch returned by the login request.

Subsequent operations use a unique ID within this socket epoch:

```json
{
  "type":"request",
  "epoch":2,
  "operation_id":"ui-42",
  "operation":{"type":"connect_city","shard_name":"City One","avatar_id":42}
}
```

IDs contain 1–96 ASCII letters, digits, `-`, `_`, `:`, or `.`. The gateway rejects
an already-submitted ID even after the city disconnects within the same socket
epoch. An ID is not a server transaction ID. Uncorrelated legacy response
families are serialized so an old response cannot acknowledge a newer request.
Clients must not replay unknown writes after reconnect.

Canceling an in-flight `FindLotRequest` reports its outcome as unknown but keeps
its uncorrelated response family occupied. The next lot admission remains
unavailable until that old reply is drained, or the city transport is replaced.
This also covers job requests whose response location is remapped by the source
server. A drained reply never opens a lot socket.

Every response is a `GatewayEnvelope`:

```json
{
  "epoch":2,
  "operation_id":"ui-42",
  "event":{"type":"pending","family":"city_admission"}
}
```

An event is `session`, `pending`, `outcome`, `roster`, `directory`, `source_event`,
`vm_frame`, or `error`. Outcomes distinguish:

| Outcome | Meaning |
| --- | --- |
| `accepted` | The relevant original response confirmed the operation, original admission completed, a requested read returned, or a local departure completed. It is not used merely because bytes were written. |
| `rejected` | Input/session admission failed or the original server returned a rejection. `source_code` and original response fields are preserved where available. |
| `unknown` | A submitted operation lacks a correlated original receipt, or its pending outcome was lost through departure, disconnect or timeout. |

A 30-second pending timeout ends the native/browser lifetime and marks pending
outcomes unknown. It does not open that same uncorrelated response family to a
new request on the old transport. Source events without a correlated operation
have a null operation ID.

The authenticated socket epoch, selected avatar and `lot_incarnation` are
independent guards. Each admitted lot connection gets a new local incarnation,
including rejoining the same packed location. Leaving clears it. Native reader
tasks also carry generations, so a queued packet from a departed connection
cannot enter the replacement lot. The source VM does not itself provide this
gateway incarnation or authoritative entity-generation IDs.

## Implemented original operations

All variants are in `GatewayOperation`, using snake-case JSON tags.

| Family | Operations and authority |
| --- | --- |
| Account/city | Refresh roster; select an owned roster avatar; anonymous avatar-0 CAS session; original CreateASim; retire avatar; disconnect city. Selection uses authenticated source XML, followed by challenge/ticket response, `HostOnlinePDU`, and source `ClientOnlinePDU` plus original city setup packets. Retirement has no durable correlated receipt. |
| Lot admission | `JoinLot` uses packed location and original `FindLotRequest`. The city's returned user must match the selected avatar. Lot handshake uses its own server-issued ticket. Readiness requires a valid original VM payload, not an arbitrary packet or socket open. `LeaveLot` cancels pending admission and snapshot refresh. |
| Social | Find avatar; private message; incoming messages and delivery ACKs. Source sender is the authenticated avatar. A private-message ACK is delivery acknowledgement, not proof that a person read it. |
| Mail | Poll from original .NET ticks; send; delete; incoming mail. SEND_SUCCESS returns the original canonical item. Delete has no correlated receipt and therefore reports unknown submission. Original nested message item IDs, timestamps, read-state and reply fields are retained. |
| Property | Purchase/move lot and source flags; roommate invite/kick/accept/decline/poll. Incoming roommate invitations are original `ChangeRoommateRequest` packets, including owner avatar and packed lot location. |
| Neighborhood | Source vote, nomination, rating, candidate-run, free-vote and permission queries. Candidate-list events carry operation correlation and requested neighborhood/action metadata. The source's already-voted/already-nominated response followed by candidates and SUCCESS is drained without turning the rejection into acceptance. |
| Bulletin | Read, post, promote, permission check and delete. Original permission/status codes and canonical message items are returned. |
| Lot chat | Original VM chat command with the selected actor. A write reports unknown, without a fabricated echo or accepted queue item. |
| Authoring | `LotCommand { lot_incarnation, data }` uses the independent original player-authoring validator. Only source-supported architecture, buy, move, delete, inventory send/place and roof commands pass; another actor or server-only fields fail before a socket write. Source command 33 SetOutfit is not browser-authorable. |
| Source Goto | `WalkTo { lot_incarnation, interaction, param0, x, y, level }` preserves the original source command 10 fields. Both menu fields are required; there is no coordinate-only default. The UI must obtain actual GotoObject menu offers before exposing the action. Submission reports unknown; the original server decides execution. |
| Cancel interaction | `CancelInteraction { lot_incarnation, action_uid }` injects the selected actor into source command 7 and preserves the original queue UID. Submission reports unknown; the gateway does not pretend the action was canceled in the VM. |
| Snapshot refresh | `RequestWorldSnapshot { lot_incarnation }` emits source command 13 with its original actorless layout, reason 0 and last observed tick. It remains pending until a valid original StateSync for the same admitted packed lot is received. A five-second request budget prevents rapid repeated full snapshots. Acceptance means snapshot received, not simulation execution parity or acknowledgement of earlier writes. |
| EOD/wardrobe | Only a matching server-observed actor/plugin `eod_enter` grants an EOD incarnation. Original tick/direct commands are decoded atomically with `wonderland-vm-protocol`; no byte scanning is used. Enter/leave replay, wrong actor/plugin, stale incarnation, malformed packets and changed lot/snapshot cannot preserve old authority. Original EOD messages retain text/binary distinction and binary u16 length. Source SetOutfit emits exact `uid`, scope and decimal asset key; ActorUID can legitimately be zero. |

`source_event` family `eod` includes `actor_uid`, `plugin_id`, `event_name`,
`text` or `binary`, EOD `incarnation`, and `lot_incarnation`. `set_outfit` includes
`actor_uid`, authoritative avatar `uid`, `scope`, decimal-string `asset_id`, and
`lot_incarnation`. These observations do not by themselves confirm arbitrary
pending EOD/authoring writes. The corresponding UI must use actual source events
and source state for its own result presentation.

The original server can send a cached or asynchronously serialized StateSync
with an older tick number than recently broadcast live ticks, then send the
history since that snapshot. A matching source snapshot is accepted independently
of the normal tick freshness check; replayed historical EOD commands cannot
restore a cleared permission. Original `ImmediateMode` commands have no normal
simulation tick sequence, so an immediate EOD leave still revokes the plugin
without advancing or lowering the ordinary tick high-water mark.

## VM boundary and remaining implementation gaps

`vm_frame` carries the bounded original packet payload and its lot incarnation.
The native gateway decodes source ticks/direct commands for admission, EOD and
snapshot checks. A `vm_decode_error` source event describes a rejected decode;
the raw payload is still available to an explicit runtime adapter. Unsupported
original snapshot versions or modes are not replaced with a fixture or a guessed
world.

The source protocol decoder supports the documented source v38 TSO snapshot and
known command layouts, but this gateway does not restore that state into the A
simulation runtime or claim deterministic source tick/RNG/interaction parity.
The connected world consumer may present a validated source snapshot and request
a new one; a forwarded socket frame is not a complete gameplay runtime.

The following remain explicit adapter gaps:

- Original data-service model subscriptions and authenticated model updates for
  profile editing, bookmarks/ignore lists and rich relationship/live presence
  vectors. Public profile/directory reads are available separately.
- A complete original inventory/catalog model subscription and portable item
  state restoration. Supported native inventory commands still defer every
  ownership, budget and execution decision to the original server.
- Full source interaction menu/check-tree discovery and execution parity. A
  coordinate alone does not identify the original GotoObject interaction and
  Param0; those must come from original content/menu authority.
- End-to-end live VM restoration, continuous simulation, complete object
  interactions, job/game-specific EOD interfaces, and source HUD authority. The
  raw protocol/read boundaries are available without claiming these adapters.
- Actual compatibility with an operator's deployed service configuration. This
  implementation was tested with controlled original-wire peers, not a live
  external account or city.

Capabilities are explicit. `account`/`directory`/`city` require configuration;
city social/property/mail/neighborhood/bulletin operations require a selected
avatar; `lot_chat`, `lot_command`, `walk`, `cancel_interaction`, and `world_snapshot`
require lot admission. `eod` requires an observed plugin; `wardrobe` further
requires the known source dresser/rack plugin. `live_world`, `live_hud`,
`profile_edit`, `bookmarks`, and `inventory` remain unavailable at this gateway
boundary while their named adapters are absent.

## Resource boundaries and verification

HTTP responses are bounded at 8 MiB before parsing. Browser JSON/body frames are
bounded at 128 KiB. Original Aries payloads are bounded at 4 MiB before allocation;
native reader queues, handshake time/packet budgets and response correlation
tables are bounded. Post-admission native writes have a five-second deadline;
a timeout can follow a partial write and closes the transport with unknown
pending outcomes, without retrying the bytes. Original VM decoding has independent command, entity,
collection, input and decompression budgets. Oversized data produces an explicit
error; limits never silently clip an account roster or fabricate a game product
limit. Passwords/tokens/tickets are memory-only and absent from Debug output for
the credential-bearing types.

Run focused verification from the workspace root:

```sh
cargo test -p wonderland-game-services -p wonderland-browser-gateway
cargo check -p wonderland-game-services --target wasm32-unknown-unknown
```

An explicitly invoked local example supports manual or native end-to-end
investigation without using a deployed service:

```sh
WONDERLAND_REPLAY_BROWSER_ORIGINS='http://127.0.0.1:8080' \
  cargo run -p wonderland-browser-gateway --example controlled_replay
```

Use the actual page origin in that variable. The example gateway listens on
`http://127.0.0.1:18787`; `WONDERLAND_REPLAY_BIND` can change the port but must stay
on loopback. Its declared synthetic account is username `controlled-player`,
password `test-only`, selected through the normal account login flow. There is
no authentication bypass route. The example is not included in production
startup and is never selected as a fallback for an unavailable live service.

The controlled directory has avatar 42, city 7, database lot 1 at packed location
55, and neighborhood 99. The lot sends the synthetic source-v38 object fixture
and repeats its cached source StateSync for explicit refresh requests. It has
one fixture object and no running source simulation, avatar/HUD fabrication,
owned wardrobe, persistent mutation service or external account authority.
Other VM commands do not mutate the fixture or receive fabricated acceptance.
This is a controlled protocol test surface, not evidence of a live-server or
completed browser-authentication journey.

The tests cover original XML/resource keys and OAuth errors, URL/HTTP redirect
boundaries, malformed original packets, selected destination restrictions,
native ticket handshakes, WebSocket session isolation/epochs, operation replay,
pending versus server rejection, original EOD authority, source snapshots and
same-lot reincarnations. The end-to-end replay uses local controlled peers and
the explicitly test-only synthetic golden source-wire fixtures in
`crates/vm-protocol/tests/fixtures`. No test fixture is a production backend or
bundled live world.

Source anchors: `TSOClient/FSO.Server.Api.Core/Controllers`, original
`FSO.Server.Protocol/{Aries,Voltron,Electron}`, city handlers under
`FSO.Server/Servers/City/Handlers`, client city/lot connection controllers,
`tso.simantics/NetPlay/Model`, and `tso.simantics/NetPlay/Model/Commands`.
