# Wonderland playable game completion design

Status: proposed detailed specification for owner review. The owner approved the Cloudflare Containers and Durable Objects direction on 11 October 2026. Implementation of the new host is not yet approved by this document.

## Outcome and scope

Deliver an expansive multiplayer life simulation in the browser, using the original FreeSO source as the behavioral inventory and the merged Rust work as the starting point. Swarms A–F are foundations, not the definition of completion. A static scene, a working menu, a forwarded packet, a test peer or a merged pull request is not a completed gameplay system.

Players must be able to join, create a usable character, enter a shared world, move, interact, maintain needs, build and furnish homes, own and exchange items, socialize and participate in the supported jobs and activities. Their progress survives reconnects, service replacement and releases. Ordinary play must not require uploading game files. Preserve the approved action-first interface, original project capabilities and shared runtime contracts; do not replace the game with a new disconnected demo.

The first connected milestone is deliberately smaller than the final game. Two independent browser sessions are the minimum multiplayer proof, not a player limit. Every later capability has its own integrated play journey and restart/recovery evidence.

## Verified starting point

The consolidated source is master `a62301e2629e4824dee8f025fd0af22732912863`, including PR58 and the small PR59 fallback-icon repair. PR58's merged source was `2e3197185a98092b26ad3e1453d33cfe17125a18`. The public Cloudflare demo initially serves the latter with explicit preview mode and no gateway. It is a usable UI/local-fixture demonstration, not hosted multiplayer.

Reusable components include deterministic simulation, imported-content conversion, `GameRuntime`, native accepted-tick/checkpoint transport, browser replica and presentation, original-protocol decoding, native construction intent/receipt handling, and reference/differential tooling. Their capabilities and limitations are documented in their source. Preserve these implementations and tests.

The production `services/browser-gateway` is an original account/Aries bridge. Its presence does not provide a native Rust city/lot authority. The native browser tests use `tools/native-browser/peer.mjs` and a controlled Rust authority example; their fixed actor, RAM budget and authored acknowledgments must not become production services by renaming them. The isolated persistence workspace currently proves MySQL ownership fencing, not an implemented Cloudflare save system.

The original reference source is pinned at `4c6b3e8f5835b228723caea3c9f683c62f244f73`; newer upstream changes require a deliberate reviewed rebase of the reference. The source census records 47 declared C# projects, 40 present project files, seven submodule-backed declarations and 1,977 explicit Compile paths. It also records unassessed SDK defaults, imports, registries and transitive source. These counts are intake evidence, not completed capabilities. The recovered work-package inventory has 81 records; reconcile these with the original source rather than assuming a historical package count is complete.

## Architecture

### Browser and edge

Keep the Rust/WASM browser application and its current native wire/projection contracts. A same-origin Cloudflare Worker serves the release, authenticates HTTP/WebSocket admission, enforces request limits and routes to the appropriate account or lot coordinator. Static assets and versioned cooked content are delivered through Cloudflare storage and caching. No production privilege enters the browser.

Expose explicit native session, directory and lot-admission providers. Native mode must not wait for an unrelated original Aries connection to reach LotReady. Preserve the original-service bridge as a separately configured compatibility path, with no silent fallback between native, legacy and preview modes.

### Runtime authority

Run the existing native Rust `GameRuntime` in Cloudflare Containers. One logical active authority owns a lot incarnation. A per-lot SQLite Durable Object coordinates its current epoch, journal, committed cursor, player admission, pending operations and durable effects. The container computes simulation transitions; the coordinator is the durable acceptance boundary. Container local disk and process RAM are caches, not saves.

All player intents arrive through authenticated admission. The host derives account, owned actor, lot and permissions from server state. The browser never chooses an authority epoch, ownership nonce, arbitrary `AcceptedCommand`, durable receipt or success callback.

Keep the existing 30 Hz deterministic simulation boundary. Measure whether bounded batches of committed ticks can meet browser responsiveness and storage cost targets. Do not change simulation time to hide a slow host. Before publication, persist the accepted batch, its predecessor cursor, commands and post-state hashes. Only then acknowledge/broadcast it. A failed or ambiguous durable write cannot become a successful client outcome.

