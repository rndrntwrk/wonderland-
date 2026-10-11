# Publication and browser acceptance of accepted-tick pose retention

## Source delivery

PR #39 publishes the conversation's approved `Wonderland-Native-Pose-Retention.patch` against PR #34 at `13580dd2d0e2fefbefb73e4d0011032f80c06fb0`. The complete patch SHA-256 is `8d4a362810418e5b31a0a50e3845e9e6e1864894beeae621f27a147c5b6dcea7`.

The source-transfer job verified that hash, all seven modified-file preimages, and all fourteen resulting Git blob identities. Commit `e32648e2592f9337e397478b4d7691982e04b706`, tree `70070b48baa12dc929dcde29cfeaaa2978a192d8`, contains the exact approved patch as ordinary source. Its temporary transfer workflow and patch fragments are absent from that tree. The transfer stored verified source objects; it did not execute tests or move any branch. Normal builds do not run an assembly or source-rewriting step.

`native-avatar-pose-retention.md` is the **original local handoff record** preserved byte-for-byte with the approved patch. Its historical statements that the patch was unpublished and had no hosted/browser execution describe that earlier record, not the current PR status. Fresh hosted results belong to the exact subsequent head and are linked in PR #39; no future pass is presumed here.

## Relationship to other work

PR #37 independently implements pose retention at observed presentation samples. PR #39 additionally carries **every intermediate accepted tick** through the real replica, wire and player before the renderer can coalesce wakes. These are overlapping alternatives on the same PR #34 base, not safe independent merge candidates. Do not merge both implementations without resolving their native-avatar/history/native-lot overlap. Neither branch or its review state is changed by this publication.

To avoid duplicating the browser test, this PR reuses #37's test-only slow one-shot fixture behavior and its unchanged `tools/native-browser/verify-pose-retention.mjs` from `194e998d021c5ce894753222a57c8518162ffc32` (blob `59178c86cd7421314cd4c20f7d6507e01d54f257`). It does not import #37's product implementation. Terrain work in #35 and gameplay audio work in #38 remain separate; this PR does not claim or overwrite them.

## Additional actual-browser witness

`verify-pose-batches.mjs` uses the actual optimized application, the existing controlled original-service gateway, the real native runtime and the normal content-import/Sim/map/Visit path. It starts fresh isolated sessions for packet sizes **1, 3 and 18**. A bounded test-only Rust operation generates the real accepted ticks and encodes each batch through the production binary encoder. The JavaScript fixture forwards those bytes unchanged. It does not synthesize avatar transforms, reinterpret the protocol, change browser time, inject CSS or replace the compiled application.

Manual tick delivery is an explicit fixture option, not a production control. With the fast nonlooping test animation, all eighteen accepted ticks—including its complete active interval and ended state—can arrive in a **single packet**. Retained final canvas pixels must differ from the initial bind pose and match the separate one-tick and three-tick runs. Subsequent accepted idle batches must not drift the pose. The authority state hash must also agree, keeping presentation changes separate from simulation outcomes.

The witness additionally checks ordinary camera rotation and return, a real source action and correlated acknowledgement after the clip ends, frozen pixels on disconnect, documented bind-pose reseeding at a new checkpoint without historical bones, no automatic action retry, and socket/viewport teardown. The slower inherited witness checks the rendered pose before and after EndReached, mobile layout and recovery. Existing action/cancellation/receipt-deadline/avatar-resource journeys remain enabled.

The fixtures remain synthetic geometry encoded in original Vitaboy formats, with explicitly declared native setup and selected unchanged source behavior bytecode. This is not original human-Sim artwork or full original-content qualification. The fixtures are never enabled by ordinary startup and are not deployed services.

## Gates and evidence

The read-only `Native pose retention` workflow executes the focused regressions and standalone native/WASM conformance. The read-only `Native browser` workflow compiles the actual release, runs the existing journeys plus both new retention witnesses, and retains full JSON reports, console errors, screenshots, code hashes and compiled WASM identities. Its final `git diff --exit-code` must remain clean. The independent `Browser UI`, `Native live wire` and `Live lot session` workflows retain their coverage.

Build and run from repository root with the pinned toolchain and locked dependencies:

```sh
cargo run -p wonderland-web-shell --example native_avatar_resources --locked -- target/native-avatar-fixtures
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
(cd apps/web-shell && trunk build --release --locked)
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install --with-deps chromium
node tools/native-browser/verify-pose-retention.mjs
node tools/native-browser/verify-pose-batches.mjs
```

Use a separate `WONDERLAND_NATIVE_QA_OUTPUT` directory for each witness. JSON reports and screenshots are evidence of their named execution; they do not constitute independent code review or physical-device inspection. Only completed jobs against the final head establish hosted acceptance.

## Unchanged limitations

Native checkpoints do not serialize historical bone transforms, so recovery deliberately resets unavailable history. Missing content/rig/container attachments remain diagnosed. Head seeking, source-complete contact/attachment and rig/outfit support, original human art, production distinct-player services, durable reconciliation, native gameplay audio and full Buy/Build/social/object interfaces remain unfinished or owned by separate integrations. No merge, deployment, shared-style redesign, source-asset edit, dependency version change or claim of complete game parity is included.
