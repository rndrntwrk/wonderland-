# Wonderland: gaps to a hosted playable game

**Review date:** 11 October 2026, UTC  
**Inspected source:** `961e97d19900198a8591a79b8f11ced20bef3996`, based on merged master `2e3197185a98092b26ad3e1453d33cfe17125a18`. The additional commit restores a portrait fallback icon; it does not add a backend or gameplay. Paths below are relative to the repository root.  
**Method:** read-only source, packaged-preview and existing execution-evidence review. No new builds, tests, service operations or deployments were performed for this audit. This file is the sole documentation deliverable.  
**Decision status:** the recommendation to specify native Rust Containers with per-lot Durable Object coordination and durable storage is approved for specification. The full specification and implementation/deployment are not approved or established by this audit.

## Executive finding

The consolidated tree contains substantial reusable simulation, content, rendering, browser replication and construction implementations. The public release currently selects a static UI/local-authoring preview. A production native session/lot service, durable gameplay storage and an automatically supplied, permission-cleared starter-content cohort are missing.

Changing `mode` to `connected` and setting `native_lots: true` will not close these gaps. The existing native end-to-end host is explicitly test-only and connects two browser contexts to one controlled account and avatar. It runs real Rust simulation, but does not establish distinct-user multiplayer, production economy or recovery after host restart.

The smallest credible next milestone is: **open the URL without uploading game files; create or select an owned, rendered avatar; enter one shared lot as either of two distinct users; move and complete one real object interaction; observe each other's accepted changes; and preserve a small, authorized build change across a server restart.** Broader cities, objects and social features follow that milestone rather than being inferred from it.

**Scope is the intended playable game and the original FreeSO/source inventory, not completion of Swarms A–F.** Those work groups were an initial implementation decomposition. Section 10 inventories additional original systems and the coverage ledger required for the full specification. Passing the vertical slice, merging all existing branches, or completing the old task labels does not close that larger scope.

## Shipped, test-only and missing

“Shipped” here means present in the inspected source or static artifact, not necessarily deployed as an active service.

| Area | Shipped/reusable implementation | Test-only evidence or surface | Missing integration and priority |
|---|---|---|---|
| Browser entry | Startup mode validation; login/roster/city/lot screens; preview authoring and device-local saves | Delayed preview replies and illustrated Sims | **P0:** native session/directory/admission provider; production config must point to it |
| First-play content | Bounded original-format import, appearance composition, rendering, cooker/manifests | Synthetic avatar resources; external installed-game verification; tiny CC0 pack | **P0:** coherent permitted avatar/object/animation pack and automatic loading before Create Sim/Play |
| Native simulation | `GameRuntime`, authority/replica roles, accepted ticks, source interactions, snapshots | `native_browser_peer` fixture and declared BHAV cohorts | **P0:** persistent, authenticated native lot host and real lot lifecycle |
| Shared multiplayer | Per-principal interaction validation; browser checkpoint/tick replication | Two contexts controlling the same fixture avatar | **P0:** distinct accounts, owned actors, independent grants, joins/departures and mutual visibility |
| Persistence | Snapshot/restore formats; MySQL/MariaDB claim/fencing repository | RAM authority, balances and construction reservations | **P0:** durable checkpoints/tail, account/avatar/object ownership, operation journal and recovery |
| Buy/Build | Authoritative quote/consent validation; browser purchase/floor tools | Authored catalogue and generated RAM completion receipts | **P0 when advertised:** transactional economy/inventory provider; **P1:** remaining browser build tools |
| Movement | Real incremental routing in sim-core | Source routing regressions and fixture poses | **P1, required for playable milestone:** authenticated browser movement ingress and qualified gait/interaction routing |
| World/object content | World projection, meshes/material adapters, rendering and picking | Small synthetic geometry and isolated routine execution | **P1:** matched models, behaviours, tuning, routing slots/topology and animations for each enabled family |
| Chat/EOD/social | Legacy service panels; native EOD libraries and provider contracts | Controlled original-wire messages and isolated EOD handlers | **P1/P2:** native transport/provider/UI connection; durable effects for effectful plugins |
| Cloud hosting | Static frontend deployment configuration; native Tokio/Axum processes | Loopback proxy/process orchestration | **P0:** approved runtime/storage adapter, HTTPS/WSS routing and hosted acceptance |

