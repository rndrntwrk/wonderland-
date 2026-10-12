# Verified FreeSO lineage and full-game scope audit

Date: 11 October 2026. Source inspection only; no original C# gameplay execution is claimed by this report.

## Lineage and preservation

GitHub metadata identifies both parent and source of `rndrntwrk/wonderland-` as `riperiperi/FreeSO`, a reimplementation of The Sims Online. A fresh fetch of pinned source `4c6b3e8f5835b228723caea3c9f683c62f244f73` and Git tree comparison confirms that Wonderland planning commit `2474596dd4c8e3420276a5994eea630e1279c226` retains the `TSOClient`, `Other` and `README.md` paths unchanged. Receipt: `FREESO-LINEAGE-20261011.json`.

The owner's deliverable is the full Rust/browser rewrite, including playable 3D, multiplayer, economy, creator tooling and refined UI. A–F, the static demo and the first connected lot are intermediate work. Generic “Sims-style” positioning cannot define a smaller completion scope.

The earlier spec inherited all original capabilities, but its generic categories did not make several important original obligations concrete. The expanded requirements below and `../compat/full-game-source-obligations.json` correct that weakness. In particular, preserve original nonredeemable game-currency casino mechanics; a blanket prohibition on “wagering” would have accidentally excluded original gameplay. New real-money finance remains a separate, unapproved product change.

## Concrete completion obligations

### 3d-mesh-pipeline

Sprite/depth reconstruction, embedded FSOM, replacement meshes/textures, cache precedence and invalidation. Implementation sequence task 5.

Sources: `TSOClient/tso.files/RC/DGRP3DMesh.cs`, `TSOClient/tso.content/RCMeshProvider.cs`.

Acceptance: Load an original object family in 3D, verify replacement precedence and reconstructed fallback, rotate and pick it after cache reload.

### graphics-camera-modes

Full2D, Hybrid2D, Full3D and perspective/first-person camera transitions. Implementation sequence task 5.

Sources: `TSOClient/tso.world/Model/GraphicsModes.cs`, `TSOClient/tso.world/GraphicsModeControl.cs`, `TSOClient/tso.world/Utils/Camera/CameraControllers.cs`.

Acceptance: Switch supported modes while interacting, preserving camera/picks and actual world state.

### city-lot-continuity

Neighboring subworlds/facades, city roads/foliage, neighborhood geometry and thumbnails. Implementation sequence task 5.

Sources: `TSOClient/tso.world/Components/SubWorldComponent.cs`, `TSOClient/tso.world/Facade/LotFacadeGenerator.cs`, `TSOClient/tso.client/Rendering/City/CityFoliage.cs`, `TSOClient/tso.client/Rendering/City/CityNeighGeom.cs`.

Acceptance: Discover, inspect and enter a property with correct facade/thumbnail/neighbors; edit and reverify its representation.

### environment-rendering

Room lighting/shadows, sky, particles and source-driven weather. Implementation sequence task 5.

Sources: `TSOClient/tso.world/LMap/LMapBatch.cs`, `TSOClient/tso.world/LMap/GPURoomMaps.cs`, `TSOClient/tso.world/LMap/ShadowGeometry.cs`, `TSOClient/tso.world/Model/WeatherController.cs`.

Acceptance: Change room structure and time/weather inputs; inspect lighting, particles, visibility and context recovery.

### job-worlds

Instanced career/grade/role-matched robot-factory, restaurant and nightclub shifts. Implementation sequence task 8.

Sources: `TSOClient/FSO.Server/Servers/City/Domain/JobMatchmaker.cs`, `TSOClient/tso.simantics/Primitives/VMOnlineJobsCall.cs`, `TSOClient/tso.simantics/Model/TSOPlatform/VMTSOJobInfo.cs`.

Acceptance: Match eligible players, respect ignore lists/roles, complete rounds, persist progression and return after disconnect.

### earning-dynamic-payouts

Single-object earning cohorts, economic history and dynamic payout tuning. Implementation sequence task 8.

Sources: `TSOClient/FSO.Server/Servers/Tasks/Domain/JobBalanceTask.cs`, `TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODFNewspaperPlugin.cs`.

