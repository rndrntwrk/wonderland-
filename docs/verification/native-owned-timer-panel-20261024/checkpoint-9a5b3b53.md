# Native-owned Timer transport and controls — 24 October 2026

The native-owned browser now has a source-driven Timer panel and an authenticated private EOD HTTP lane. This closes the binary-only socket/private-text mismatch and supplies the retained source-session UI sequence needed after reconnect. **Rendered object selection and panel visual acceptance remain blocked/unqualified.** This is a functional source/transport increment, not a completed non-Walk browser journey or hosted-game release.

The tested product source is `9a5b3b53d966495a8f9b283810cc9aed0229b83d`, tree `49a5b70f141b8d1a79468d064eb3904f5f90de69`, incorporating transport/native cursor `971717e1`, panel/bridge `e25ee7a0` and the stale-session fix `9a5b3b53`. It is stacked on draft #83 at `9b62016b40d1d722a8197db44d251b7b3d0b3b8b` (tree `20850fdc449fed083ded7c8d5fe7453f2763cdb7`); local base `82c9728b` has identical preceding product postimages. Later changes add tests and this inventory/evidence only. Public delivery overlays these scoped postimages on #83; private local history and assets are excluded.

## Behavior and authority

`POST /native-account/eod` accepts only the selected avatar, lot, retained entry operation, current incarnation and strict private query. Every request revalidates the account reservation and source-owned in-world placement through the existing serialized authority/fence boundary. UI and delivery acknowledgements retain the existing native prepare/commit semantics. A returned UI receipt never substitutes for action completion.

Owned browser sockets explicitly opt out of private text and continue to process binary native frames. Older clients omitting the opt-out keep the original private-text behavior. Read-only private metadata returns only source sessions owned by the current control, including next UI sequence and last receipt. No persisted schema, authority check, receipt limit, simulation tick or durability acknowledgement is changed.

The panel opens only on a verified original `Timer_Show` for plugin `0xAA65FE9E`. It ports the original Set, countdown/stopwatch, start/pause, close and Update→Set feedback behavior into existing application chrome. It verifies private digests before presentation/acknowledgement, retains exact uncertain control bytes for explicit retry, and fences old-session confirmations from newer windows. It does not invent elapsed time, action terminals or object art. Full original UIScript skin parity is not claimed.

## Actual qualification

- 28 focused client/socket/private-route cases, 27 owned-account cases and five preview-policy groups pass. New source-scope, sign-out/late-reply, Origin/method, exact retry and stale-session cases are included.
- One focused native cursor test passes; 35 other native tests were filtered out, not reported as passed. Targeted WASM checking and pinned offline release packaging pass. All 153 generated artifacts, including the copied Timer module, were rehashed. Existing Rust 1.99 / Trunk 0.21.14 / wasm-bindgen 0.2.129 / Binaryen 123 pins were reused without installation.
- Five real native/workerd HTTP groups pass with two isolated local accounts and independent authentic NativePlayers. The probe selects actual source entity 2/generation 1, GUID `0x39D04C9C`, local TTAB 5 “Be Timekeeper” from projected offers. Actual admission/command acceptance produces private Show. Set/mode/start/pause commit; exact retry retains one receipt; changed bytes and foreign account/session/lot/incarnation/operation/Origin requests reject.
- Source Close emits private exit and a real typed `completed` VM terminal (action 3). Both clients match on 203 common ticks with no thread faults. Process restart preserves the closed Timer, renews bindings and rejects the old incarnation. This is closed-session recovery, not fresh active rendered-panel recovery.

The real source offer was invoked programmatically by the existing NativePlayer probe. **It was not clicked through a hidden fixture UI and is not counted as a rendered object click.** No actual Timer panel visual/DOM journey, rendered queue cancellation, or active browser-panel reconnect/recovery pass is claimed. The first probe omitted the required WebSocket Origin; its failure is retained, and correcting the harness produced the passing result without a product change. The stale-session semantic failing test is also preserved. Sole-implementer source review is recorded; no new independent review is claimed under the single-task/no-extra-agent instruction.

## Exact missing content and remaining gate

The retained original `timer.iff` is 12,727 bytes, SHA-256 `6927573513f0ffdcc26879d8366242f8c26c03baf21080d1a7126386ff04a092`. Its inspected chunk table has zero SPR, SPR2 or DGRP chunks. The authored intake explicitly says visuals were not imported; the current 30-file pack supplies avatar resources, not Timer world graphics.

The next required input is the matching original **`timer.spf` graphics from its `objspf*.far` cohort**, resolved through `packingslips/objecttable.xml` for GUID `0x39D04C9C`, or a provenance-verified matching original Timer FSO mesh and textures. `TSOClient/tso.content/WorldObjectProvider.cs` selects `objspf*.far` and loads the object reference basename plus `.spf`. Pixel-faithful panel skin additionally needs **`timereod.uis` and its referenced background/button graphics**, as loaded by `TSOClient/tso.client/UI/Panels/EODs/UITimerPluginEOD.cs`. Relevant retained asset/package indexes have no such graphics. No invented mesh/skin or broader search is substituted. This is a missing-input blocker, not a denied permission.

After that intake, the remaining gate is actual visible object click → source-authorized offer → accepted queue action → displayed Timer and controls → source completion/cancellation, peer agreement and active reconnect/recovery. Existing creation, movement and public lot chat remain incorporated. Other object/EOD/person families and the full game inventory remain open; Buy/Build stays disabled.

## Reproduction and evidence access

Run the published fast checks from the repository with the existing Node runtime:

```sh
node --test apps/web-shell/scripts/native-timer.test.mjs apps/web-shell/scripts/native-socket.test.mjs tools/native-browser/owned-demo/eod.test.mjs
node --test apps/web-shell/scripts/native-owned-account.test.mjs
node --experimental-strip-types --test services/native-edge/tests/preview-policy.test.mjs
```

The focused native command uses the existing locked offline Rust toolchain: `cargo +1.99.0 test --release --locked --offline -p wonderland-native-lot-host private_ui_cursor_retains_only_the_requested_source_session_sequence`. Matching browser packaging requires the recorded pins. The real-source probe additionally requires the privately retained original inputs and local authoring setup; it is not reproducible from public source alone without those inputs. [Sanitized evidence](checkpoint-9a5b3b53-evidence.json) records exact results and hashes; the hash references identify preserved private receipts and do not pretend to be public downloads. No original content, credentials, stores, recordings or raw private traces are published.

Baseline generated crypto declaration TS7016 and TS5097 under an unqualified tsc invocation remain; no full repository formatting/type-check pass is claimed. Existing hosting access/Linux-image/startup-input prerequisites remain. No R2 retry, new access, installation, spending, deployment, merge or performance retest occurred.
