# Native browser continuation: tracked player integration and action admission

This continuation builds on PR27 at `8a5f6770a2d924e2012b8c66877ba7322d1a08ca`, itself stacked above PR26. It preserves the existing character selection, map, Home and legacy source-lot path. It does not merge or replace the separate renderer or legacy-inbox branches.

## What was actually broken

The published branch contained native player modules, tests and an assembly script, but its application call sites and dependency declaration were generated only in a special CI job. Ordinary source builds did not contain that assembled integration. The new authority-admission tests also imported a missing module, so the last candidate failed before building or testing a browser.

The correction materializes the existing call sites as ordinary tracked source, adds the missing action-admission module and removes the assembly/evidence-transfer scripts and write-enabled verification job. The permanent workflow verifies the actual checked-in tree with read-only repository permissions.

## Native action admission

`live_wire::player::admission::revalidate_action` runs only on an authority runtime. The caller supplies the principal and actor from authenticated lot admission, not from client assertions. Packet size/shape, current grant, actor and target generations, queue ownership/revision, source action and dynamic parameter, sequence and observation age are validated.

An ordinary tick arriving during network latency does not invalidate an otherwise available action: the authority executes the real source checks again and refreshes only the observation stamps. It does not retarget another object, invent a menu item, discard a changed queue, restore revoked permission or retry an old command. Observations beyond 128 ticks and observations from the future reject. Cancellation retains the exact visible queue action ID.

The query itself does not mutate state. Its returned intent must be committed under the authority's serialization boundary; the existing runtime validates it again on execution. A transport write is never an acceptance receipt.

## Tracked browser integration

- Native lots are opt-in with `native_lots: true` in version-1 connected startup. Omission retains the original server path; preview mode rejects native mode.
- The browser requests a short-lived native admission through the configured gateway using the existing in-memory authenticated session and its selected actor/lot incarnation.
- The socket destination is derived from the configured gateway, rather than accepted from an arbitrary server response. Tickets are sent in the first socket message, not a URL, and cleared from the JavaScript closure.
- Every binary callback reaches the real native player synchronously, retaining generation/session guards. Accepted simulation state drives the world, needs and action queue; source picks/Your Sim open current source actions.
- Exact request bytes correlate action decisions. A result may follow only its accepted state, not precede it. An interrupted pending result remains Unknown across reconnect and is not resent. Dismissing Unknown does not retry it; a newly selected action gets a later sequence.
- Route changes, sign-out and disposal retire the socket and callbacks. Recovery preserves a matching player/cursor and requests a fresh admitted connection.

Browser WebSocket does not provide a pre-buffer receive cap. The client rejects oversized delivered packets, but the production server must impose its own pre-buffer limits, admission expiry/replay policy, rate limits and durable authority. The loopback test service is not that production service.

## Verification interpretation

Local native workspace: 1,397 passed, zero failed, five existing optional source/corpus gates ignored. This includes 10 authority-admission tests and 11 existing native-player tests. The 10 browser-socket tests and 59 audio tests are separate Node suites. Formatting and strict native and browser-target lint pass. 5,384 raw original `TSOClient/` and `Other/` blobs match the recovered base. Cargo.lock changes only the web-shell's dependency on the existing game-runtime package, not dependency versions.

The original missing-module failure is retained separately from the compiling passthrough control: all five initial admission regressions failed behaviorally before implementation and passed afterward. Extended coverage checks dynamic original menus, future/expired observations, exact cancellation and stale identity. The controlled browser server inserts three ordinary simulation ticks between receipt of each action and source revalidation, rather than relying on timing luck.

The optimized, actually built application passed the native browser journey locally, using Chromium 141.0.7390.37 and Playwright 1.56.1. Ten scenario records cover admission; an advancing native tick; the unchanged-source needs action despite three intervening server ticks; the same accepted needs update in an independent browser context; unknown-result retention without automatic retry; four viewport sizes; socket disposal; and console/framework health. Seven unedited screenshots are retained. The scenario records are grouped checks, not a substitute unit-test count.

The action changed Energy, Hunger and Hygiene from 50 to 100 after authoritative execution. The canvas element remained mounted. Three admissions/checkpoints and four explicitly requested actions were observed, including one deliberately dropped decision. That decision remained Unknown after recovery and was not resent. Two browser contexts use the same test Sim; neither the test nor this record establishes distinct production identities or a two-player capacity cap.

At 1440 x 1000, 390 x 844, 320 x 600 and 844 x 390, the test checks the actual header, toolbar, action/needs panels and each of the 13 toolbar buttons. Buttons have labels, remain inside the toolbar, retain at least a 30-pixel hit region in each dimension and do not overlap. Camera and floor buttons are 36 x 36 at all four tested sizes. No page/console errors were recorded. Existing ignored-preload-integrity warnings and the native fixture's local TERM warning remain in the raw evidence; they are not hidden or counted as application failures.

The stronger controls test was first run against the initial CI-built candidate. It failed on a zoom button only 22.59375 pixels wide at 390 pixels. The fix reuses the existing named source-toolbar grid regions and labelled circular controls, without changing shared styles or other viewports. It passes against rebuilt source without injected CSS. Screenshots depict the deliberately limited native fixture geometry, not a complete furnished or animated original lot.

The first hosted candidate run 37552368078 also passed native tests/lints, the actual release build and the initial browser journey. Its subsequent source-packaging step failed; that overall run is not labelled green. Temporary publication workflows are removed from the final tree. The permanent read-only `Native browser` workflow now builds and tests ordinary tracked source without assembly scripts, and uses the pinned npm lockfile. Hosted results for the final published revision are tracked separately on its PR.

### Reproduce the built-browser journey

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
(cd apps/web-shell && trunk build --release --locked)
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install chromium
node tools/native-browser/verify.mjs
```

Use the repository's Rust 1.99.0 and Trunk 0.21.14. The default Trunk Binaryen optimizer is version 123; wasm-bindgen is 0.2.129 from Cargo.lock. The runner owns loopback ports 18787/18888 and its controlled processes. `WONDERLAND_NATIVE_QA_DIST` selects an already-built distribution; `WONDERLAND_NATIVE_QA_OUTPUT` selects the evidence directory. It never opens historical user saves or contacts real players.

The adjacent machine record binds code inputs, the local WASM artifact, commands, logs, browser assertions and screenshot hashes. Native audio tests do not establish synchronized active-lot sound. There was no independent reviewer, real server, physical device or screen-reader qualification in this correction.

## Boundaries that are unchanged

The test service uses the original controlled gateway plus a declared native Rust authority fixture containing unchanged source BHAV bytes. Its two browser contexts intentionally control the same test Sim. This verifies independent clients following one admitted actor, not distinct real player accounts, capacity, production authentication or durable multiplayer ownership.

Native `WLB1/WLR1/WLC1/WLA1` and original `FSOv/VMNet` remain different protocols. The original-source lane stays refresh-only; this continuation does not translate arbitrary legacy saves or commands into native state. Native appearance/content must be supplied by the authority. The controlled lot has limited fixture geometry and missing original avatar/object models; it does not qualify full walking/contact animation, original asset coverage or physical-device performance.

Full production native admission/stream service, complete original content, direct movement and object-family qualification, Build/Buy/property, remaining social/object dialogs, accepted gameplay audio, legacy translation and deployed real-user acceptance remain separate work. No merge or deployment is part of this handoff.
