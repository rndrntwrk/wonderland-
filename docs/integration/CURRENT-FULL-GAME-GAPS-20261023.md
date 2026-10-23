# Current full-game completion inventory — 23 October 2026

This is a bounded reconciliation of the current native path against the existing full FreeSO/Rust/browser scope, not exhaustive parity qualification. The published checkpoint remains PR #81 at `b0741032bddff45f408420b25967c91041da6aa3`, mapped to local `de5e9eae2564ae744b901837811648c92d52f537`. The accompanying local candidate adds only the authenticated avatar-creation HTTP boundary, its routing allowlists and focused regressions.

## Scope and evidence rules

The owner deliverable includes playable 3D, multiplayer, economy, creator tooling and refined UI. A–F, a static preview and a two-player Walk slice are intermediate milestones. Existing anchors:
- [Full scope and 20 concrete source obligations](FREESO-FULL-GAME-SCOPE-20261011.md).
- [21 preserved player surfaces](../design/action-first/player-capability-map.md).
- [Source preservation/intake](../compat/full-game-intake-20261012.md).
- [Existing integration reuse](RUST-REUSE-AND-INTEGRATION-GAPS-20261011.md).
- [Machine capability ledger](../compat/game-capabilities.json).

The ledger currently has 71 seeded rows: 41 unassessed and 30 partial; none is declared full-game qualified. Those seed rows are not a complete game denominator. Unresolved registration/build/submodule/content intake still prevents an exhaustive release claim. Later bounded native qualifications close specific paths without proving every historical gap remains or every reusable module is complete.

The 29 earlier hosting cases test boundaries, not exhaustive gameplay. Current live hosting remains the static Worker. Nothing below is a claim that the native game is publicly playable.

## Functional inventory

| Player capability | Working end-to-end evidence | Implemented but not complete/verified | Missing native completion |
|---|---|---|---|
| Sign-in, roster and character selection | Real local Argon2id/SQLite account login; two independent owned avatars selected and admitted | Revocation/rotation/interruption tested locally | Hosted onboarding/actual intended account cohort and production smoke |
| Character creation, retirement, appearance | Private account authority already creates independent head/body/gender/skin/name/description with durable operation dedup and three-Sim source policy | This local increment connects authenticated POST /native-account/create through the private HTTP bridge; it does not expose registration or arbitrary RPC | Native browser CAS UI, pre-entry eligible appearance/content loading, retirement/reconciliation and rendered new-player journey; the current native empty-roster screen has no Create control |
| City, directory, property/home entry | Native local authenticated entry to the configured lot passes | Legacy connected path has city terrain, directories, people/property views and home/location mapping | Native mode currently offers configured property buttons, not a complete city/directory/purchase service; native lot lifecycle and full city continuity remain |
| Animated movement/camera | Saved automatic-walk-04: two original models, actual deformation/animation, genuine source Walk completion, peer agreement, unchanged 45-second deadline | Existing render/camera/pose implementations and recovery | All source camera modes, custom/pet/contained rigs, full source terrain/scenery/material/lighting/weather breadth and hosted GPU/device qualification |
| Object/person interactions and queues | Original Walk source action completes; earlier Timer flow5/6 proves Be Timekeeper association, source events, private output isolation and recovery through native accounts/wire | Source offers, invocation, queue/cancel and source callback machinery exist; old controlled rendered action tests remain scoped | Rendered non-Walk owned-account journey; original Timer artwork/panel; remaining object/person families and held/slot/contact behavior. The authored Timer receipt explicitly records zero rendered Timer objects |
| Needs, skills and relationships | Source need projection and accepted native simulation algorithms exist | Native needs panel consumes projected values; source skills/relationship policies and old unit tests exist | Whole needs cycle through real objects, UI/authority skill controls, cross-lot persistence and relationship/social subscriptions; no current full-loop rendered acceptance |
| Buy, build, inventory and property rights | Prior controlled/RAM browser construction journey passed; runtime admission/quote/consent/recovery implementations exist | Purchase/floor native UI; broader wall/terrain/move/remove runtime and source architecture encoders; preview local saves | Owned native Buy/Build is explicitly disabled in native_lot.rs. Durable economic/inventory/catalog provider, complete tool families, pricing/capacity/ownership and restart-safe purchases remain |
| Persistence, reconnect, leaving/saving | Real local hosted-profile two-account cold recovery into empty cache; actors/registry and sessions preserved. Source-owned departure/save/fencing qualification retained | Durable native journal/checkpoint/receipt/outbox and account ownership stores exist | Cloud Container restart smoke, complete account/world/economy persistence and multi-lot lifecycle qualification; no claim of full-world saves |
| Multiplayer, chat, messages/mail | Two independent native accounts observe matching accepted state | Legacy connected chat/private messages/mail adapters and native original-chat projection are implemented | Native-owned UI/transport has no chat/message/mail path. Full social/service integration and durable delivery needed; shared world state alone is not chat parity |
| Audio, options and accessibility | Prior controlled native accepted-audio/device-cleanup journey exists | Accepted-event native audio driver, source decoders, Web Audio controls and device lifecycle | Authorized complete audio cohort/automatic active-lot wiring and audible original-object journeys, synchronized music/stations, full graphics/chat preferences |
| Wardrobe, shops, trading and economy | Account CAS ownership defaults and source wardrobe/economic foundations exist | Dresser/rack controls and native service/EOD handlers preserve source operations | Live native EOD UI/providers, stock/inventory, atomic two-party settlement, furnished property ownership side effects, business discounts and verified recovery |
| Jobs, bonuses, rankings, events and civic play | Original codecs/handler/state foundations exist | Job/EOD handlers, newspaper/payout requests, directory/election/announcement controls and scheduling contracts | Matchmaking/role/job-lot orchestration; durable earnings/tuning/visit/ranking/reward/election schedules and complete player journeys |
| Object-specific EODs, games and casino | Native Timer source event/recovery path has bounded local evidence | All 30 registered native handler translations exist; game rules/private state are reusable | Each enabled original handler needs actual VM/provider/UI/content/participants and durable recovery qualification. Original nonredeemable game-currency casino remains in scope; handler presence is not completed play |
| Creator tooling and content breadth | Existing Rust import/cooker/editor/render foundations preserved | Object/GUID, BHAV/resource/mesh/avatar editing and undo/redo implementations exist | Instruction-level live tracing/debug controls, complete cook/reload workflow, source-family/dependency/patch closure and permitted delivery for all enabled content |