Acceptance: Earn via supported object chains, apply an idempotent tuning period and verify authority-derived newspaper/economic projection.

### visitor-bonuses-rankings

Durable visits, Top 100 and category-scaled visitor/property bonuses. Implementation sequence task 8.

Sources: `TSOClient/FSO.Server/Servers/Tasks/Domain/BonusTask.cs`.

Acceptance: Visit across a period boundary, calculate once, rerun after failure and verify ranking and bonus without double payment.

### player-shops

Rack stock/try/buy/defaults/pricing, player object sales, business discounts. Implementation sequence task 6.

Sources: `TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODRackOwnerPlugin.cs`, `TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODRackPlugin.cs`, `TSOClient/FSO.Server/Servers/Lot/Domain/LotServerGlobalLink.cs`, `TSOClient/tso.simantics/Model/VMBuildableAreaInfo.cs`.

Acceptance: One user stocks/prices a shop, another buys an owned item/outfit; recover lost receipt with correct two-party ownership and balance.

### secure-property-trade

Bilateral money/items/property trade including optional furnishings and ownership side effects. Implementation sequence task 6.

Sources: `TSOClient/FSO.Server/Servers/Lot/Domain/LotServerGlobalLink.cs`, `TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODSecureTradePlugin.cs`.

Acceptance: Exchange a furnished property through mutual consent, interrupt settlement, verify all-or-nothing durable ownership and reject community-lot trade.

### object-lifecycle-upgrades

Wear, breakage, repair/skill effects, harvest components and upgrades. Implementation sequence task 7.

Sources: `TSOClient/tso.simantics/Model/TSOPlatform/VMTSOObjectState.cs`, `TSOClient/tso.simantics/Primitives/VMGenericTSOCall.cs`, `TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetUpgradeCmd.cs`, `TSOClient/tso.content/Upgrades/ObjectUpgradeProvider.cs`.

Acceptance: Use until worn/broken, repair and upgrade, verify costs/tuning/reinitialization/value and persistence.

### property-growth-capacity

Size/floor upgrades, roommate costs, object capacity/disable rules and discounts. Implementation sequence task 6.

Sources: `TSOClient/tso.simantics/Model/VMBuildableAreaInfo.cs`.

Acceptance: Expand a lot and floors, verify capacity and costs with roommate changes; reject invalid over-budget edits.

### community-property

Distinct community donations/build rights/sale rules and mayor property actions. Implementation sequence task 9.

Sources: `TSOClient/tso.simantics/Model/Platform/VMFSOCommunityValidator.cs`, `TSOClient/FSO.Server/Servers/City/Handlers/PurchaseLotHandler.cs`.

Acceptance: Operate a community lot with builder/donor/visitor roles; verify donation value/wear treatment and denied private-sale paths.

### skills-social-multipliers

Six skills/locks, lot skill modes, welcome boosts and directional relationships. Implementation sequence task 7.

Sources: `TSOClient/tso.simantics/Entities/VMAvatar.cs`, `TSOClient/tso.simantics/Primitives/VMRelationship.cs`, `TSOClient/tso.client/UI/Panels/UILotSkillModeDialog.cs`.

Acceptance: Train and lock skills, interact in different lot modes, reload and verify asymmetric relationships/economic multipliers.

### avatar-interaction-fidelity

Multi-actor acceptance/timeouts, held/contained objects, slots/reach, suits/accessories, headlines and pet rigs. Implementation sequence task 7.

Sources: `TSOClient/tso.simantics/Primitives/VMGrab.cs`, `TSOClient/tso.simantics/Primitives/VMDropOnto.cs`, `TSOClient/tso.simantics/Primitives/VMReach.cs`, `TSOClient/tso.simantics/Engine/VMSlotParser.cs`, `TSOClient/tso.simantics/Engine/VMSuitProvider.cs`.

Acceptance: Complete and decline multi-actor interactions; hold/place objects and change outfit/rig while observing contact, picks and recovery.

### calendar-events-cooldowns

Timed tuning/events, participation dedup, configured age gifts, game token attributes and global cooldowns. Implementation sequence task 8.

