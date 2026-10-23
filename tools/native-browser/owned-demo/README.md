# Account-owned native property integration runner

## Status and required source input

This package connects the real account authority, controlled lot coordinator,
private Rust host and Rust/WASM browser UI on one loopback origin. It is an
integration runner, not a deployed service or a production-ready game.

**No qualified original-content manifest is currently included.** The original
source join needs global routines 271/280 and template-person GUID `7FD96B54`;
the avatar presentation closure also needs the original adult skeleton and outfit
resources. Their exact lawful import path, provenance and cooker recipe are in
[the original-content report](../../../docs/verification/native-playable-content-20261015/README.md).
The runner never substitutes a fixed actor, generic spawn, authored appearance,
or successful fixture action for those missing resources. The eventual game
scope remains the original full game.

Startup refuses missing pinned appearance or uninitialized source join metadata.
`admission_ready` proves metadata presence only; native admission still checks
source callback/content qualification and may refuse a candidate. Successful
account, provider or authored-host tests are not evidence of an original-source
playable browser journey.

## Build and run

Use the repository's pinned Rust toolchain with the `wasm32-unknown-unknown`
target, `wasm-bindgen 0.2.129`, Trunk, and Node/npm available on PATH. From a clean
checkout, install the locked edge dependencies and generate the account crypto
WASM before starting the runner:

```sh
npm ci --prefix services/native-edge
bash services/native-edge/tools/build-account-crypto.sh
```

The crypto script accepts `WASM_BINDGEN=/absolute/path/to/wasm-bindgen` if needed.
It verifies the exact bindgen version. `services/native-edge/generated`,
`node_modules`, `.wrangler`, test outputs and original/private runtime artifacts
are generated or local state, not files to commit. The runner loads Miniflare,
esbuild and ws from the locked edge dependency tree.

Build the private host and WASM app:

```sh
cargo build --locked --release -p wonderland-native-lot-host
node apps/web-shell/scripts/build.mjs
```

The default private host path is `target/release/wonderland-native-lot-host`.
The release profile retains overflow checks. A development host can be selected
explicitly with `--host`, but its throughput requires separate measurement.

After importing and qualifying the original content, supply a private manifest:

```sh
node tools/native-browser/owned-demo/run.mjs \
  --manifest /absolute/path/to/qualified-source-manifest.json \
  --dist apps/web-shell/dist \
  --storage /absolute/path/to/private-demo-state \
  --port 4173
```

The manifest identifies actual prepared artifacts. It must contain:

- `authority_id`: trusted account-authority SHA-256 identity
- `content_release`: SHA-256 of the pinned cooked release binding
- `protocol_release`: trusted protocol-release SHA-256
- `host_config`: absolute path to the native-host configuration
- `lot`: `{lot_id, lot_location, name}`; the canonical decimal-text database lot
  ID and positive u32 packed source location are independent identities (for
  example database ID `41`, source location `16777472`)
- `catalog`: real source head/body selection catalogue, with matching release
- `avatar_specs`: exactly two distinct account creation selections from that
  catalogue, each with `name`, `description`, `gender`, `skin`, `head_key`,
  `body_key` and matching `release`

The native-host configuration must reference the pinned cooked release, lot,
paired `appearance_file`/`appearance_sha256`, the original imported avatar
archetype GUID (`7FD96B54`), and source-derived initialized
checkpoint/import. This runner does not create a source startup snapshot from
supervisor commands. Do not publish private original assets or generated account
credentials. The CLI prints two temporary loopback test-account logins once.

Open the printed loopback origin in two separate browser contexts. Sign in,
choose **Play as {Sim}**, then **Enter {property}**. The existing native player
UI supplies source object **Invoke** and **Cancel**, accepted state rendering,
and **Reconnect**. Enter does not fabricate a legacy `LotReady` session.

## Programmatic contract

```js
const demo = await createOwnedDemo({
  dist,
  manifest,
  hostExecutable,
  storageRoot,
  port,
});
const accounts = await demo.setupAccounts();
const durable = await demo.inspectDurable();
await demo.restartHost();
await demo.close();
```

Import `createOwnedDemo` from `tools/native-browser/owned-demo.mjs`.
`setupAccounts()` returns exactly two
`{username,password,name,avatar_id,lot_name}` test records. IDs are allocated by
real account creation. `inspectDurable()` compares the authoritative cursor and
registry and returns actors, cursor, registry hash, journal and pending outbox.
`restartHost()` sends SIGKILL, obtains the V2 durable recovery grant/checkpoint/
accepted tail, starts a replacement candidate, checks its actor registry and
permission binding, commits the takeover marker, then activates it. It takes no
new snapshot at crash time.

An ingress restart can use the same manifest and original storage directory:

```sh
node tools/native-browser/owned-demo/run.mjs \
  --manifest /absolute/path/to/qualified-source-manifest.json \
  --dist apps/web-shell/dist \
  --storage /absolute/path/to/private-demo-state --resume true
```