## 1. First-play content is a release gate

`apps/web-shell/src/avatar_content.rs:55–68` initializes the resource bank and appearance choices to `None`, with original-file names as defaults. `:77–192` reads user-selected files; `:424–437` exposes file/folder selection and Apply. There is no automatic hosted starter-pack loader. `components/connected_player.rs:840–845` blocks creation when appearance choices are absent. This agrees with the separately reported live-browser observation that Create a Sim requests game files and cannot complete.

An ordinary new player must not need an installed proprietary game or manual resource upload. Keep local import as an optional advanced feature, not the signup prerequisite.

### What content is actually available

- `apps/web-shell/public/assets/ASSETS.md` records contributed **illustrated UI PNGs** under MPL-2.0, Nunito under SIL OFL 1.1 and Tabler icons under MIT. It explicitly distinguishes these illustrations from rigged avatars, runtime models and an imported catalogue.
- The public asset tree contains city PNG channels, an empty-lot XML and 20 pool OBJ pieces. It has no standalone skeleton/mesh/animation/appearance/binding/collection/outfit or cooked `.wlp` starter cohort. Pool origin/hash records alone do not establish a complete per-asset redistribution grant.
- `TSOClient/FSO.Content.TSO/Content/Avatar` contains addon resources, not the complete base character chain. `docs/design/action-first/content-integration.md:29–39` explicitly identifies the missing adult skeleton and complete base head/body collections. Successful original-avatar verification used external original files; those files are not supplied by that evidence.
- `apps/web-shell/public/assets/world/provenance.json` lists **eight unresolved placed object GUIDs** in the bundled lot; its audit found no matching OBJD/model mapping. The lot document is not a ready furnished gameplay world.
- `fixtures/packs/README.md` and `demo.json` explicitly provide a **CC0-1.0 authored pipeline fixture** containing BHAV, BCON and an unknown chunk. It is not a playable object or avatar pack.
- `apps/web-shell/examples/native_avatar_resources.rs:1–6` explicitly describes its generated resources as test-only, never a production avatar pack.

**No existing self-contained playable avatar + object + animation starter pack with established redistribution documentation was found.** The repository's source licence must not be treated as permission to publish missing original EA assets or every separately sourced addon.

### Smallest content delivery work

Author or obtain permission for a matched starter cohort: base rig; independently selectable head/body options and stable identities; textures; idle/walk/use animations; one usable object with mesh/materials, behaviour, interaction table and routing data; and a small lot. Record author/source/licence/redistribution per resource.

Reuse the existing cooker and trusted release hashes. `docs/swarm-b/cooker.md` documents `cook … --public`, which requires recorded redistribution permission and a licence. It verifies declared permissions, not their legal truth, and does not discover every dynamic gameplay dependency. Add automatic, bounded loading into `ContentUi`, bind it to the authority's content identity, and show explicit loading/error/retry states before enabling creation. Never replace missing dependencies with an unrelated avatar or silently substitute assets.

## 2. Native session and authority service are not implemented in the production router

The browser lifecycle is still:

1. `ConnectedUi::login` calls `/v1/sessions` and opens the gateway socket (`connected_bridge.rs:166–228`).
2. `ConnectedUi::play` sends `ConnectCity` for the selected roster avatar (`:753–764`).
3. `JoinLot` must produce `SessionState::LotReady` (`connected.rs:42–62`).
4. Only then does `NativeLot` request `/v1/native-lot/admit` and connect to `/v1/native-lot/stream` (`native_lot.rs:120–137`; `connected_bridge.rs:1179–1246`).

`services/browser-gateway/src/server.rs:59–66` exposes health, original-service login/session/roster/shards/query and `/v1/ws`. It has **no native admission, stream or construction routes**. Its account operations use an external original account API, and its city/lot connections use original Aries TCP. Its sessions are process-local (`server.rs:24–37,209–231`). It does not host a native account database or native simulation.

The missing native routes currently exist only in `tools/native-browser/peer.mjs:51–109,125–182`. That file is a loopback test proxy. `current()` requires avatar 42 and lot 16777472 (`:38–44`); its Rust subprocess refuses ordinary startup without `WONDERLAND_NATIVE_PEER_TEST_ONLY=1` (`crates/game-runtime/examples/native_browser_peer.rs:529–539`). Do not publish that host by removing its test-only guards.