Sources: `TSOClient/FSO.Server/Servers/City/Domain/EventSystem.cs`, `TSOClient/FSO.Server/Servers/Tasks/Domain/BirthdayGiftTask.cs`, `TSOClient/FSO.Server/Servers/Lot/Domain/LotServerGlobalLink.cs`.

Acceptance: Run a configured event/age boundary, retry after replacement and verify one reward, correct eligibility and durable cooldown.

### election-lifecycle

Nominations/eligibility/voting/failsafe/winner/mayor replacement and announcements. Implementation sequence task 9.

Sources: `TSOClient/FSO.Server/Servers/City/Domain/Neighborhoods.cs`, `TSOClient/FSO.Server/Servers/Tasks/Domain/NeighborhoodsTask.cs`.

Acceptance: Complete an election period with eligible accounts; recover scheduled boundary and verify one authoritative outcome and recipient notices.

### all-eod-registrations

30 server registrations reconciled to client and server-only controllers. Implementation sequence task 8.

Sources: `TSOClient/tso.simantics/NetPlay/EODs/VMEODServer.cs`, `TSOClient/tso.client/UI/Panels/EODs/UIEODController.cs`.

Acceptance: Qualify each registered handler separately with participants, actual outcomes, cancellation, private-state boundaries and recovery.

### game-currency-casino

Original nonredeemable Simoleon casino games, owner bankroll management, bets and payouts. Implementation sequence task 8.

Sources: `TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODSlotsPlugin.cs`, `TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODRoulettePlugin.cs`, `TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODBlackjackPlugin.cs`, `TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODHoldEmCasinoPlugin.cs`.

Acceptance: Play each supported game with isolated game-currency accounts, recover mid-hand/reply loss and verify hidden data and balances without real-world value.

### volcanic-live-authoring

Object/GUID creation, BHAV graphs, tracing/stack inspection, resource-specific editors, PIFF/mesh/avatar work and undo/redo. Implementation sequence task 10.

Sources: `TSOClient/FSO.IDE/BHAVEditor.cs`, `TSOClient/FSO.IDE/ObjectWindow.cs`, `TSOClient/FSO.IDE/Managers/BHAVEditManager.cs`, `TSOClient/FSO.IDE/ResourceBrowser/PIFFEditor.cs`, `TSOClient/FSO.IDE/ResourceBrowser/FSOMEditor.cs`.

Acceptance: Create/edit an object, step live behavior and inspect stack, undo/redo, cook/export/reload and retain exact behavior/dependencies.

### object-family-breadth

Full deployed family/dependency denominator across household, pets, commercial, jobs and activities. Implementation sequence task 7.

Sources: `TSOClient/tso.content/WorldObjectProvider.cs`, `TSOClient/tso.content/RCMeshProvider.cs`, `TSOClient/tso.content/Upgrades/ObjectUpgradeProvider.cs`.

Acceptance: Qualify complete family interactions and patch/global/tuning/mesh variants; record missing distribution rights/resources as unresolved gates.

## Inherited limitations and conditions

- Job type 3 is explicitly unavailable in VMOnlineJobsCall; five state/role slots do not prove five implemented careers.
- House Statistics/Bills/Log are disabled in UIHouseMode.cs.
- Unknown EOD IDs use VMEODStubPlugin and disconnect. There are 30 explicit server registrations; not every controller needs a separate visible client panel.
- EA-Land PayPal/custom-commerce enum names do not establish implemented cash-in/out or real-money commerce handlers.
- First-person direct avatar control has an April Fools tuning gate; distinguish it from ordinary camera support.
- Weather includes real rain/snow/manual behavior and conditional/event logic; it is not evidence of a complete server climatology system.
- Configured birthday/event content is not necessarily enabled or available in every deployment.
- The upstream README distinguishes limited TS1 compatibility from the separate, incomplete Simitone frontend.

## What this audit does not prove

Source presence does not establish active configuration, redistribution rights, deployed behavior or Rust/browser parity. The full census must still reconcile SDK/default files, submodules, primitive modes, active registries and content families. The twenty obligations are concrete additions to that ledger, not its final denominator. Every requirement needs integrated browser and durable-service evidence. Unsupported upstream modes stay documented for explicit disposition; they are never silently reclassified as completed Wonderland features.