This restores the saved loopback origin and uses V2 takeover. Existing tabs retain
their memory-only session token and reconnect with the original admission
operation. The adapter retrieves the frozen export through an authenticated
account read, then verifies its committed receipt against the restored actor
registry. It never reapplies portable state. A new account login/session cannot
adopt a previous session's custody. No passwords or bearer tokens are saved in
runner metadata, and resume never recreates the demo accounts.

## Bounds and incomplete lifecycle

- The whole reserve/export/install/prepare/commit chain, actions, autonomous
  ticks and logout/fence drain use one bounded lot queue.
- Only committed actor grants produce one-use 15-second WebSocket tickets.
  Browser transport accepts no supervisor commands. Private workerd RPC requires
  a per-run memory-only capability and rejects Origin and non-JSON requests.
- A 20-second authenticated browser-presence heartbeat can renew the 60-second
  custody lease when less than 30 seconds remain. Presence never extends the
  account's original idle/absolute deadline. Real actions and manual reconnect
  may refresh account activity. Server ticks cannot renew custody by themselves.
- Disconnection/expiry/logout retains custody and fences player actions and
  ticks. Leave remains visibly pending until source save/detach has a qualified
  receipt; this implementation does not claim that lifecycle is complete.
- Checkpoints now compact the hot journal into verified, content-addressed R2
  archives while preserving exact durable batch decisions and outbox records.
  After checkpoint publication, the runner separately persists full native
  terminal receipts, including original request hashes and action correlations,
  before acknowledging their bounded in-memory retirement. A retired ID can only
  retrieve its exact durable record; missing/mismatched history fails closed.
  Normal checkpoints occur every 32 ticks, with earlier transfer at 32 retained
  receipts or 8 MiB. Failed checkpoint/receipt publication leaves local receipts
  intact. Recovery adopts a new incarnation before any new batch is prepared.
  The 4096-entry limits are hot-storage bounds, not a fixed 136-second session
  cutoff. Bounded-component tests alone are not sustained original-content play.
- New player-action proposals bind the original WLC1 bytes and both native WLA1
  decision receipts into the proposal digest. The coordinator stores the chosen
  outcome atomically with the batch decision, before host resolve/export. A
  same-account/session/control browser can retain a memory-only pending capsule
  across a verified takeover and query that exact decision after a fresh matching
  checkpoint. This never resubmits the action or imports the old runtime. Normal
  authorization still records/drains expiry and fence obligations; lookup does
  not execute game actions or renew custody/activity. Older batches without
  evidence, lost browser memory, changed ownership/session or missing history
  remain Unknown rather than inferring success. Acceptance does not mean that
  the source interaction completed. See the
  [recovery contract](../../../docs/verification/native-action-outcome-20261015/CONTRACT.md)
  for exact qualification and remaining lifecycle work.
- Outbox effects are retained, not a claim that every original-game external
  effect consumer exists. Source-content completeness, lifecycle completion,
  and deployment remain separate requirements.

## Verification

```sh
node --test apps/web-shell/scripts/native-owned-account.test.mjs \
  apps/web-shell/scripts/native-socket.test.mjs \
  tools/native-browser/owned-demo/actions.test.mjs \
  tools/native-browser/owned-demo/receipts.test.mjs \
  tools/native-browser/owned-demo/manifest.test.mjs \
  tools/native-browser/owned-demo/recovery.test.mjs \
  tools/native-browser/owned-demo/contracts.test.mjs
```

The workerd contracts exercise actual account revocation, durable fence draining,
private-RPC isolation and denial without journal/outbox/cursor mutation. Provider
and transport tests cover late replies, own-avatar binding, incarnation recovery,
presence teardown and correlated rejection delivery. These are bounded component
checks, not a rendered source-game pass.

The actual browser acceptance helper is `tools/native-browser/verify-owned.mjs`.
It requires original-object UI hooks, two real accounts with distinct decoded
actors, both actors rendered, a shared visible original-object change, and that
same change after host loss/recovery. It must fail rather than substituting a
fixture actor or ignoring unavailable original content.

### Placement and private authored setup

Committed admission is distinct from playable in-world placement. Enter and
reconnect return `admitted_awaiting_placement` without a ticket when the exact
committed actor is out of world. Custody and the same-control action recovery
capsule remain retained. The mounted player is suspended until placement is ready;
late grants/frames cannot reopen transport during the pause. Actual leave or
identity invalidation still closes that control.

An optional trusted Node factory `authorGenesis` hook supports a private authored
object fixture tied to the first genuinely created account/avatar. It is not an
HTTP API or a purchase/roommate/object-creation implementation. It requires a fresh
empty store and is forbidden on resume. Only frozen manifest pins and a verified
credential-free account creation receipt reach the callback. Its four-field result
pins a new host config and ownership receipt; all other manifest fields are
unchanged. The reopened source host must have zero owned actors/placements, an
unchanged empty registry and unchanged lot permissions before coordinator setup.
Final manifest/receipt pins are persisted, and ordinary handlers stay unavailable
until initialization completes. Actual source exporter and object gameplay must
be qualified separately before using this optional fixture.

Miniflare uses `cf: false`: this private runner does not need or fetch remote
Request.cf metadata. Account/lot RPC stays in-process, and browser HTTP/WebSocket
transport binds loopback. Supervisor inspection methods are never routed to the
browser.