On startup, load a content-bound checkpoint and replay its accepted journal tail. Verify every cursor and hash before accepting players. A replacement host advances the authority epoch. Stale callbacks, old host writes and old browser sessions are rejected. Cap checkpoint, batch, journal and catch-up sizes using the existing native decoder limits and measured storage limits; an over-limit case becomes explicit recovery, not unchecked allocation.

### Atomic acceptance and authority replacement

A proposed batch carries lot ID, authority epoch, host incarnation, batch ID, previous committed cursor/hash, the exact accepted commands, resulting cursor/hash and stable external-effect IDs. One SQLite transaction checks the active incarnation and predecessor, then records the journal chunks, command results, effect outbox rows and new committed cursor. A duplicate batch ID must have the same digest; conflicting reuse rejects. Effects are dispatched only from this committed outbox, never directly from speculative runtime callbacks.

The container retains a committed snapshot or clone while calculating a tentative batch. It exposes no tentative player projection. A definite rejection discards the tentative runtime; an ambiguous response queries the stored batch ID before retrying. An exact stored result becomes committed locally. An absent result can be recomputed from the last committed state; a conflicting result stops the host and requires recovery. Timeouts never authorize reapplying economic effects.

Replacement first transactionally fences the old host and records a new host incarnation/epoch in a recovering state. Player admission remains closed. Restore the last committed checkpoint and tail under their original runtime identity, then adopt the granted epoch through a narrow reviewed `GameRuntime` authority method backed by the existing `SimRuntime::adopt_epoch`. This wrapper is currently missing and must preserve integrated content, queues and pending stable effect IDs; never patch snapshot bytes or expose arbitrary mutable simulation access. Record a takeover marker containing the old committed cursor/hash, old committed epoch and new delivery/authority epoch; adopting the epoch changes the state hash and therefore starts a new explicitly linked checkpoint anchor. Preserve original effect namespaces/nonces and their committed epochs while redelivery uses the new delivery epoch. Persist the resulting replacement checkpoint/cursor, then transactionally mark this same incarnation ready. A crashed recovery retries its recorded epoch rather than inventing fresh authority on every request. Clients receive fresh session grants and a new correlated bootstrap; previous callbacks cannot satisfy it.

### Storage and transport limits

The existing core snapshot ceiling is larger than both the current player-bootstrap ceiling and a Durable Object WebSocket message. The host must therefore impose a coherent smaller admission profile, not inherit whichever module allows the most bytes. The first hosted profile caps a serialized checkpoint at 16 MiB and an entire WebSocket application message at 20 MiB, below the documented 32 MiB received-message ceiling. Reject oversize input before buffering. Qualify the actual protocol overhead and client bootstrap limit before enabling this profile; lower the host cap if measured overhead demands it.

Store large checkpoints as immutable, content-addressed R2 objects. Verify upload length/hash before atomically advancing the SQLite manifest pointer with the matching committed journal cursor. A crash before pointer publication leaves an unreferenced blob eligible for delayed garbage collection; a missing or mismatched referenced blob fails recovery. Keep SQL journal chunks at or below 1 MiB, below the documented 2 MB row/value ceiling. Never rely on a cross-store atomic transaction. Retain the prior complete checkpoint and journal until the replacement is verified. Each checkpoint manifest captures the committed tick and outbox acknowledgement watermark. Compact only journal entries covered by a verified checkpoint whose external outcomes are durably retained; unresolved effects and replay-deduplication records cannot be discarded. Keep the previous two verified checkpoints, and delete unreferenced uploads only after a delayed reachability scan confirms that no current/recovery manifest references them.

Predictable world/content limits must reject a state-expanding operation before its economic commit when the resulting checkpoint cannot fit. Profile widening or a new chunked wire protocol is a versioned change with its own migration and browser recovery proof, not an automatic fallback.

### Durable accounts and economy

Use account coordinators for authoritative account sessions, avatar ownership, inventory and game-currency operations. Use D1 for indexed directory/search projections and R2 for immutable content and checkpoint blobs where appropriate. Neither a stale directory row nor an R2 upload is spending authority.

Retain the familiar account/avatar flow. The native account provider must implement registration, secure password verification, revocable sessions and recovery without forwarding credentials to the original server. Passwords are never logged; password hashing cost, rate limits, session expiry and recovery are security review gates. The exact signup/recovery UX is reviewed before public account creation. Internal tests use isolated test accounts, never production credentials embedded in browser fixtures.

