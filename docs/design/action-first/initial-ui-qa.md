# Wonderland action-focused UI — design and browser verification

Historical verification for the first UI increment (PR #5). See the [current verification report](../../../design-qa.md) for character creation, wardrobe, and Home authoring.

## Result and remaining evidence gate

The character, map, and café interfaces were rendered in the browser and compared with their approved references, including matching full views and detailed crops. The material visual and behavior findings listed below have been corrected. The first increment remains an illustrated UI preview with explicit fixture replies.

**The durable screenshot evidence gate is blocked.** The supported browser API displayed actual screenshots, but files written through its documented shared directory disappeared before they synchronized to the execution workspace. Two destination paths and a bounded five-second poll were tried; neither produced a usable file. No alternative browser-control mechanism or manual container transfer was used. This report therefore includes the observations and reproduction instructions, but does not embed a fabricated or missing implementation screenshot. The pull request remains a draft rather than claiming completed visual handoff.

- Source visual truth: [characters](references/characters.png), [city](references/city.png), [lot](references/lot.png).
- Implementation screenshot path: **unavailable — browser export/synchronization failure**. Browser-rendered screenshots and combined comparisons were inspected in the work session on 5 October 2026.
- Implementation: the actual Rust/Leptos release WASM build served by the supervised local preview; no reference image is used as an interactive page.
- Scope: [approved specification](../../superpowers/specs/2026-10-05-action-first-browser-ui.md), [design record](README.md), and [shell README](../../../apps/web-shell/README.md).
- Final product commit checked: `79aceeaad622c73679c5b6dfaa0beb462f15b66c`; release distribution applied at **09:30:23 UTC on 5 October 2026**. Later changes in this increment add this report and the local QA helper only.

## Comparison conditions

| Check | Source / implementation dimensions and state |
| --- | --- |
| Matched desktop fidelity | All source PNGs are **1672 × 941 pixels**. The live app ran in a **1672 × 941 CSS-pixel iframe at DPR 1**, verified through its document dimensions. Source and live frame were displayed together at the same scale, 660 / 1672, with the same crop. Browser chrome and comparison-page padding were excluded from design judgments. |
| Normal desktop use | **1363 × 936 CSS pixels, DPR 1**. Verified the actual full-page character, city, café, action, and needs states. |
| Narrow layout | A same-origin iframe measured **390 × 844 CSS pixels, DPR 1**. The character page's small vertical overflow reduced its inner client width to 375 pixels; city and café measured 390 × 844 with no document overflow. This is a CSS viewport check, not physical touch-device or GPU certification. |
| Character comparison | Maya selected, five named portraits, creation slot, independent full-body stage, Play as Maya. |
| City comparison | Harbor Café selected, six visitors, Visit available, default camera. |
| Café comparison | Maya, selected coffee machine, Make coffee / Clean / Inspect petals, compact needs HUD. Clean is intentionally unavailable in the fixture. Queue content and money come from projection data, so the mock's decorative values are not treated as an exact state match. |

The [local comparison helper](../../../tests/ui/prepare-browser-qa.py) prepares full and focused comparison pages in ignored `dist` after a build. It presents the source image beside the **live app iframe**, then changes both crops together without changing app state. It does not inject a simulation or fabricate a screenshot. The helper and the mobile iframe are outside production build inputs.

### Full-view and detailed comparison evidence

Each of the following was actually opened with both source and implementation visible in the same browser capture. Separate remembered images were not treated as a side-by-side comparison.

| Screen | Full-view comparison | Detailed region in source coordinates | Outcome |
| --- | --- | --- | --- |
| Characters | `/__verify-characters-comparison.html` | x45, y105, width880, height685 | Large 3 × 2 portrait grid, selected green border/diamond, full-body stage, and dominant green Play preserve the approved hierarchy. Detailed view checked names, portrait crops, border treatment, typography, and creation affordance. |
| City | `/__verify-city-comparison.html` | x590, y440, width575, height380 | Same town geography and building-first selection. Detailed view checked the café footprint, placard pointer, Visit, café name, six-visitor label, icons, and blue/green state colors. |
| Café | `/__verify-lot-comparison.html` | x795, y260, width500, height390 | The world remains dominant, with independent character/object sprites, object-attached petals, and a compact HUD. Detailed view checked the three actions, close control, object thumbnail, disabled Clean reason, font weight, icon alignment, and spacing. |

These are local diagnostic routes, not deployed public links. Use the controls inside the live iframe to reach the stated screen, then the diagnostic page's **Toggle detailed crop** button to compare the same region. A new Trunk build removes the diagnostic pages; do not publish a `dist` directory that contains them.

## Required fidelity surfaces

| Surface | Specific evaluation |
| --- | --- |
| Fonts and typography | Bundled **Nunito**, with Arial/sans-serif fallback and font synthesis disabled, is a documented approximation of the rounded heavy lettering in the generated references; those raster references contain no authoritative font metadata. White display text, dark blue outlines/shadows, heavy action labels, and readable smaller visitor/need labels maintain hierarchy. The mobile creation caption's wrapping defect was fixed. No leaked Rust/template text remains in camera controls. |
| Spacing and layout rhythm | Desktop grid/stage proportions, rounded blue chrome, green primary controls, compact map placards, three-petal action grouping, and bottom HUD agree with the action-focused design. The narrow character composition was changed to three columns by two rows so all choices, the full-body stage, and Play are visible together. Map and café controls remain within the viewport; focused targets are revealed by camera movement rather than hidden browser scrolling. |
| Colors and visual tokens | The implementation uses blue `#1358a5`, dark blue `#0a3775`, light blue `#74c5ff`, green `#75fc38`, warm-white text, and the documented beveled chrome gradients. The full and detailed comparisons preserve the reference's blue secondary actions and green selection/primary-action hierarchy. Disabled state also has explicit text; it is not conveyed by color alone. |
| Image quality and asset fidelity | Actual generated scene PNGs and independent transparent character, machine, and diamond sprites are used. The visual inspection found clean compositing without opaque transparency boxes. Portrait identity, outfits, world geography, and café subject matter follow the references. Companion-art pose and machine-model differences are expected fixture variations. Icons are the bundled original Tabler assets, not emoji or hand-drawn replacement illustrations. The PNGs become softer at maximum zoom; production renderer content and image cooking remain separate work. |
| Copy and app content | Character names, Play, named map destinations, Visit, three object actions, all eight needs, and concise unavailable/retry messages stand on their own as game UI. The small UI preview label identifies the increment. There is no account-form substitute for character selection or destination form replacing the map. Request and cancellation feedback reflects actual fixture replies, with concurrent reply classification covered by a regression test. |

## Findings and correction history

| Severity | Earlier finding and evidence | Correction and post-fix evidence |
| --- | --- | --- |
| P1 | Initial city/café camera markup leaked a Rust comparison/handler expression into the Zoom in button; both zoom buttons were disabled. The leaked text also produced page overflow and scroll displacement when selecting Café. | Braced the Leptos comparison closures. Browser recheck: Zoom in enabled at fit, Zoom out enabled after zoom, Reset restores fit, no source text displayed, and desktop document remains 1363 × 936 at scroll (0,0). |
| P2 | Initial 390 × 844 character layout used a tall two-column grid and required scrolling to reach Play; document height was 1150. | Compact three-column grid and shorter full-body stage. Browser recheck: all six cards and the body are visible; Play occupies y699–766 in the first viewport. |
| P2 | The compact Create a Sim caption wrapped into two lines inside a one-line strip. | Mobile caption uses a smaller no-wrap label while retaining the full accessible name. The final 390 × 844 screenshot confirms the complete single-line caption inside its strip, with all six cards, the full-body stage, and Play visible. |
| P2 | At maximum zoom and two ArrowUp pans, Café's pick began at y971.868 below the 936-pixel viewport. Tab focused it by implicitly scrolling the overflow-hidden scene by 749 pixels, while the camera transform and placard projection stayed unchanged. The placard appeared below the building. | Reveal focused anchors on both axes and use non-scrollable scene clipping. Exact browser repeat: target becomes y222.876–713.058, placard y181.891–261.891 above the building, and scene, outer screen, and page scroll offsets all remain (0,0). The camera transform performs the recentering and the focus outline is visible. |
| P2 | Independent source review found that a rejected action followed by another outstanding action's acceptance could announce the old persistent reducer error as the current reply's outcome. | Feedback derives from the current event and `receive` result. The focused native regression submits both requests before either reply, retains the earlier reducer error, then verifies accepted feedback for the second reply. The browser automation's sequential-click latency exceeded the 850 ms fixture interval, so it is not claimed as a concurrent browser reproduction. |
| P2 | Final integration review found that the café object caller lacked the shared focus-reveal handler. At zoom 2.8 after thirteen ArrowLeft presses, the machine pick lay entirely offscreen at x1414.914–2113.415. | Object focus now uses the same camera reveal with the lot's chrome/HUD bounds. Exact browser repeat: Tab focuses the machine, moves its pick to x332.227–1030.727, places its anchor/label at y467.986, and leaves scene, outer-screen, and page scroll offsets at (0,0). After opening the menu and panning again, Escape dismisses it and restores the same visible object focus. The tall sprite remains partly above the viewport at maximum zoom; its action anchor and focus outline are reachable. |

No additional visual P0/P1/P2 differences were identified in the matched full and detailed comparisons. The screenshot-export gate above remains open independently of those corrected product defects.

## Browser interactions checked

| Journey or state | Observed result |
| --- | --- |
| Character selection | Clicking a different portrait updates selected state, the full-body stage, and Play text. Tab from Maya reaches Jules; Enter selects Jules. |
| Map and travel | Character → Play → café pick → Visit → pending state → matching café entry works. Park remains selectable with a disabled Visit and an explicit missing-scene reason. |
| Scene controls | Zoom, keyboard pan, Reset, common art/pick transform, anchored placard, and the maximum-zoom focus regression were checked in the actual browser. |
| Object actions | Machine selection opens three named buttons and places keyboard focus in the action group. Escape dismisses it and restores `object-coffee-machine` focus. At maximum zoom, both Tab and Escape reveal the offscreen object's anchor through camera movement, with no native scroll offset. |
| Queue and cancellation | A request appears pending, acceptance creates its queue entry, cancellation remains visible while pending, and the matching acknowledgment removes the entry. |
| Needs and settings | Four summary meters and all eight expanded meters have names and values. All needs and Settings are native dialogs. Escape restores `all-needs` and `settings` focus. The Reduce motion checkbox changes state. |
| Narrow map/café | Both documents fit 390 × 844. Object menu is 286 × 264, clamped within x92–378; all-needs dialog is 352 × 300.375 at x19–371, y325.625–626. Controls remain reachable while panning. |
| Travel rejection | `?fixture=reject-travel` retains selected Café, shows the busy reason and enabled Visit, then succeeds on retry and clears the error. |
| Empty characters | `?fixture=empty-characters` disables Play, displays Try again, restores five portraits, and focuses Maya after recovery. |

Browser console inspection found no application-origin errors in the checked flow. The browser's own extension emitted “Error sending browser metadata to extension” from a `chrome-extension://` URL; that is recorded separately and is not attributed to the application. No production GPU, physical mobile, screen-reader software, or multiplayer certification is implied.

## Code verification

The final product source passed `cargo test --workspace --locked`: **29 tests**, covering reducer/request identity (15), projection/fixture contracts (2), the preview reply adapter (3), camera and overlay geometry (7), and current-reply feedback (2). The final workspace formatting check passed. Native and WASM clippy checks passed during implementation; the final two-line lot caller also passed WASM clippy and the release Trunk/WASM build. The diagnostic preparation script successfully generated all comparison pages after that build.

Independent review approved the contracts/state increment and the browser correction delta. Final whole-branch review includes the remaining lot focus caller and this QA/helper record. Cargo's existing `proc-macro-error2 v2.0.1` dependency emits a future-compatibility advisory; it does not fail the pinned Rust 1.99.0 build. No legacy C# suite or remote CI result is claimed by these local checks.

## Expected scope differences and follow-up polish

The approved images are the visual target for a larger game. This first increment omits the reference's 3D orbit/view toggle, social pins/chat bubble, character-screen home thumbnail, and game completion animations because their adapters are not present. Creation, outfit, Build, and Buy expose unavailable states. Harbor Café is the only implemented lot scene. These are explicit specification boundaries, not hidden working features.

The companion character poses and coffee-machine model differ from the reference illustration while preserving identities and scene roles. Nunito and the consistent Tabler icon family are close style matches, not recovered original assets. Portrait backdrops are busier than the source; simplifying those backgrounds is optional P3 polish after the interaction model is reviewed. Maximum-zoom raster softness is a content/renderer follow-up.

## Implementation checklist

- [x] Render actual compiled Rust/WASM controls and verify the primary action journey.
- [x] Compare all three source references with the live app at matching viewport and scale.
- [x] Inspect readable detailed crops for typography, controls, icons, imagery, and copy.
- [x] Correct the zoom markup, narrow Play layout, and creation caption.
- [x] Correct and reproduce the maximum-zoom keyboard focus/overlay defect.
- [x] Verify the shared focus behavior for the café object, including Escape restoration after panning.
- [x] Add a meaningful concurrent-reply feedback regression.
- [ ] Attach durable browser-rendered screenshots and close the visual evidence gate.

final result: blocked
