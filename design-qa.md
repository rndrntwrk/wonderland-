# Wonderland character and Home UI — design and browser verification

## Result

This increment extends the approved game interface with a visual character creator, wardrobe, a separate Home, a furniture catalog, direct placement, room arrangement, and owned inventory. It is stacked on the original character/map/café UI in PR #5. The original report is preserved in [initial-ui-qa.md](docs/design/action-first/initial-ui-qa.md).

The actual Rust/Leptos release application was exercised in the browser. Its preview provider acknowledges operations after 850 ms; only a validated matching commit changes the accepted character, room, ownership or budget. Accepted local preview changes survive refresh. These are working preview journeys, not production account, economy or simulation services.

**Verified product source:** `d2561a7d7c01cc04d4897a9880a76ab5c7073f5f`, actual release completed **13:14:50 UTC on 5 October 2026**. All **72 workspace tests**, formatting, native clippy and WASM clippy passed on this source. Later documentation commits preserve this product tree. Final whole-branch review and remote CI outcomes are reported with the PR.

**Screenshot limitation:** the browser displayed the actual rendered creator, Home and comparison views. The supported screenshot export again failed to synchronize a saved JPEG into the execution workspace, so this report does not embed a missing file or substitute a reference image for implementation evidence. The compiled live preview and reproduction instructions are available for review. Publication proceeds as a draft, as required by the specification.

## Design and source of truth

- [Character and Home specification](docs/superpowers/specs/2026-10-05-character-and-home-ui.md) and [implementation plan](docs/superpowers/plans/2026-10-05-character-and-home-ui.md).
- [Visual design record](docs/design/action-first/README.md), including the selected [character](docs/design/action-first/references/characters.png), [city](docs/design/action-first/references/city.png), and [café](docs/design/action-first/references/lot.png) references.
- [Asset manifest](apps/web-shell/public/assets/authoring-manifest.json): 20 new runtime PNGs, 34,165,660 bytes, exact generation lineage, dimensions, alpha bounds and checksums. These extend the existing five Everyday characters with ten wardrobe looks, a separate unfurnished Home, and nine furniture views.
- [Authoring contract](docs/contracts/authoring-v1.md), [run and fixture instructions](apps/web-shell/README.md), and [service/content/3D integration handoff](docs/integration/character-home-ui-handoff.md).

The character remains the focus of creation and outfit changes. Appearance and look tiles update the large stage immediately; the only name entry is a compact nameplate. The map remains the location selector. Home editing uses object thumbnails, scene cells and a placement ghost, then actions attached near the selected furniture. The room remains visible above the lower drawer.

The retained style uses rounded blue game chrome, green selection and primary actions, bundled Nunito typography and Tabler icons, warm scenic art and independent transparent character/object images. The new views extend that visual family; they do not have separate approved full-screen raster references and are not claimed as pixel-identical reproductions.

## Browser conditions and visual comparison

| Condition | Actual observation |
| --- | --- |
| Desktop | Main application at 1363 × 936 CSS pixels, DPR 1. Creator, character grid, map, Home, catalog, object actions, café and dialogs inspected. |
| Narrow | Same-origin live application iframe at 390 × 844 CSS pixels, DPR 1. Character roster, creator and Home measured with equal client/document dimensions and no page overflow. This is a CSS viewport check, not physical-device certification. |
| Matched reference family | Each original 1672 × 941 reference displayed beside a live 1672 × 941 iframe at the same 660/1672 scale and crop. Current character, city and café full views inspected on the 13:01 UTC release. The prior detailed-crop record remains in the original QA report. |
| Character hierarchy | Five profile cards and Create slot, independent body stage, selected green treatment, green Play, visual outfit control. Pagination fits below the grid. |
| Map hierarchy | Building selection and an attached destination placard with Visit; scenic geography remains the primary surface. |
| Café hierarchy | Independent character/object presentation, object-attached action petals, compact needs and queue. Fixture values differ from decorative mock values. |
| Home geometry | Explicit 8 × 6 affine floor projection; background, grid, ghost, footprints and placed objects share the camera transform. Directional sprites use separate front/back art and mirroring, with alpha-bounds width normalization. |

The local-only [comparison helper](tests/ui/prepare-browser-qa.py) prepares matching live/reference pages and the narrow iframe in ignored `dist` after a Trunk build. It does not replace the application with a screenshot. A clean release build removes those diagnostics; they are not publication inputs.

