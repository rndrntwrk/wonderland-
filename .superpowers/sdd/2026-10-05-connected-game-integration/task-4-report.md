# Task 4 — connected account and player panels

## Delivered integration

`ConnectedGame { gateway_url: String }` in `apps/web-shell/src/connected.rs` is the independent connected entry point. Root mounts it only after operator configuration has selected connected mode. The component creates `ConnectedUi`, `ContentUi`, `ContentTarget`, `AvatarMotion`, and `SourceAuthoringUi` contexts. It routes account selection, source character creation, city admission, and `ConnectedLotView` without constructing a preview provider or opening preview saves.

The browser adapter uses the concrete DTOs in `crates/game-services/src/{account,directory,gateway}.rs`. It does not invent an upstream host. HTTP requests use `/health`, `/v1/sessions`, `/v1/session`, `/v1/roster`, and `/v1/query`; the WebSocket at `/v1/ws` authenticates with its first JSON message. Tokens are never put into URLs or browser persistence. Password inputs clear on submission. The gateway token and socket resources remain in memory and are removed on logout. Logout also aborts active HTTP reads, closes the socket, clears private session state, and revokes the captured gateway token through the actual DELETE endpoint.

## Player surfaces

| Surface | Implemented behavior and source boundary |
| --- | --- |
| Account and roster | Real account login, complete paged roster, actual shard cards and status, selected Sim stage, roster refresh, source city admission, explicit Go home using the selected roster home, and reconnect/sign out. No hardcoded avatar capacity and no substitute account identities. Unknown outfit/skin data uses a neutral existing icon. |
| Character creation | Reuses the original resource importer and separate source head/body collections, skin and gender choices, name/description drafts, original animated avatar stage, and lossless packed outfit IDs. Source CAS requires anonymous avatar 0 admission. Create sends `CreateAvatar` and refreshes the real roster only after the gateway reports acceptance. Retirement is confirmed and restricted to the actual selected account actor; the server decides whether it is allowed. |
| City discovery | Actual `Shard.map` and packed source locations feed `SourceCityMap`. Lot database IDs remain distinct from locations. All source directory pages are reachable; search, online lists, Top 100 category filters, people, neighborhoods, and per-selection full details are wired. No service pins are overlaid onto the unrelated coastal illustration. |
| Profile and people | Full profile request on selection, source description/gender/date, available job information, source needs when supplied, private-message/mail/Find actions, actor-owned retirement, and explicit unavailable profile/relationship adapters. Find results must belong to the selected Sim and current source session before offering the returned property. |
| Property | Full source details, owner/resident links, packed-location Visit and Open actions, neighborhood navigation, source roommate invitation poll, actual received invitations, Accept/Decline, Invite, and confirmed Kick/Move out. Ownership guards require a real nonzero actor and matching source owner; two missing IDs cannot grant an owner control. |
| Chat and mail | Source private messages, source identity deduplication, local unread/selection state, recipient drafts, correlated outgoing delivery state, source mail polling and merging, full-mail refresh reconciliation, source reply/send/delete requests, and explicit unknown results when the original protocol has no receipt. Drafts are cleared only after an accepted result and only if they still equal the submitted draft. |
| Neighborhood | Full source profile, mayor and elected date, town hall, neighborhood people/lots, public bulletin filters/details, and election cycle/candidates. |
| Civic actions | Source eligibility checks precede posting, voting, nomination, rating, running, and selecting a voting neighborhood. Candidate lists are tied to the exact source request operation ID; free-vote candidate IDs retain their original neighborhood meaning. Ratings use the original half-star range and bounded comment. Final choices require confirmation and remain pending/rejected/unknown until the gateway reports their actual result. |
| Bulletin actions | Selected bulletin gets its own detail request and view. Post, Delete and Promote use source operations and actor/mayor checks. Posting requires a current accepted source permission probe. Body drafts survive a rejection. |
| Wardrobe and EOD | Mounts Task 5's `ConnectedAuthoringPanel(Wardrobe)` through `SourceAuthoringUi::attach`. Active object dialog identity is admitted from actual EOD enter/leave events and the current native lot incarnation. Other source EOD families have named contextual states and explicit unavailable control adapters. |
| Options and chrome | Existing blue game chrome, responsive player overlays, keyboard Escape/return focus, native confirmation dialogs, reduced motion, actual source audio controls, switch Sim and sign out. |