A real host can preserve the existing browser DTOs and native wire while providing native accounts/session ownership, roster/directory results and lot admission. Alternatively, retaining actual original account/city services requires an operator deployment of those services plus a native lot host. These are different integration choices. `native_lots` alone does not select either backend.

## 3. Distinct-user multiplayer and lot ownership

The two-context check in `tools/native-browser/verify.mjs:139–146` compares the same test actor's needs. Native bootstrap binds every client to the same fixture principal/actor (`native_browser_peer.rs:350–375`). This establishes shared replication, not two independent players.

Reuse `live_wire/player/admission.rs::revalidate_action`, which checks principal/actor binding, generations, queue revision/sequence, observation age and fresh source offers. Supply those values from authenticated owned-avatar admission, never browser assertions. Add real actor spawn/join/departure, session revocation, separate permissions, one serialized lot executor and authority-epoch fencing. Preserve bounded input and backpressure limits and reject stale callbacks after leave/rejoin.

Acceptance needs two different principals and avatar IDs. Each must control only its own avatar; a visitor must not inherit an owner's build permissions; the other browser must observe accepted movement, object and build changes.

## 4. Persistence and economy require service providers

The core already provides `GameRuntime::{snapshot,restore}` (`crates/game-runtime/src/lib.rs:153–157`) and checkpoint/tail replication. A snapshot is state after completed tick N; replay starts at N+1 (`sim-core/src/snapshot.rs:1–7`). These are formats and algorithms, not a durable storage service.

`crates/persistence/src/lib.rs` is a separate MySQL/MariaDB ownership-claims implementation. `ClaimRepository::fenced` (`:206–228`) is useful for transactional fencing, but no runtime service currently consumes it. Its sole claims table is not account, inventory, economy or world storage, and it is not a Cloudflare SQLite adapter.

The native construction fixture stores reservations, balance, request IDs and pending completion in RAM (`native_browser_peer/construction.rs:10–19`). `grant` supplies fixture ownership, permissions and prices (`:36–56`). `complete` generates receipt/persistent IDs and updates the RAM balance (`:180–207`). Reconnect to this surviving process is not restart recovery.

Keep `ConstructionSession`'s existing quote → exact consent → pending → outcome/unknown protocol. Its contract explicitly leaves authentication, stable operation allocation and durable accounting to providers (`live_wire/construction/session.rs:1–5,87–90,196–198`). Add:

- Durable owned avatars/objects, balances and permissions/catalogue revisions.
- Stable operation ID + immutable request hash; duplicate identity with changed bytes must reject.
- Transactional debit/reservation/object allocation and durable effect receipt/outbox.
- Persisted accepted history/checkpoints, content identity, authority epoch and operation correlation before reporting durable completion.
- Restart reconciliation of pending/unknown operations without a second debit or replacement object.

A process failure after a commit but before its reply must recover the same operation. Do not retry an uncertain write as a new transaction or infer completion from a socket write.

## 5. Movement and the playable simulation loop

Native ground clicks currently do not walk. `native_lot.rs:437–447` turns picks into object action targets; `native_avatar.rs:486–505` rejects non-object picks. `PlayerAction` has only Invoke/Cancel (`live_wire/player.rs:51–54`).

There is useful implementation to extend, rather than rewrite:

- `AcceptedCommand::BeginRoute { entity, target, goals }` exists at `sim-core/src/runtime.rs:112–116`; `RouteGoal::point` is in `world/routing/mod.rs:74–83`.
- Routes use incremental pathfinding/collision/replanning, movement progress and `RuntimeEvent::RouteFinished` (`runtime.rs:1421–1537`). This is not teleportation.
- Standalone routing requires an idle actor with no VM frames/continuation (`:777–792`). The fixture's persistent wait thread does not meet that condition.
- `BeginRoute` is trusted authority input, not an authenticated browser request. A bounded movement-intent adapter must bind actor, lot/epoch/generation and sequence. Never expose arbitrary `AcceptedCommand` or accept browser-supplied route callback completion.
- For active/queued Sims, qualify source Goto/object interactions through TTAB → Invoke → BHAV routing. `runtime_routes.rs::handle_route/route_slot` exists, but complete content and callback providers are necessary for stand/sit/portal/shoo paths.