## Journeys verified in the actual browser

| Journey | Observed result |
| --- | --- |
| Appearance and look draft | Maya → Leo → Active changed the full-body preview. Unicode name `試作 Éloïse` was accepted. Cancel returned to the unchanged roster. |
| Create and select | Created `Éloïse` using Amara/Smart. During pending, presets, looks, name, Cancel and submit were disabled. Acceptance produced exactly one selected/focused new profile on page 2. |
| Outfit Cancel and Save | Active preview then Cancel retained `amara-smart.png`. Save locked controls while pending; acceptance changed the stage to `amara-active.png`. |
| Refresh persistence | Reloading the normal URL retained Éloïse on page 2 with the accepted Active outfit. The default initial selection remained Maya. |
| Home entry and portraits | Playing Éloïse opened the map with the correct budget. Home visit entered a distinct Éloïse Home. The entrance sprite used the accepted Active look. Pending and arrival messages named Home. |
| Category/search | Decor plus `zzzz` showed a useful no-match message; clearing search and choosing Living returned the armchair and coffee table. |
| Invalid placement | Entrance cell (0,0) showed an explicit reserved-entrance reason and disabled Buy and place. A rotated out-of-room candidate also showed its validity reason. |
| Purchase | Chair at (2,2): pending disabled navigation; acceptance placed one instance and changed budget from 1250 to 1070. |
| Move and Cancel | The purchased `preview-instance-4` moved as a draft; Escape restored the committed position. A later accepted move preserved its ID and budget. |
| Store and place again | Store removed the chair from the room and exposed it in inventory without refund. Placing it again at (2,2) reused `preview-instance-4`, kept budget 1070 and emptied inventory. |
| Per-profile isolation | Ordinary Maya retained an empty room and budget 1250 while Éloïse retained the chair and budget 1070. |
| Narrow character UI | All six roster choices, full body and primary action fit 390 × 844. Play measured x25, y679, width340, height67. Creator choices, body, nameplate, Cancel and Create were visible together. |
| Narrow Home | The corrected room fills the 734-pixel usable Live scene above the compact drawer, without the previous unused vertical gap or document overflow. |
| Nearby object actions | Arrange armchair group measured x48, y281.5, width330, height60 on narrow. Escape removed it and restored the furniture button's focus; no independent scene scrolling was observed. |
| Placement keyboard | Catalog selection focuses the Home scene. Arrow keys move the candidate, R changes direction, Enter confirms and Escape cancels. Current-release edge checks passed; details are recorded below. |
| Existing café | Visit named Harbor Café. Selecting the machine opened its actions; Make coffee showed Requested then Accepted in the queue. Existing unavailable Clean retained its reason. |
| Needs/settings | All needs exposed eight named meters. Escape closed the needs dialog and restored All needs focus. Reduce motion changed state; Escape closed Settings and restored Settings focus. |
| Existing empty roster | `?fixture=empty-characters` showed the empty state and disabled Play. Try again restored five profiles and focused Maya. |

## Failure and unavailable states

| Scenario | Browser result |
| --- | --- |
| `reject-authoring` | Fixture bypassed saved Éloïse data. First Create for RetryTest rejected with name/look draft intact and a clear error. Retry created exactly one selected new profile and cleared rejection. |
| `poor-home` | Maya budget 20; fern price 45. Placement showed insufficient funds and disabled confirmation. |
| `read-only-home` | Buy showed the provider's read-only reason and disabled confirmation. The corrected empty Build view also displayed “This Home is read only in this preview.” |
| `empty-catalog` | Corrected Buy view displayed “No furniture is available in this preview.” It no longer suggested clearing an already empty search. Filtered no-match and empty inventory keep separate guidance. |
| Invalid saved data | Native codec tests cover corrupt, unsupported, invalid and oversized envelopes. The storage adapter preserves the existing value and disables writes while providing a temporary preview. Browser storage denial and corrupt-storage injection were not performed. |

Console inspection returned 378 warning/error records for the checked main-tab flows, all from the browser extension's metadata messaging at a `chrome-extension://` URL. No application-origin warning/error entry appeared in that returned set. This is not a claim about unobserved devices, multiplayer or production renderer behavior.

## Corrections made during verification