Relevant original contracts were checked against `TSOClient/FSO.Server.Api.Core/Controllers/{AvatarInfoController,LotInfoController,NhoodInfoController,BulletinInfoController,ElectionInfoController}.cs`, `TSOClient/tso.common/Enum/LotCategory.cs`, `TSOClient/FSO.Server/Servers/City/Handlers/ChangeRoommateHandler.cs`, `TSOClient/FSO.Server.Protocol/Electron/Packets/ChangeRoommateRequest.cs`, `TSOClient/tso.client/Controllers/{BulletinActionController,NeighborhoodActionController}.cs`, and the integration audit. The invitation request decoder missing from the initial gateway shape was coordinated with Task 1 and is now emitted as `roommate_invitation`.

## Session and response guarantees

- Local browser login epochs reject late login/read callbacks after logout or another login. Named read request stamps reject an older selection's reply.
- Wire requests use the actual `SessionProjection.epoch`, independently of the browser epoch. Incoming source envelopes cannot regress a newer source session.
- Native lot frames are accepted only for the admitted `LotReady` incarnation. `latest_vm` is invalidated on source session/lot changes and socket interruption; the separate lot adapter performs full source decoding.
- Balances and needs are cleared on source epoch, selected actor, or native lot-incarnation changes; leaving `LotReady`; disconnect; reconnect; and send failure. Previous lot values cannot become the next lot's HUD.
- Losing a socket marks unconfirmed actions unknown. Reconnect does not replay mutations or purchases. Drafts, selected conversations, and unread state survive interruption for the same actor. A different admitted avatar clears private messages/mail/drafts and old permission probes.
- A malformed/closed socket invalidates its callback generation before deferred resource cleanup; later callbacks from that socket cannot restore it.
- Streamed HTTP bodies use the original HTTP adapter’s 8 MiB quota and are bounded before accumulation beyond it. Oversized responses fail as a whole; they do not truncate directory rows. Incoming WebSocket envelopes permit `4 * MAX_ARIES_PAYLOAD + 4096` bytes (16 MiB plus 4 KiB), accounting for the worst-case JSON integer-array representation of a 4 MiB native frame; decoded raw frame data is independently limited to the original 4 MiB quota. Outgoing requests use the gateway’s 128 KiB incoming message quota and are locally rejected before sending if too large. Requests time out after 30 seconds and use omitted ambient credentials, no-store caching, and rejected redirects.
- Capability checks and operation feedback reflect source availability, pending, accepted, rejected, or unknown. A successful browser socket write never becomes a successful game action.

## Reusable source content changes

`avatar_content.rs` now accepts an optional `ContentTarget` context with busy/revision/install callbacks and an `AvatarMotion` context. Existing `ContentLoader()` callers continue to derive their target from `AuthorUi`; preview busy checks, load generation, revision checks, and provider install behavior remain. `ContentUi.choices` exposes the imported source metadata to the connected creator. `AvatarStage` can receive reduced-motion state without depending on preview `Ui`. No preview provider or save implementation was edited.

## Verification evidence

The native tests are in `apps/web-shell/tests/{connected_adapter,connected_events}.rs`. TDD failures were observed before the relevant reducer implementation, including lost-session draft preservation, cross-avatar private isolation, source roommate invitations, and HUD invalidation.

| Command/check | Observed result |
| --- | --- |
| `cargo test -p wonderland-web-shell --test connected_events --test connected_adapter` | **24 passed, 0 failed**, last focused run in `/tmp/task4-budgets-green.log`. Covers late login/logout, newer login, latest selection reads, rejected/unknown sends, exact-draft clearing, unread/dedup behavior, complete directory pages, response bounds, lossless `u64` outfit IDs, source session transitions, full mail reconciliation, roommate acceptance, lot HUD invalidation, real serialized maximum VM frame expansion, decoded raw frame overflow, envelope overflow, and an HTTP response between 4 and 8 MiB. |
| `cargo check -p wonderland-web-shell --target wasm32-unknown-unknown` | **Passed**, complete connected city/lot/authoring integration in `/tmp/task4-wasm-check3.log` (36.45 seconds). Only existing audio unused imports and the proc-macro future compatibility warning remained. |
| Owned-file Rust formatting and `git diff --check` | Passed after the final source edits. |
| Final full-workspace/native/WASM/release/browser gates | Root owns these gates. Root was informed of the final category/detail/label/icon/HUD/WalkTo and transport-budget edits after the successful focused WASM check and will compile that exact final tree. No full-suite or live-browser claim is made by this task. |