Current line/path evidence includes:
- `apps/web-shell/src/native_account.rs`: current native roster/entry UI and empty-roster message.
- `services/native-edge/src/http/accounts.ts`: strict session-bound HTTP API.
- `apps/web-shell/src/native_lot.rs`: source queues/needs, accepted poses/audio and explicit `owned.is_some()` construction gate.
- `apps/web-shell/src/native_build.rs`: Purchase/Floor surface.
- `apps/web-shell/src/native_audio.rs` and `native_audio_browser.rs`: accepted-timeline audio/device path.
- `docs/verification/native-timer-delivery-20261015/RESULTS.md`: actual native Timer flows and explicit absence of rendered panel/artwork.
- `docs/integration/2026-10-11-renderer-buy-build.md`: prior renderer/build/audio browser evidence and RAM/provider limits.
- Published `docs/verification/native-hosted-preview-20261023/checkpoint-de5e9eae.md`: current local hosting/animation evidence and limits.

The 20 explicit additional source obligations remain: mesh reconstruction/replacement/cache; graphics/camera modes; city/lot/facade continuity; environmental rendering; job worlds; dynamic earnings; visitor bonuses/rankings; player shops; secure property trade; wear/repair/upgrades; property growth/capacity; community property; skills/social multipliers; complex avatar interaction; calendar events/cooldowns; election lifecycle; all EOD registrations; game-currency casino; Volcanic live authoring; and complete object-family breadth. Existing algorithms should be connected and qualified, not rewritten from this inventory. Original disabled/conditional features retain explicit dispositions, including job type 3, House Statistics/Bills/Log, event-gated content and unknown-EOD fallback.

## This bounded implementation unit

The missing native character-creation HTTP boundary can progress without hosting credentials. The candidate delegates to existing `AccountAuthority.createAvatar`; it does not replace catalog validation, source account capacity, ownership defaults or operation transactions. Exact appearance/operation fields pass through same-origin, bounded, authenticated HTTP. Browser-supplied account/principal authority is rejected. An explicit return type avoids unbounded Durable Object RPC type inference without runtime changes.

Regressions use the existing workerd/SQLite HTTP journey and private outbound transport. No external account or permission is created. The new UI and its resource authorization are explicitly unfinished. Existing compiled browser/native artifacts need no rebuild for this backend-only increment. No hosted/full-game/30 Hz pass is implied.

## Access and delivery boundary

No supported tool in this executor exposes the user's existing authenticated Prism session. Dashboard read is not attempted; no dashboard denial or product/plan fact is invented. A fresh unrelated browser, profile/cookie inspection or the previously denied R2 CLI operation is not substituted.

Hosting still needs owner-confirmed existing Workers Paid/R2 status/private bucket, narrowly approved R2 access, eight additional private startup inputs plus the actual hosted owner/cohort, qualified Linux image/build capability and all-in usage within the approved one-hour/USD1 preview. No new subscription, credentials/grants, installation, paid activation or public/private asset upload has occurred. Current source work is independent of these access gates.