An account coordinator also owns a fenced avatar-location claim. Two sessions cannot concurrently take independent authoritative control of the same avatar. Additional observers and explicit control transfer are separate grants. Cross-lot travel is a recoverable handoff: mark transfer pending, obtain a durable detach/freeze receipt from the previous lot that revokes the old actor grant, carry the committed avatar state and unresolved logical operations, advance the avatar claim epoch, and admit the destination from that recorded transfer. A crash leaves a resumable transfer record; it never admits both lots or clones inventory. Source-lot Durable Object acceptance rejects subsequent old-grant commands after detach, including callbacks from a stale container. Unresolved economic operations retain their logical IDs across the handoff.

Each economic operation has a stable logical ID independent of host epoch. Purchases and construction use a durable reservation and an explicit state machine: requested, reserved, accepted by the lot, captured, or conclusively rejected/released. The lot records the build receipt and resulting state; account capture uses an idempotent durable outbox. Recovery queries the existing result. It must not blindly refund on timeout, debit twice, or treat one coordinator's write as an atomic transaction across all coordinators.

Construction specifically follows the existing runtime order: exact quoted consent and immutable request digest; account reservation; admitted `BeginBuild`; durable capture and persistent object-ID allocation; admitted `CompleteBuild`; then either committed geometry or `NeedsReconciliation`. “Admitted” is not “installed.” Bind the operation to account, owned actor, lot, source/content/catalog revisions, price, quantity, consent and allocated persistent IDs. Reusing an operation ID with any changed field rejects. Preserve `Unknown` after a lost reply and query this same operation. Refund, sale, transfer and inventory placement require similarly explicit request/result/compensation records before their UI is enabled; a purchase proof does not qualify those paths.

The existing `CompleteBuild` contract can return `NeedsReconciliation` after a durable receipt exists but source-object initialization fails and local geometry is rolled back. Persist that exact receipt and reconciliation state. Do not report the purchase as playable, silently retry initialization as a new purchase, refund merely because the client disconnected, or lose the charged receipt. A reconciler checks the committed runtime outcome and account operation, then either completes the same permitted operation or records an explicit idempotent compensating ledger entry with its own stable ID. It must prove that no matching committed item remains before compensation. The browser retains the existing reconciliation state until the authoritative result is known.

Start with game currency only. This plan does not introduce cryptocurrency, cash payments, wagering or real-money item trading. Any later financial feature requires its own economic and authorization design.

## Playable content and first play

The shipped illustrated portraits and room PNGs remain useful UI assets, but are not a rigged playable avatar pack. The current CC0 fixture pack exercises resource formats and is not a complete starter world. Generated native avatar probes are explicitly test-only. A checked-in original resource is not automatically licensed for public redistribution.

Create a coherent, provenance-recorded starter release: compatible avatar skeleton and meshes, skin/clothing variants, locomotion and interaction animations, collision and routing slots, terrain and room geometry, furniture, catalog identity, prices, behavior resources, tuning, sound and icons. Preserve the approved art direction and existing character identity where applicable. Add content in a form consumed by the real cooker/runtime, not a parallel toy scene.

Use the existing public cooker gate with per-resource license/redistribution permission, dependency closure and source hashes. Serve an immutable manifest and automatically fetch the selected release during first play. Content identity is bound to sessions, checkpoints and builds. Corrupt, missing or incompatible packs produce a recoverable visible error; they cannot switch a live user to fixtures. Optional private imports remain separate from the ordinary signup path.

A full object is qualified by its complete behavior/dependency closure and actual interaction outcomes. Passing one BHAV excerpt does not make every chair, bed, stove or job object playable.

## Capability completion inventory

Maintain a machine-readable coverage ledger derived from active original source registries, screens, commands, content cohorts, schemas and services. Each entry records original source pin, destination, implementation status, connected provider, representative browser journey, failure/recovery evidence and any explicit product disposition. Never collapse an unassessed capability into done or silently remove it.