The independent gateway review identified a valid-frame failure caused by applying the original 4 MiB raw quota to its larger JSON integer array. Two acceptance tests failed before the split-budget correction, then the complete 24-test set passed. The new envelope bound is derived from the source `MAX_ARIES_PAYLOAD`; it does not widen the accepted raw source frame. The existing oversized HTTP test now exercises the aligned 8 MiB limit.

No live service credentials or running original city/lot server were supplied to this task. Actual backend acceptance, gateway CORS/deployment configuration, source content import in the final browser, keyboard/touch layout, and full preview regression must be checked in root's integrated environment.

## Final capability audit: Go home

Added the roster **Go home** action beside Play. It reads only `RosterEntry.home.location`; the home database ID is retained for identity validation and is never substituted for the packed location. A matching `CityReady` avatar/shard sends `JoinLot` immediately. Otherwise an explicit user intent is armed against the exact written `ConnectCity` operation ID, browser epoch, source epoch, avatar, shard, home lot ID, and home location. The one-shot follow-up requires both the matching accepted city receipt and matching `CityReady` projection, in either arrival order.

Changing the Sim/shard/home, a new source epoch, source disconnection, browser reconnect/logout, or an explicit competing city/lot/create/retire/residence action cancels the intent. Rejected and unknown admissions never retry. An absent or unusable source home disables the button with a reason. Go-home lot rejections are surfaced through the existing global feedback. `OpenIfClosed` remains false, matching the original `PersonSelectionController.ConnectToAvatar` and `LotConnectionRegulator` request construction.

Eight focused native guards were added in `apps/web-shell/tests/connected_home.rs` for direct entry, exact response correlation, both arrival orders, one-shot consumption, terminal rejection/unknown, selection/home mismatch, avatar/shard/source-epoch mismatch, and disconnect/logout/missing-home behavior. The initial RED-stage compiler attempts failed before running tests because the changing shared `world-view` dependency chain lacked its matching rlib; root then requested that workers stop compiler invocations for a single clean final chain. Accordingly no behavioral RED or passing result is claimed for this additional target here. Root owns `cargo test -p wonderland-web-shell --test connected_home` and the final WASM gate. Owned-file formatting and diff whitespace checks passed.

## Explicit remaining source adapters

This handoff is not a claim of full legacy gameplay parity. Account bookmarks/relationships, editable profile fields and complete job/skill records need their original data-service provider. Property purchase quotes and comprehensive admission/donor/environment/resize management data are not supplied by the current directory API, so this UI does not manufacture terms, permission sets, prices, or successful purchases. Most specialized EOD games/jobs/trade/music/sign/door controls still need typed browser UI adapters; their actual session is shown with an explicit unavailable control state. The Task 5 authoring and root live-world adapters own their respective source protocol coverage and limitations.

Connected lot chat submission is wired to the real gateway; this task does not claim a complete incoming lot-chat VM playback adapter. Source HTTP bulletin lists and mail refreshes are used for reconciliation; an original unacknowledged delete remains unknown until a new authoritative read shows removal.

## Owned files

- `apps/web-shell/src/connected_adapter.rs`
- `apps/web-shell/src/connected_adapter/state.rs`
- `apps/web-shell/src/connected_bridge.rs`
- `apps/web-shell/src/connected.rs`
- `apps/web-shell/src/components/connected_player.rs`
- `apps/web-shell/src/components/player_menu.rs` (added connected component; preview component retained)
- `apps/web-shell/src/avatar_content.rs` (authorized shared importer refactor)
- `apps/web-shell/public/connected.css`
- `apps/web-shell/tests/connected_adapter.rs`
- `apps/web-shell/tests/connected_events.rs`
- `apps/web-shell/tests/connected_home.rs`
- This report

Shared root modules, manifests/lockfile, config, style imports, source city renderer, connected lot renderer, gateway, VM protocol, and authoring implementation are owned and committed by their respective workers.
