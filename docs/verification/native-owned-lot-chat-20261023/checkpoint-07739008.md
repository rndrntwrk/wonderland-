# Native-owned public lot chat

Two actual authenticated browser players now send and read public lot chat while remaining bound to their source-owned actors. The panel reuses the existing chat chrome, message fields and original 200 UTF-16-unit convention. It is a native membership-backed service lane; it does not fabricate VM Chat commands, speech bubbles, simulation ticks or action receipts.

Tested local source is `07739008063e2a6bf78a1b6dd294d411894cbbde`, tree `ee00e6bbb45a10ea3dc645ba3f0fd9b3e732d930`. It incorporates `63dba3e8` (implementation) and `4787e2b1` (release copy entry). Publication is stacked on draft #82 at `810b3eceb84bab91125f8aa86ccf173fce0cdc5e`. Public commit identities differ because private local history is excluded; later test/document-only changes do not alter compiled product inputs.

## Behavior and authority

Every read/send uses the existing same-origin authenticated account session, selected lot/avatar, admission operation and current incarnation. The supervisor freshly validates the reserved account and native in-world placement. Inside the same SQLite transaction as the chat operation, the coordinator checks installed ownership, active control, expiry, fences, lot and incarnation. The installed account export supplies attribution and the bound source ignore list. A browser cannot supply a sender name, principal or authority.

The room retains its latest 100 public messages. Each avatar has a monotonic sender sequence and independent last receipt: an exact retry returns the same durable identity even after history pruning; changed bytes, stale or skipped sequences reject. Identical text under a new next sequence is a distinct intentional message. Persisting the message, sender receipt and history pruning is atomic before acknowledgement. Read polling does not renew activity or publish a simulation tick; only an authenticated send uses the existing activity renewal.

The panel suspends polling and sends when hidden/disconnected, discards late lifecycle responses, and hides history when current authorization cannot be confirmed. An ambiguous send retains its exact bytes and offers explicit retry; authoritative history can also reconcile its durable receipt. Text renders through textContent, never HTML. The panel supports scrolling and desktop/mobile sending.

## Actual qualification

- 36 JavaScript creator/owned-account/chat regressions pass, including lost replies, correlation, lifecycle invalidation and input limits.
- Five real SQLite check groups pass: duplicate conflict, 100-message retention, receipt survival after pruning, separate rooms, atomic rollback, cold store restart and source ignore filtering.
- Seven real native/workerd/SQLite HTTP groups pass: two source-placed players; exact retry/intentional repeats; input boundaries; account/avatar/lot/incarnation/origin isolation; native recovery; reverse delivery and revoked-account denial. These ran at `63dba3e8`; only index.html and connected.css differ from the rendered source.
- Eleven focused public-policy/private-RPC routing cases pass.
- Targeted WASM check and pinned offline release packaging pass. All 151 release artifacts were verified. The original native host artifact was reused because its inputs are unchanged.
- Fresh sandbox-enabled Chromium passes six rendered groups on the exact matching source: two-way escaped text, actual committed send with deliberately dropped reply followed by identical retry/one message, fresh socket reconnect/history, native host recovery with both current bindings, mobile sending and sign-out cleanup. Desktop 1200×800 and mobile 390×844 images were visually reviewed. Retry visibility, feedback spacing and toolbar separation are asserted.

The rendered run has no page exceptions and 22 successful chat responses, all HTTP 200. Two ERR_FAILED console entries are intentional fault injection (dropped send reply and read); two preload-integrity warnings remain. Four content fetches were canceled during authenticated lifecycle changes. These are recorded rather than reported as a zero-network-error run.

Failed attempts remain intact. The first browser package omitted the new module from Trunk's explicit copy list. The next run passed functionality but visual review found an author CSS override of hidden retry and a fourth mobile action overflowing the toolbar. The exact final run includes scoped corrections and explicit geometry assertions. A static artifact reuse attempt was rejected by its own proof because CSS is Trunk-processed; final packaging used the pinned pipeline with cached Rust outputs.

## Boundaries and reproduction

Run the focused JavaScript tests with `node --test apps/web-shell/scripts/native-chat.test.mjs apps/web-shell/scripts/native-creation.test.mjs apps/web-shell/scripts/native-owned-account.test.mjs`. From services/native-edge, run `node tests/lot-chat-store.mjs` and `node --experimental-strip-types --test tests/preview-policy.test.mjs tests/supervisor-rpc.test.mjs`. Original content/native fixtures are private and required separately for rendered qualification.

This closes the bounded public lot conversation path. It does not close mail, private messaging/channels, moderation/slash commands, original VM speech bubbles, live ignore editing, full social systems, hosted gameplay or complete-game parity. One-second reads are not a latency/performance qualification; historical 30 Hz results are unchanged. Creator and movement implementations remain incorporated; Buy/Build remains disabled.

Baseline TS7016 for the generated account crypto declaration and the existing proc-macro future warning remain. A tsc invocation without allowImportingTsExtensions also reports existing TS5097 imports; no full type-check/format pass is claimed. Review is by the sole implementer under the one-task instruction.

See the [sanitized evidence manifest](checkpoint-07739008-evidence.json) and [current game inventory](../../integration/CURRENT-FULL-GAME-GAPS-20261023.md). Private screenshots, stores, original assets, credentials, raw logs and local history are retained privately, not uploaded or presented as public downloads. No new grants, installation, paid services, deployment or merge occurred.