1. Simulation: scheduling, threads, primitives, routing, collision, motives, autonomy, relationships, time, effects, save/restore and deterministic replay. Resolve unsupported original operations through source-backed implementation or an explicit reviewed disposition.
2. Content: resource formats, patch/semiglobal/global resolution, tuning, object definitions, menus, animation/audio dependencies, catalog, runtime loading, cache integrity and public/private distribution.
3. World and avatars: live geometry, terrain, walls/floors/roofs/pools, lighting, occlusion, selection, avatar rig/appearance/clothing, locomotion, contact animation, camera, audio and renderer/device recovery.
4. Player experience: complete onboarding, character creation/editing, city and neighborhood discovery, lot visits, needs and action queue, build/buy, inventory, permissions, settings, keyboard/touch/accessibility and error/reconnect states.
5. Online services: native account/session identity, avatar ownership, directory, shared lot admission, simultaneous players, chat/presence, mail, relationships, bookmarks/ignore, neighborhoods, elections/bulletins where retained, jobs and service-side effects.
6. Durable progression: balances, skills and needs policy, property and roommate rights, inventory/portable state, purchases/sales/transfers, inventory placement, build receipts, migrations, recovery, backup and restore.
7. Activities and EOD: enumerate every original registered handler, job and game-specific interface. Connect its actual server effects, permissions, outcomes and persistence. Preserve unavailable states until integrated acceptance exists.
8. Creator and operations: resource editing/import/export, undo/redo, cooked releases, diagnostics, compatibility matrices, moderation and abuse controls, deployment/rollback, observability, quotas and support recovery.

The inventory is not closed merely by this list. Complete SDK/default-file and active-registry discovery, then reconcile additional original capabilities into it. No percentage-complete claim is justified until the denominator and acceptance evidence are established.

### Public and private activity state

EOD hands, decks, hidden choices, RNG state and private checkpoints remain in their authoritative private provider state. They must not enter shared lot snapshots, public tick payloads, browser caches, shared state hashes or logs merely to make replay convenient. Persist and recover them with versioned private records and stable logical effects; publish only the permitted recipient projection and public outcomes. Qualify each retained handler's information boundary as well as its gameplay result.

The expanded inventory explicitly inherits every row of `docs/design/action-first/player-capability-map.md` (21 current rows), every active original screen/primitive/EOD registry, and the preserved source dispositions. Any exclusion or changed product behavior requires owner approval and a recorded reason. A rename, consolidation or replacement is not permission to omit a capability.

## First integrated milestone

Two distinct authenticated test accounts select different owned avatars and enter the same authority-owned lot. Both clients receive the same admitted content and committed cursor. Each sees the other's real movement and action state. An eligible object exposes a genuine menu and completes a real runtime action, with needs/queue outcomes projected from authority.

Ground movement must be an authenticated intent routed through real collision/routing behavior. `BeginRoute` already exists for eligible idle actors; active VM interaction routing needs its actual source Goto/menu contract. Do not implement teleportation or fabricate route completion. Bind target, actor generation and world revision, then revalidate at authority acceptance.

A player purchases and places an eligible object, moves it and saves. The other client sees accepted geometry. Kill the authority process after acceptance, reconnect both clients, and verify ownership, balance, inventory, geometry, motives and logical operation results against durable records. Repeat interruption before acceptance and after reservation to prove no duplicate debit, free item or lost committed change.

A third session and configurable larger cohorts then establish that admission is general multiplayer. Determine and publish measured per-lot capacity; queue or reject excess admission explicitly. Do not imply unlimited player counts or qualify hardware from software-rendered browser tests.

## Completion evidence and release policy

Use real compiled frontend journeys and real services as the primary acceptance evidence. Preserve existing unit suites as regression checks, but do not substitute new unit-test counts for playability. Fixture protocol tests remain useful and labelled; they cannot satisfy live account, authority or persistence gates.

For each capability, exercise successful play, denied ownership, concurrent users, stale generations, replayed intents, network loss, tab suspension, reload, slow content, corrupted content, host replacement and release mismatch where relevant. Record exact source/content/image/Worker versions, command and receipt IDs, before/after durable state and browser traces/screenshots. Avoid secret-bearing logs.

Performance evidence must include cold asset load, input-to-visible accepted response, tick backlog, memory, checkpoint/recovery time, bandwidth and actual device rendering. Set supported cohorts from measurements, then enforce their limits. Include inactivity shutdown and bounded active Containers so the demo cannot create unbounded cost.

