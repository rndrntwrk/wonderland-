# Player capability preservation map

A redesigned capability is complete when its controls, data, permissions, rendering and accepted/rejected behavior work together. This map tracks the existing primary player interfaces; a visible button or local fixture alone is not game parity.

| Existing surface | Source reference under TSOClient/tso.client | Redesigned interaction | Required integration |
| --- | --- | --- | --- |
| Login, progress, city/shard choice and reconnect | UI/Screens/LoginScreen.cs; UI/Panels/UICitySelector.cs | Account entry and visual city choice, with progress in context | Authentication, shards, account capabilities and recovery |
| Character roster, create, retire, city/home entry | UI/Screens/PersonSelection.cs | Character grid, selected Sim stage, Play/Go home/Create and confirmed retirement | Account character IDs, actual capacity, authority and results |
| Independent head/body, gender, skin, name and description | UI/Screens/PersonSelectionEdit.cs; Controllers/PersonSelectionEditController.cs | Head/body thumbnail grids and compact choices around the character | Original collections and independent outfit keys, field policy and creation request |
| Rotating avatar preview | PersonSelectionEdit.cs creates UISim | Drag the character to rotate; reset view | Adult rig, appearance composition, actual meshes and texture pixels |
| Owned Day/Sleep/Swim outfits and decorations | UI/Panels/EODs/UIDresserEOD.cs | Owned-outfit grids with Wear, Set default and confirmed Delete | Distinct owned/content IDs, categories, ownership and action rights |
| City map, property/people/neighborhood discovery, filters and Top 100 | UI/Screens/CoreGameScreen.cs; UI/Panels/UIGizmo.cs | Select map locations and contextual search results | City/search/ranking projections |
| Property details, purchase and entry | UI/Panels/UILotPage.cs; UI/Panels/UILotPurchaseDialog.cs | Select property, inspect details and choose available action | Property rights, terms/price and accepted results |
| Lot live view, person/object actions, queue and needs | UI/Panels/UILotControl.cs; UI/Panels/UILiveMode.cs | Tap the person/object; act beside it; inspect queue and needs | Generation-checked entities, action offers, queue/need updates |
| Camera, floors and Walls Down/Cutaway/Up/Roof | UI/Panels/UIUCP.cs | Controls at the scene edge | Actual renderer camera/picking, levels and view modes |
| Full Buy catalog, categories, search and pages | UI/Panels/UIBuyMode.cs; UI/Controls/Catalog/UICatalog.cs | Visual content palette with supplied categories | Catalog IDs, thumbnails, prices, tags and availability |
| Inventory, move/store/sell, sale price, donation and upgrades | UI/Panels/UIObjectHolder.cs; UI/Panels/Upgrades/UIUpgradeList.cs | Object-attached actions and placement preview | Owned IDs, pose/level, ownership, quote and authoritative results |
| Terrain, Water, Wall, Wallpaper, Stair, Fireplace, Plant, Floor, Door, Window, Roof, Hand | UI/Panels/UIBuildMode.cs | Tool rail and contextual palette around the lot | Original build capabilities, permissions and runtime execution |
| Wall line/rectangle/delete; wall/floor painting and fill | UI/Panels/LotControls/UIWallPlacer.cs; UIWallPainter.cs; UIFloorPainter.cs | Drag in the scene; explicit draw/rectangle/erase/fill choices | Canonical picking, edit preview, cost/refund and commit/rejection |
| Terrain raise/flatten/grass and roof style/pitch | UI/Panels/LotControls/UITerrainRaiser.cs; UITerrainFlatten.cs; UIGrassPaint.cs; UIRoofer.cs | Direct brushes and contextual adjustment | Terrain/roof state, permissions, preview and accepted edits |
| Property information, residents/donors, rights, environment and resizing | UI/Panels/UIHouseMode.cs; UIEnvPanel.cs | Property panel and direct resident/permission actions | Property roles, management state and explicit change results |
| Player profile, description, skills/jobs and social options | UI/Panels/UIPersonPage.cs; Controllers/Panels/PersonPageController.cs | Select a Sim to open their profile and actions | Player data, relationships/privacy and action rights |
| Relationships and bookmarks | Controllers/CoreGameScreenController.cs | People and bookmarks overlays | Stable player/property references and updates |
| Lot chat, private messages, inbox/mail | UI/Panels/UILotControl.cs; UIInbox.cs; UIMessageWindow.cs | Lot chat, conversations and inbox overlays | Authenticated delivery, unread state, rejection and reconnect |
| Neighborhood profiles, rankings, mayor/rating, bulletins and elections | UI/Panels/Neighborhoods/UINeighPage.cs | Neighborhood map panel with people, places and bulletin actions | Neighborhood data, permissions and transactional results |
| Object-specific dresser, racks, trade, jobs, games, music, signs/doors and other EODs | UI/Panels/EODs/UIEODController.cs | Context-specific panels opened from the object | Typed session/messages, permissions and lifecycle |
| Graphics/display/camera, audio, chat/filter preferences, switch Sim, help and exit | UI/Panels/UIOptions.cs; UIGraphicsOptionsDialog.cs; UIUCP.cs | In-game options grouped around their effect on play | Renderer/audio/preferences adapters and supported values |

## Source qualifications

The original server enforces three account avatars in `TSOClient/FSO.Server.Database/DatabaseScripts/changes/0001_3_max_avatars.sql`. A different policy must come explicitly from the chosen service. It must not invalidate existing preview saves.

The original creator submits head outfit, body outfit, gender, skin tone, name and description independently. Original registration has its own name/description policy; preserving old Unicode preview names does not change the backend's policy.

The original wardrobe uses owned outfit records, Day/Sleep/Swim and head/back/shoes/tail decorations. Three whole-character illustration styles are not equivalent.

Verified functional Buy categories include Seating, Surfaces, Appliances, Electronics, Skill, Decorative, Misc, Lighting and Pets. Room-sort controls are declared but working behavior was not established by this inspection. House Statistics/Bills/Log are explicitly disabled in the inspected original source. These must not be invented as working legacy features.

Lot geometry, stories and object capacity come from the world/runtime. Some Build categories place objects; others submit architecture commands. They cannot all be represented by a furniture Move request.

## Content and runtime status

The existing browser preview has local roster, map, cafe and Home journeys. The correction removes their fixture-specific rules from the game-facing model and restores source-driven controls. The remaining authentication, property, social, messaging, neighborhood, EOD and complete simulation adapters are integration work, not deleted scope.

Swarm C's portable appearance composition and skinning libraries are available at commit `ca79bbd251491277d9fd5838d3f84d247675709a`. Swarm B's portable resource decoders are available at `04f0a407dd81acf0685453e7367764bf75b5c092`. Their fixture engine hosts are not a finished player renderer.

The repository includes addon avatar resources but lacks the original adult skeleton, base normal head/body outfit collections and their base payloads. These must be supplied as game content. Diagnostic meshes or historical artwork are not evidence that the original textured avatar catalog works.

Keep this map until each integration has direct control/request/renderer evidence, including a meaningful unavailable or rejected path. The [correction plan](../../superpowers/plans/2026-10-05-preserve-game-capabilities.md) records implementation and verification.