| Finding | Correction and evidence |
| --- | --- |
| Home travel feedback incorrectly named the café | Feedback now derives from the request destination; native regression and actual Home/café pending/arrival checks passed. |
| Visible cell coordinates made placement look like a debug overlay | Visible coordinate pills removed; 48 named semantic cell controls remain available. |
| Furniture arrangement actions lived in the bottom drawer | Move/Rotate/Store/Done moved near the selected furniture with usable-area clamping; narrow position and Escape focus rechecked. |
| Narrow Live left about 245 pixels of unused space | Scene viewport and initial camera fill corrected; 390 × 844 recheck confirmed the room fills the usable scene. |
| Read-only Build and provider-empty catalog gave misleading guidance | Two focused rendering branches corrected and independently re-reviewed; both messages confirmed on the actual 13:01 UTC release. |
| Camera revealed only the ground point at room edges | On narrow, a valid East coffee table at (0,4) had a 204.88-pixel ghost extending to x−62.44. Complete sprite/footprint bounds now drive reveal. Current-release repeat at (0,4) had x44.42–249.30; (7,4) and (7,0) had x140.70–345.57, all within the 390-pixel viewport. South (1,5) used the back view, x117.58–316.54. All were valid, with no page overflow. Enter at (3,2) accepted one table and changed budget1070→950; Reset and nearby owned-object actions remained usable. |

## Native, WASM and review evidence

The first increment's 29 tests are preserved. Task 1 adds 31 authoring tests covering typed receipts, snapshot validation, stale/duplicate outcomes, preview provider atomicity, ownership, budgets and inventory. Task 2 initially adds ten tests for persistence envelopes, current-reply classification, destination feedback, affine geometry and overlay placement. The integrated pre-camera run passed **70 tests**.

After the camera correction added two behavioral geometry regressions, the final integrated run on `d2561a7` passed **72 tests** (29 original +31 state/provider +12 adapter/geometry/persistence). Workspace format, native clippy and WASM clippy all exited0. The worker’s actual 13:14:50 UTC release was reloaded and verified in the browser. Commands are `cargo test --workspace --locked`, workspace format, native and WASM clippy with warnings denied, and the actual `NO_COLOR=true trunk build --release --locked`. Rust 1.99.0, Trunk 0.21.14 and the existing lockfile are used. No dependency package versions were upgraded and no Node tooling changed. The existing dependency's `proc-macro-error2` future-compatibility notice remains separate from project warning gates.

Task 1 and Task 2 have independent scoped source reviews. The two Home feedback fixes and complete-bounds camera correction were re-reviewed without new source findings. The earlier effective sprite-width test gap was also closed by the camera regression. This report is part of the final whole-branch review input. Publication requires the remote Git tree to equal the reviewed local tree; the final verdict, exact-tree check and GitHub CI are recorded with the PR.

## Screenshot evidence limitation

At 13:12 UTC on 5 October 2026, the documented browser screenshot API captured the actual creator as a 256,229-byte JPEG. The same bytes were displayed and written to `/home/oai/share/browser-screenshot-1791205952711.jpg`. The matching execution-workspace path did not appear within a bounded five-second poll. No alternative browser controller, manual container transfer or reconstructed image was used. Earlier first-increment attempts are documented in the archived QA report.

The browser-rendered views were visually inspected, but a durable screenshot attachment remains unavailable. Reviewers can run the compiled app using the shell README and reproduce the journeys above. The PR remains a draft; this export limitation does not conceal an application defect or prevent reviewing the committed implementation.

## Scope and next integration work

This increment completes the specified character/Home preview journeys. It does not complete the entire W11 screen inventory or replace FreeSO's game renderer. Appearance choices are whole looks across five identities, with eight local profile slots. Build arranges furniture; architectural tools, a live economy and live account saves are outside this provider.

The Home projection is a calibrated fixed-camera illustrated adapter with directional sprites. Production 3D requires a renderer/pick interface, canonical lot-coordinate and ID mapping, authoritative placement transactions, rigged content, motive conversion and shared dependency integration. The source-backed handoff documents those boundaries against the inspected simulation/content branches. Other swarm PRs have not been merged or repinned.

Remaining polish includes production asset cooking and replacing raster zoom softness through the renderer/content pipeline. No physical touch device, screen-reader software, real GPU or multiplayer certification is claimed.

Final result: specified preview journeys verified locally; durable screenshot attachment unavailable. Final review and remote CI status accompany the stacked draft PR.