Promote coherent browser/host/protocol/content versions together through reviewed PRs and Cloudflare deployment receipts. Retain rollback source and reversible schema migration where possible. Do not roll back an irreversible schema or restore an old snapshot over newer committed money/items. Test a restore in an isolated environment before relying on it.

## Delivery sequence

- Complete original-source capability census and maintain the gap ledger throughout development.
- Implement the native host, authenticated session/admission and durable acceptance boundary using the existing contracts.
- Build and auto-load the qualified starter pack; connect actual movement, interaction and presentation.
- Qualify shared-lot play and restart-safe build/economy; deploy an explicitly scoped connected preview.
- Expand original simulation/object/EOD/job/social/city systems in independently reviewable integrations, each with an actual play journey.
- Complete content breadth, visual/mobile/performance acceptance, moderation, operations, migration and production recovery.

### Idle time and movement protocol

For the first connected cohort, an empty lot drains or durably records pending effects, checkpoints and pauses simulation ticks before its container sleeps. Re-entry resumes that exact state without inventing offline ticks or motive changes. This is an explicit early-preview policy. Original jobs, timers and offline progression must be inventoried and given their source-backed clock/deadline providers before claiming their completion; no handler may silently fast-forward RNG or expire an uncertain economic operation. Keep a lot awake while a qualified active activity needs live simulation, within its operator budget.

Preserve WLB1/WLC1/WLA1/WLR1 byte meanings. Introduce movement as a separately versioned authenticated native intent endpoint, advertising it only when enabled. It accepts a logical operation ID, target grid position/level and observed world revision; all 64-bit identities remain exact decimal strings or native binary values, never JavaScript Numbers. Account/actor/authority identity is server-derived. The authority maps eligible intents to the existing routing operation. Active Sims use qualified source Goto/menu routing, and unsupported busy states reject visibly. Cap ingress and pending movement per actor; cancellation/replacement is itself an admitted operation. Do not add a hidden new variant to WLA1.

### Required failure windows

Hosted acceptance must deliberately interrupt before and after coordinator commit, lose a successful commit reply, replace a coordinator while its old container remains alive, lose a capture receipt, and cause source initialization to fail after a genuine durable receipt. Also exercise changed-body operation replay, same-avatar double admission, interrupted cross-lot travel, corrupt/oversized checkpoints, orphan uploads and interrupted manifest publication. Verify new signup, independent appearance choices and reload, visitor permission denial, a floor edit, genuine cancellation and return to the city. Record exactly which boundary was interrupted.

Preserve device-local preview saves and original reference source. Seed scope with every existing player-capability row, including independent head/body/skin/gender choices, owned outfits, property purchase/move/resize, roommate/admit/ban rights, architecture tools, object donation/upgrades/sale pricing, private mail/messages, neighborhoods/rankings/elections and settings/accessibility. These remain requirements even when absent from the first milestone.

The first playable lot is an early delivery, not exhaustive completion. Work continues against the full source-backed ledger until every retained capability has implementation and integrated evidence. Architectural changes, rights questions, new persistent grants and external spending are surfaced specifically rather than hidden inside a generic deployment approval.

## Source references

- Repository: `crates/game-runtime/README.md`, `services/browser-gateway/README.md`, `docs/design/action-first/native-live-wire.md`, `native-browser-admission.md`, `native-construction-contract.md`, `docs/contracts/boundary-map.json`, `docs/compat/dispositions.md`, and `docs/swarm-e/OWNERSHIP_CLAIMS.md`.
- [Cloudflare Containers architecture](https://developers.cloudflare.com/containers/platform-details/architecture/) and [FAQ](https://developers.cloudflare.com/containers/faq/): native runtime integration and ephemeral local disk.
- [Durable Object storage](https://developers.cloudflare.com/durable-objects/best-practices/access-durable-objects-storage/) and [limits](https://developers.cloudflare.com/durable-objects/platform/limits/): durable state and bounded storage/transport.
- [R2 consistency](https://developers.cloudflare.com/r2/reference/consistency/): immutable object readback and metadata publication design.

Platform figures are design inputs observed on 11 October 2026 and must be rechecked during hosted qualification. They are not measurements of Wonderland's workload.
