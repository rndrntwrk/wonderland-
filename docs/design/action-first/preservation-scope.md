# Scope correction: modernize the game and preserve its capabilities

## User requirement

On 5 October 2026 the user clarified that this work must improve FreeSO/Wonderland's existing game experience across screens, menus, options, scenes and 3D views while preserving its features and customization. Character selection should be a visual grid; location selection should happen on the map; interaction should happen on characters and objects. Approval of that visual direction and permission to implement it did not approve reducing the game to five identities, three whole looks, six catalog items or furniture-only Build.

The current browser work is a limited interactive prototype. Its sample content was also hard-coded into contracts, validation and controls. That is an implementation gap, not an accepted product requirement. The previously recorded prototype limits must not be carried into the production design by default.

## What the preservation check established

The read-only comparison from original fork baseline `4c6b3e8f5835b228723caea3c9f683c62f244f73` to UI source `a1130b21fdd07a31714ba975a1e39efbd946dfd9` found no deleted paths. The only modified pre-existing path was `.gitignore`; legacy `TSOClient` and `Other` source remained unchanged. The browser code was added alongside the original game.

That establishes preservation of the original source. It does **not** establish that the new browser experience preserves all original functionality. PRs #5 and #10 were still open, unmerged drafts when checked for this correction.

Reproduce the source-preservation comparison:

```sh
git diff --name-status --diff-filter=MD 4c6b3e8f5835b228723caea3c9f683c62f244f73..a1130b21fdd07a31714ba975a1e39efbd946dfd9
git diff --name-only 4c6b3e8f5835b228723caea3c9f683c62f244f73..a1130b21fdd07a31714ba975a1e39efbd946dfd9 -- TSOClient Other
```

## Verified gaps to correct

| Existing capability or requirement | Current browser prototype | Required redesign behavior |
| --- | --- | --- |
| Content-backed, independent head and body outfit selection | Five fixed `VisualIdentity` variants, each represented by whole-character PNGs | Populate visual head/body choices from the actual content collections and preserve independent selection. Sample characters may seed a demonstration but cannot define the content model. |
| Skin appearance and gender choices | No corresponding creator controls | Preserve the existing choices in compact visual controls beside the character. |
| Name and profile description | Name only | Preserve both fields while keeping the character stage dominant. |
| Rendered, rotating avatar preview | Static full-body image | Integrate an actual avatar rendering/appearance interface and preserve the preview behavior. |
| Account-managed character availability | Fixed eight-profile cap in the preview contract and UI | Use the actual account/service policy. Do not infer a new product slot limit from fixture convenience. |
| Existing Buy categories and content | Six exact records, four fixed categories; validation rejects other catalog records | Populate categories, content, thumbnails, availability and prices through the content/service interface, preserving the original catalog's scope. |
| Build tools for terrain, walls, wallpaper, stairs, floors, doors, windows and roofs | Furniture Move/Rotate/Store | Modernize the existing tools as direct scene actions with their original capabilities, permissions and feedback. Room arrangement alone is not Build parity. |
| Real lot and renderer data | An illustrated 8 by 6 room with fixed projection | Take geometry, levels, orientation and picking from the actual world/renderer interfaces. Keep fixture geometry separate from game contracts. |
| Full screen/menu/view coverage | Character, map, cafe and Home prototype journeys; Park and Arcade unavailable | Complete the source-backed screen inventory and map every existing capability to its redesigned control and implementation status. Unimplemented screens remain explicit gaps. |

Original creator evidence: [PersonSelectionEdit.cs](../../../TSOClient/tso.client/UI/Screens/PersonSelectionEdit.cs) loads the male/female head and body collections, constructs separate selection browsers, exposes skin and gender choices, and creates an auto-rotating `UISim`. [PersonSelectionEditController.cs](../../../TSOClient/tso.client/Controllers/PersonSelectionEditController.cs) submits separate head/body outfit IDs, name, description, gender and skin tone.

Original construction/catalog evidence: [UIBuildMode.cs](../../../TSOClient/tso.client/UI/Panels/UIBuildMode.cs) and [UIBuyMode.cs](../../../TSOClient/tso.client/UI/Panels/UIBuyMode.cs). Current restrictions are in [authoring types](../../../crates/contracts/src/authoring/mod.rs), [validation](../../../crates/contracts/src/authoring/validation.rs), [geometry](../../../crates/contracts/src/authoring/geometry.rs) and the [creator](../../../apps/web-shell/src/screens/creator.rs). This is a verified gap list, not an exhaustive audit of every original game feature.

## Implementation direction from this correction

1. Establish an explicit original capability → redesigned control → implementation/integration status map before declaring a screen complete.
2. Generalize appearance, catalog and lot contracts around the real content, account and renderer interfaces. Move example identities/items and demonstration policy into the fixture adapter. Keep schema validation and resource-safety limits; replace exact demo membership with validation against actual versioned content and service policy.
3. Bring the creator's independent choices and rendered preview across first. Additional whole-character images do not close this gap.
4. Apply the same preservation rule to Buy, Build, world interaction and the remaining screens. Keep the approved action-focused presentation where useful.
5. Verify both usability and feature preservation. Passing tests written around reduced prototype requirements is insufficient evidence of a completed modernization.

This correction changes the recorded scope and interpretation of the existing PRs. It does not itself remove the runtime restrictions or complete the missing integrations. The previously passing tests and release build remain evidence for the limited prototype, not evidence of full FreeSO parity.