The actual lot host also needs a scheduler and explicit idle policy. The fixture runs a tick every 100 ms while clients are present (`peer.mjs:182`); sim-core's semantic rate is 30 ticks/second (`sim-core/src/lib.rs:24`). Define and verify the real-time policy instead of inheriting fixture timing accidentally.

## 6. Build, object-family and social coverage

The native browser already has GPU placement, catalogue search, rotation, exact-price consent and unknown-result recovery. Its exposed catalogue item kinds are Purchase and Floor (`apps/web-shell/src/native_build.rs:17–20,239–246`). Runtime construction also supports solid wall, terrain, move and remove (`game-runtime/src/live_wire/construction.rs:15–43`); those browser tools remain to be connected.

`GameRuntime::{from_imported,from_cooked,advance,offers,invoke,cancel}` (`lib.rs:74–145`) are reusable. The source-family report at `docs/swarm-b/runtime-source-family-results.json` explicitly marks its isolated chair/bed/appliance scenarios `complete_gameplay: false`. Missing global/semi routines, animation metadata, normalized routing slots and multipart topology are not cured by successful isolated instructions. Qualify each advertised object through the full play loop, including queue completion/cancel, pathing, effects and rendered animation.

`GameRuntime` currently has no `wonderland-eod-runtime` dependency. EOD libraries expose durable-provider interfaces but supply no production database/economy provider (`crates/eod-runtime/src/effects.rs:4–12,126–135`). Existing chat/EOD panels consume legacy gateway messages (`components/connected_player.rs:335–340,673–700`); they do not constitute a native chat/EOD stream. Add native lot chat if social play is part of the milestone, and leave unconnected families visibly unavailable. Do not enable effectful EODs before their private durable journal/provider boundary exists.

The browser city renderer currently selects fixed `city_0100` resources (`source_city_renderer.rs:43–58`). One explicitly supported city is credible; this is not yet a general multi-city content delivery system.

## 7. Cloudflare feasibility and constraints

The reviewed recommendation can preserve Rust simulation by running a native image in a Cloudflare Container, with a per-lot Durable Object coordinating routing, authority and durable metadata. Its specification is approved to be written; that does not establish full-specification approval, provisioning or implementation. The current source has no such adapter or container deployment.

Official documentation reviewed on 11 October 2026:

