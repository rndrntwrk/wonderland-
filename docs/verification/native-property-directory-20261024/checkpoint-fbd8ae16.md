# Native property directory — 24 October 2026

Native-owned players can search the authenticated property list, inspect a property's original identifier/location, enter through the existing source-owned admission path, leave back to the directory and repeat. An unconfirmed entry keeps its exact Sim/property visible and disables switching until the explicit same-operation retry resolves it. This is a configured native property-directory increment, **not** a complete city, multi-lot host, property-purchase or full-game implementation.

Tested product source: `fbd8ae164f503e875c11100f00345a930d847a3e`, tree `463042184b93d5532ec65943ae209ff9e6feb6d9`. It incorporates the directory/provider integration at `5481c7ac70a38a2e9e61762506c3ab8c6ecea490` and the scoped responsive layout correction at `fbd8ae16`. Base: draft #84 at `61ad4f95e92fe53eeee5bcdc72ecd79780e62d3b`, tree `0f04323f7bb4c4311c47584e15511bebf363c056`; local equivalent base `5da7984f`. Later changes add tests/inventory/sanitized documentation only. Public delivery overlays only these postimages on #84 and excludes private local history.

## Reused authority and design

The list comes from existing validated native roster `destinations`, not invented records, a legacy session, a new directory database or a fixture-only success path. Rows, search and details reuse the existing directory/placard blue chrome. Search is local to those authenticated records and supports name or exact property identifier; it does not query an unimplemented city service. Location unpacks the original supplied value. No owner, price, online status, neighborhood or right to purchase is fabricated.

The existing provider/native server remains the authority for entry, placement, exact operation identity, save/departure, fencing and recovery. The only new published provider state is the pending avatar/property IDs; bearer tokens and operation values stay private. Unknown entry cannot silently switch to another eligible Sim/property. Directory refresh remains disabled until that selection resolves. Sign-out removes the projection and existing cleanup handles late replies. No service, native runtime or protocol/durability logic changes.

## Actual rendered journey and QA

Environment: existing isolated loopback native/workerd fixture, existing original avatar cohort and real disposable account. The ephemeral port was not retained as a public/live URL. Browser plugin is unavailable in this executor; the established sandbox-enabled Chromium/Playwright route was used, with no `--no-sandbox` or `--disable-setuid-sandbox`. Desktop 1200×800 and mobile 390×844 were exercised.

| Check | Result |
|---|---|
| Page identity/content | Actual fixture origin and Wonderland title checked; real directory, property and native actor render |
| Blank page/framework overlay | Meaningful controls/world appear; no overlay in reviewed images and no page exceptions |
| Search/details | Matching case-insensitive search, empty results, property ID/location, dismissal and filtered-detail cleanup pass |
| Actual entry/departure | Directory entry reaches source-owned placed/rendered Sim; confirmed save/departure returns to usable directory |
| Interrupted/repeated flow | Server commits second entry, reply is deliberately lost, explicit retry sends identical bytes/operation; exactly one native actor remains |
| Preservation/cleanup | Chat sends after re-entry; second confirmed departure, successful roster refresh and sign-out pass; Buy/Build remains disabled |
| Responsive visuals | Desktop entry control is visible; mobile retry is reachable without horizontal overflow; final images reviewed |
| Console | Zero page exceptions; one intentional ERR_FAILED for dropped entry reply, one existing preload-integrity warning, three content lifecycle cancellations |

Four fresh browser groups pass. All 18 recorded roster/entry/leave responses are HTTP 200, including repeated departure polling until actual detachment. Heading visibility alone is not treated as completed departure: the probe waits for the controls to become enabled after confirmation. The rendered fixture has one owned Sim and one real configured property; multiple eligible Sim/property refusal is also tested at the actual provider boundary using explicit catalog inputs. This does not establish multi-property hosting.

The initial implemented run passed its first three groups but checked search availability before the second departure finished. That failed result is preserved. Visual review separately found the old login columns compressed the new card; the directory now owns a scoped responsive account layout. A fresh final run passes without relaxing authority or substituting a UI success state.

## Source and artifact checks

All 49 related account/creator/chat/Timer JavaScript cases pass, including 29 owned-account cases. The new pending-selection test first failed semantically because the provider omitted that state. The preceding browser build first failed at the missing directory heading. The targeted WASM check passed after correcting a moved String caught by compilation. Pinned offline release packaging verifies 153 artifacts. The final CSS-only packaging reuses exactly identical WASM bytes; the verified native host is reused because all native/service inputs are unchanged. No dependencies were installed and no redundant native build/performance suite was run.

Published fast reproduction:

```sh
node --test apps/web-shell/scripts/native-owned-account.test.mjs apps/web-shell/scripts/native-creation.test.mjs apps/web-shell/scripts/native-chat.test.mjs apps/web-shell/scripts/native-timer.test.mjs
```

The real rendered fixture additionally needs the retained private original avatar inputs, authored native setup, matching pinned release and existing Playwright installation. It is not reproducible solely from public source without those inputs. [Sanitized evidence](checkpoint-fbd8ae16-evidence.json) records exact results and hashes. Raw fixtures/images/stores stay private and are not presented as public downloads. Review is by the sole implementer under the one-task/no-new-agents instruction; no new independent review or full repository format/type-check pass is claimed.

## Remaining actual gaps

Native city terrain continuity, people/neighborhood/top-lot discovery, multi-lot provider/routing, property/home ownership/purchase and full city service integration remain open. No alternative backend was invented. Retained content did not supply a real original needs-restoring object cohort, so that full needs cycle was not attempted or relabeled complete. Original Timer world graphics (`timer.spf`/its `objspf*.far` cohort) and original panel skin inputs (`timereod.uis`/referenced images) remain absent; its rendered-object/panel gate is unchanged.

Existing creation, movement, chat and Timer source are incorporated. Baseline TS7016, unqualified-invocation TS5097 and the existing Rust proc-macro future-compatibility warning remain. Hosting prerequisites are separate and unchanged; the live Worker remains static. No credentials, grants, installations, paid resources, asset/media uploads, deployment or merge occurred. No new 30 Hz or exhaustive-game claim is made.