- [Durable Object Container API](https://developers.cloudflare.com/containers/api/durable-object-container/): a Durable Object manages/proxies a Linux-container workload and has its own storage/coordination APIs. This is a plausible host for the existing native Rust code; it is not the same execution environment as publishing browser WASM.
- [Container FAQ](https://developers.cloudflare.com/containers/faq/): container disk is ephemeral by default. Merely writing snapshots to the container filesystem does not make gameplay durable across restart.
- [Durable Object persistent storage](https://developers.cloudflare.com/durable-objects/best-practices/access-durable-objects-storage/): in-memory state may be lost on eviction/restart; durable state belongs in storage.
- [Durable Object limits](https://developers.cloudflare.com/durable-objects/platform/limits/): SQLite values/rows have a 2 MB limit; received WebSocket messages have a 32 MiB limit. Current native snapshots permit 64 MiB (`sim-core/src/snapshot.rs:33`), while bootstrap is bounded at 32 MiB (`game-runtime/src/live_wire/player.rs:20`). Design storage chunking or immutable blob storage and explicit end-to-end frame limits; do not silently truncate snapshots or assume every accepted native packet fits a terminating DO socket.
- [R2 consistency](https://developers.cloudflare.com/r2/reference/consistency/): direct API/binding writes have strong read-after-write consistency. If selected for larger immutable checkpoints/content, publish verified blobs before transactionally advancing their durable metadata pointer; this does not create a cross-store transaction automatically.
- [Workers TCP sockets](https://developers.cloudflare.com/workers/runtime-apis/tcp-sockets/): Workers provide an outbound TCP API. The existing Tokio/Axum/TCP process is not a drop-in static Worker deployment; a runtime/transport adapter or native container is needed.
- [Workers WebSockets](https://developers.cloudflare.com/workers/runtime-apis/websockets/): edge WebSocket handling is available, but application admission, replay protection, permissions, backpressure and state durability still belong to this implementation.

A same-origin `/gateway` route can preserve the browser's current URL model and avoid introducing unnecessary cross-origin plumbing. Exact account entitlements, runtime sizing, storage choice and costs still require configuration/approval; this audit does not establish them.

## 8. Recommended first milestone and acceptance gate

Implement these in dependency order, reusing current libraries:

1. **Permitted starter content + automatic bootstrap.** A fresh browser has rendered character choices without uploading files. Manifest/content identity matches the authority. Creation produces an owned persistent avatar; unavailable resources fail visibly.
2. **Native account/session/directory/admission host.** Preserve existing browser DTOs where sensible. Start acceptance with two distinct provisioned accounts, then verify the actual signup/CAS path; do not substitute two tabs of one fixture account.
3. **One serialized, fenced native lot.** Real clock, actor lifecycle, bootstrap/checkpoint/tail and exact action receipts over HTTPS/WSS. Reuse `GameRuntime` and `revalidate_action`.
4. **One movement/object play loop.** Collision-aware movement, idle/walk/use animation, one real interaction with a state/need effect, cancellation, and mutual observation by the second user. Explicitly select either the idle-avatar movement adapter or fully qualified active-Sim source routing.
5. **One durable build loop.** Owner purchases/places one permitted object and changes one floor; visitor mutation rejects. Persist state, funds, ownership and operation outcomes.
6. **Hosted recovery and abuse checks.** Disconnect, lose a receipt, kill/restart the authority and rejoin. Require identical recovered state, one debit/one object, stale principal/epoch rejection, no uncertain-write replay, and successful operation from the other client. Add native lot chat if it is part of the promised experience.

This establishes a genuinely playable vertical slice, not completion of every original object family, city, job, EOD, wardrobe or social service. Keep subsequent capability expansion explicit and backed by end-to-end journeys.

## 9. Existing executable and E2E entrypoints

These are reproducible entrypoints inspected in source, **not newly executed for this audit**. The current browser scripts start controlled peers themselves; they are not hosted-production acceptance runners.

```sh
# Actual production legacy-service bridge (requires operator original services):
cargo run -p wonderland-browser-gateway

# Existing controlled browser/runtime gate:
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
(cd apps/web-shell && trunk build --release --locked)
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install chromium
node tools/native-browser/verify.mjs
node tools/native-browser/verify-construction.mjs
node tools/native-browser/verify-checkpoint-tail.mjs
node tools/native-browser/verify-receipt-deadline.mjs
node tools/native-browser/verify-legacy-isolation.mjs

# Explicitly synthetic avatar preparation and browser observation:
cargo run -p wonderland-web-shell --example native_avatar_resources --locked -- target/native-avatar-fixtures
node tools/native-browser/verify-avatars.mjs
node tools/native-browser/verify-audio.mjs
node tools/native-browser/verify-pose-retention.mjs
node tools/native-browser/verify-pose-batches.mjs

# Existing public-cooking interface; output must be a new directory:
cargo run --manifest-path tools/asset-cooker/Cargo.toml -- cook fixtures/packs/demo.json /tmp/cooked-content --public
cargo run --manifest-path tools/asset-cooker/Cargo.toml -- verify /tmp/cooked-content
```

`.github/workflows/native-browser.yml` assembles the controlled journeys. Existing focused tests include `crates/game-runtime/tests/native_player_admission.rs`, `construction_session.rs`, `construction_exchange.rs` and checkpoint recovery. `crates/sim-core/tests/runtime_routes_source.rs::runtime_multilevel_portal_callback_snapshot_finishes_on_both_replicas` exercises genuine routing and recovery.

SQL claims verification is separate: `cargo test --manifest-path crates/persistence/Cargo.toml --locked -- --test-threads=1` requires an explicitly disposable `_claimstest` database and performs writes. Do not point it at gameplay data.

A new hosted acceptance runner must target the actual HTTPS/WSS service, use two independently owned avatars and test server restart/durability. Preserve the existing fixture gates as regression evidence rather than relabelling them as production multiplayer tests.

## 10. Original-system inventory beyond the first milestone and A–F

### Denominators and evidence boundaries

The inspected original tree contains **37 C# project files**, **23 API controller files**, **20 city-handler files**, **33 database-domain directories**, and **9 task-server C# files**. These are reproducible file/directory counts, not implemented feature counts or acceptance percentages. The full specification should trace all original projects, their runtime registrations, data domains and player/editor/admin surfaces. A project or handler name alone is not a completed functional requirement.

Existing source-derived ledgers provide useful starting denominators:

- `docs/swarm-a/COVERAGE.md` and `vm-source-notes.md` record **56 primitive registration statements**, resolving to 50 populated TSO and 53 TS1 slots. Their historical status table distinguishes implemented, host-adapter, partial, request-only and unsupported cases. Do not convert registry presence into gameplay completion; refresh each status against the final integrated implementation and execution evidence.
- `fixtures/eod/registration-census.json` records **30 server registrations, 28 UI registrations, 30 source-translated native handlers, zero fully original-runtime-verified handlers and zero production-provider-verified handlers**. `docs/swarm-b/eod-coverage.md` retains per-registration source, state-transition and recovery references. The two missing original UI registrations remain intentional source distinctions, not invented browser tasks.
- `docs/swarm-b/object-coverage.md`, `docs/compat/object-matrix.json` and `fixtures/objects/cohorts/index.json` inventory the pinned checked-in subset: **745 IFF/PIFF paths, 730 distinct source hashes, 1,327 decoded OBJD definitions plus 275 source-only cohorts, producing 1,602 leaves**. Each leaf's enter/use/cancel/leave/save/reconnect evidence starts explicitly unverified. This is not the full installed-game catalogue: the shipping installation/patch manifest denominator is unavailable.
- `docs/design/action-first/player-capability-map.md` inventories **21 original player surfaces** with original paths and remaining adapters. It is useful scope evidence, but some coverage text predates newer native animation/audio/build work; current code and exact receipts govern status. The 21 surfaces do not replace the server, editor, persistence, asset and operations inventories below.

### Full-game workstreams to retain in the specification

“Missing/unstarted integration” below means no complete native hosted implementation and acceptance was found in this review. It does not mean all lower-level code is absent, and it must not erase existing contributions.

| Original system and source anchors | Current evidence to preserve | Work still requiring specification, integration or acceptance |
|---|---|---|
| **Account lifecycle and trust:** `FSO.Server.Api.Core/Controllers/{RegistrationController,PasswordController,AuthLoginController,UserOAuthController}.cs`; database `DA/{Users,AuthTickets,EmailConfirmations,Bans}` | Original-service login bridge and browser login/session states | Native signup, recovery/verification policy, account persistence, abuse/rate controls, bans/moderation, session revocation and owned-avatar authorization; complete native service integration is missing |
| **Avatar lifecycle/progression:** `tso.client/UI/Screens/{PersonSelection,PersonSelectionEdit}.cs`; database `DA/{Avatars,AvatarClaims,Outfits,Relationships}` | CAS inputs, appearance readers/composer, accepted native avatar poses/needs, claim primitive | Automatic permitted resources, durable creation/retirement, owned wardrobe and accessories, skills/job progression, social state, custom/pet rig policy and independent multiplayer lifecycle |
| **City/shards and property discovery:** `FSO.Server/Servers/City/Domain/{LotAllocations,LotServerPicker,Neighborhoods}.cs`; API shard/city/lot controllers | Directory contracts, packed-location handling and one city renderer | Native city/session allocation, multi-city content, admission policy, real directory updates, online presence, map/facade delivery and scale/capacity policy |
| **Property ownership and management:** city handlers `PurchaseLotHandler.cs`, `ChangeRoommateHandler.cs`; database `DA/{Lots,LotAdmit,Roommates,LotClaims}` | Guarded source operations and selected-owner UI | Durable purchase/move offers, residents/donors, admit/ban/build rights, environment/resize controls and accepted permission changes; no native end-to-end ownership service is established |
| **Economy and inventory:** `Servers/Lot/Domain/LotServerGlobalLink.cs`; database `DA/{Transactions,Objects,Outfits,Bonus,DynPayouts}` | Build preview/consent/commit validation and EOD effect contracts | Atomic virtual-currency ledger, purchase/sell/store/place/donation/upgrade/refund semantics, exact persistent IDs, reservations, capacity/catalogue subscriptions, reconciliation and durable replay |
| **Jobs and payouts:** `Servers/City/Domain/JobMatchmaker.cs`, `Handlers/MatchmakerNotifyHandler.cs`; `Servers/Tasks/Domain/{JobBalanceTask,BonusTask}.cs`; `tso.simantics/Primitives/VMOnlineJobsCall.cs` | Source job-related fields, request boundaries and translated cooperative EOD families | Job matchmaking/lots, participants, skill/progression/payout state, job-specific UI and durable providers; no complete native hosted jobs loop is established |
| **Object-specific EODs:** `tso.simantics/NetPlay/EODs/Handlers`; `tso.client/UI/Panels/EODs` | All 30 registered server families have native translation according to the census; some gateway dresser/rack controls | Per-family actual game UI, VM opening/closing, participant/permission flow, provider wiring and original-runtime comparison. Include cooperative pizza/paper/maze, trade, wardrobe, signs/doors, music/nightclub/band/DJ, bulletin/newspaper, gameshow/scoreboard and casino/game families. Do not equate translated handlers with live playable objects |
| **Social and communication:** city handlers `MessagingHandler.cs`, `MailHandler.cs`, `FindPlayerHandler.cs`; database `DA/{Inbox,Bookmarks,Relationships}` | Original chat/private-message/mail protocols, panels and controlled receipt tests | Native lot chat, private delivery, durable inbox/read preferences, relationship/bookmark/ignore updates, live presence, moderation and user-facing delivery/error/reconnect behavior |
| **Neighborhoods, elections and bulletin:** API `{NhoodInfoController,ElectionInfoController,BulletinInfoController}.cs`; city `{NhoodHandler,BulletinHandler}.cs`; database `DA/{Neighborhoods,Elections,Bulletin}` | Read/query contracts, some original request flows and panels | Native eligibility, membership/rankings, mayor/rating/nomination/vote lifecycle, timed election tasks, permissions, persistence and complete player workflows |
| **Simulation engine and lifecycle:** `tso.simantics/{VM.cs,VMContext.cs,Engine,Primitives,Entities}` | Deterministic native VM, primitives, queues, clock/RNG, scoped original interpreter receipts | Every registered mode/case, actual init/main/reset/exit/deletion behavior, autonomy, async/effect completions, relationships, special character/object cases and approved differences; primitive census alone is insufficient |
| **Routing and world semantics:** `tso.simantics/Engine/Routing`, `Model/Routing`, `VMArchitecture.cs` | Native room/placement/route/build implementations and scoped original placement differential | Ground movement admission, active-Sim Goto, content callbacks, stairs/doors/chairs/portals/shoo, containment/multipart topology, actual footprints/SLOT data, collision/path animation and declared compatibility differences |
| **Full Build/Buy and property editing:** `tso.client/UI/Panels/{UIBuildMode,UIBuyMode,UIObjectHolder}.cs`; `UI/Panels/LotControls` | Purchase/floor native tools, source authoring codecs and some drafts; richer runtime Selection variants | Terrain/water/walls/wallpaper/stairs/fireplaces/plants/floors/doors/windows/roof/hand tools, brush/fill/drag selection, move/store/sell/upgrades, cost/refund and ownership UI; qualify every advertised tool against durable authority |
| **Rendering and environment:** `tso.world`; `tso.vitaboy.engine`; `FSOFacadeWorker`; original city rendering | Source-world GPU/software paths, picking, captures/facades, avatar composition/pose/terrain contact | Full content/model binding, material/light/shadow/weather/environment behavior, contained attachments, special rigs, performance/LOD, multi-city fidelity and physical-device/accessibility acceptance |
| **Audio/music:** `tso.sound`; `tso.simantics/Model/Sound`; original HIT/EVT/HSM/station resources | Decoders/runtime, Web Audio lifecycle and accepted native gameplay-audio fixture evidence | Automatically delivered permitted banks, original object/camera/station/music wiring, multi-actor semantics, missing codec/provider cases and complete end-to-end object sound qualification |
| **Assets and compatibility:** `tso.files`, `tso.content`, `FSO.Content.TSO`; installed patch/catalogue manifests | Bounded readers, content identity/cooker/creator tools and checked-in subset census | Rights-cleared distributable release, full dependency and patch/tuning ordering, installation/corpus denominator, playable family manifests, automatic browser delivery, upgrades/migrations and source coverage per resource |
| **Creator/editor/debug tools:** `FSO.IDE/{ResourceBrowser,ContentEditors,EditorComponent,AvatarUtils}`; `tso.debug`; JIT projects | `tools/creator`, `tools/creator-web`, resource editors/import/export/workbench and cooker | Inventory the original IDE and intended browser/native authoring workflow separately: BHAV graph/debug execution, resource editors, sprites/meshes/avatar/animation tools, tuning/upgrades, patch review, city/neighborhood authoring, undo/transactions and deploy-safe content publishing. Existing tool modules do not establish a complete editor product |
| **Networking and protocol migration:** `FSO.Server.Protocol/{Aries,Voltron,Electron,Gluon}`; `tso.simantics/NetPlay` | Legacy parsers/bridge and separately versioned native WLB/WLR/WLC/WLA protocols | Real native admissions/streams, shard/lot routing, upgrades/version negotiation, backpressure/replay policy, loss/reconnect behavior and migration/import plan. Original FSOv/VMNet cannot be treated as an existing native restore path |
| **Durable state and service models:** `FSO.Server.Database/DA` (33 domains); `FSO.Server.DataService`; `FSO.Common.DatabaseService`; `Servers/Lot/Domain/{LotHost,LotContainer}.cs` | Snapshot/restore contracts and isolated claims repository | Schemas/migrations for all retained domains, model subscriptions/updates, journal/outbox/checkpoint atomicity, host fencing, backups/restore, compaction, retention and crash/redeployment reconciliation |
| **Scheduled world operations:** `Servers/Tasks/TaskEngine.cs`; `Domain/{BirthdayGiftTask,BonusTask,JobBalanceTask,NeighborhoodsTask,PruneDatabaseTask,ShutdownTask}.cs` | Original implementations and source inventory | Native durable schedules, idempotent task execution, clock/time-zone policy, reward/election/prune/shutdown effects, retries and operational controls; no complete replacement task service was found |
| **Production operations and administration:** API `Controllers/Admin`; `FSO.Server/Admin`; `FSO.Server.Updater`; `FSO.Patcher`; server lifecycle/shutdown handlers | CI/build checks and the static deployment | Approved hosting/runtime/storage spec, production config/secrets boundaries, monitoring/audit, moderation/admin authorization, recovery/backups, incident handling, graceful shutdown, schema/content/client rollout, resource/cost limits and load/security acceptance |
| **Player settings/help/accessibility:** `tso.client/UI/Panels/UIOptions.cs` and graphics/display/chat controls | Reduced motion, camera controls, audio groups, sign-out and some help | Complete retained preferences, synchronization, input/accessibility/mobile/physical-device behavior, support/recovery affordances and original option semantics |

### How to close the full scope honestly

Create a source-derived ledger whose unit is a real player/server/editor/operations capability, registration or content leaf, rather than an old swarm ticket. Each row should retain:

1. Original source path/symbol or registration and baseline/hash; applicable TS1/TSO mode.
2. Intended product behavior and explicit approved changes or retirements. Original disabled controls, optional platform hosts and internal implementation details must be identified rather than blindly reproduced or silently dropped.
3. Current code/provider/UI/content/storage owners and exact dependency gaps, including **not started**, **implementation partial**, **adapter missing**, **fixture-only verified**, **hosted verified**, or **approved out of scope**.
4. Concrete enter/use/cancel/leave/save/reconnect and error/permission/restart scenarios where applicable, with distinct-user cases for shared systems.
5. Exact executing commit/artifact/config, observations, negative controls and source/hosted receipts; code presence and an authored acceptance plan are not execution evidence.

Expand the pinned checked-in content census with an explicitly identified, legally usable content baseline where necessary. Do not invent a full-installation denominator or declare the entire game covered by the 1,602 checked-in leaves. The first playable milestone is one dependency-closed subset of this ledger; the full intended game remains open until its own source-derived scope and acceptance conditions are agreed and satisfied.

## Evidence interpretation

The reviewed original-VM receipts at `evidence/pr58-original-245079` establish scoped bootstrap/interpreter/ground-placement observations with the corrected real MonoGame runtime. They preserve the x=73 overlap rejection and fault controls; they do not establish complete object gameplay or a hosted game. Likewise, the successful static Browser UI artifact at master `2e319718` is valid frontend evidence, not proof that absent backend providers are deployed.

**Release claim today:** interactive UI/local-authoring preview with reusable native gameplay components and controlled integrated runtime evidence.  
**Release claim after the proposed gate passes:** a specifically enumerated hosted playable multiplayer slice with automatic permitted content and durable state.  
**Not established:** complete original-game parity, universal content coverage, production-scale capacity or an independently verified production security posture.
