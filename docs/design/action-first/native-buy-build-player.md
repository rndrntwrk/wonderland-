# Native Buy/Build player connection

Pinned parent: PR #53, `505e74e9befeb08e44784fa4f51a3e7848a412ef`,
raw tree `86b2ef453c2bfee38d6397231376e9938cc11710`.

## Delivered player flow

The actual native-lot Buy/Build button now opens a scoped catalogue and placement
panel. The panel loads the admitted account's server-supplied items, categories,
prices, thumbnail paths, permission indicator and balance. Search and eight-item
pages preserve the complete supplied cohort; the browser does not install the
controlled test catalogue as product stock. Missing providers produce an explicit
unavailable message instead of an invented catalogue or a successful purchase.

Selecting a supported item switches to map placement. Existing renderer tile
picks supply the coordinate; camera and floor controls remain available. The
preview is a **disposable tile outline**, not a full object ghost, footprint
qualification, or an optimistic addition to the simulation. Rotate item changes
the selected native cardinal direction. An actual server quote must succeed
before the exact-price confirmation is shown. The consent closure captures that
reviewed hash and cost. The existing construction client and server session are
reused without changing their wire formats or authority semantics.

Only a matching committed operation in the admitted native replica reports
completion. Pending leaves geometry unchanged. A lost confirmation response
becomes Unknown and cannot be turned into a second purchase by clicking Recheck:
that sends a status query for the original operation. A read-only catalogue/balance
refresh follows each observed commitment once. The current native session retains
operation correlation through its socket reconnect. Leaving the lot disposes the
HTTP channel; **it does not reverse a debit or resolve a durable obligation**.

Supported selections in this interface are catalogue object purchase/placement
and one-tile floor painting. The underlying authority still checks actual content,
footprints, entity allocation, permissions, price/revisions and trusted completion.
Walls, terrain editing, storage, selling, donation, upgrades, all remaining Build
families and full inventory/property management are not enabled by this change.

The panel uses the existing game chrome. It reserves visible placement space
rather than covering the map with the open drawer. Needs and Actions remain
mutually exclusive with the construction inspector. On short mobile displays the
panel scrolls internally; all quoted-price controls remain reachable. This is not
a new game identity, a form-only replacement, or a removal of original surfaces.

## Product modules and host contract

- `native_build.rs`: bounded catalogue admission, exact item selection, pages,
  current-scene tile checks, rotation and construction request preparation.
- `native_build_browser.rs`: Leptos panel, generation-owned transport lifecycle,
  existing exchange Client consumption and replica confirmation.
- `connected_bridge.rs`: the existing authenticated gateway connection supplies
  the catalogue and derives the construction endpoint from trusted configuration.
- `native-construction.mjs`: the existing byte-only Fetch helper now additionally
  accepts the gateway's memory-only bearer. It retains anti-CSRF, redirect refusal,
  byte/read/deadline limits, synchronous WASM byte copying and no retries.
- `native_lot.rs` and the Trunk entrypoint: real application mounting; no runtime
  source generator or separate replacement browser is required.

The operator's native service must implement these routes relative to the existing
configured gateway base:

### POST `/v1/native-lot/construction/catalogue`

Read-only, authenticated, no-store catalogue admission. The existing bearer
header identifies the account. The JSON request contains `source_epoch` and
`lot_incarnation` as decimal strings, plus u32 `lot_location` and `avatar_id`.
The server must compare them with the actual admitted account/Sim/lot. These
fields are correlation hints, not permission grants.

Response fields (strict JSON object; schema `version: 1`):

| Field | Meaning |
| --- | --- |
| `binding` | Exact PlayerBinding: source_epoch/lot_incarnation u64, lot_location/avatar_id u32. |
| `lot_id`, `authority_epoch`, `content_hash` | Actual native lot and 32-byte content identity matching the admitted scene. |
| `permissions_revision`, `account_revision`, `catalog_revision` | Current trusted provider revisions, carried into the quote request. |
| `balance`, `can_build` | Current account balance and whether this account may make new edits. Not a grant accepted by the server. |
| `next_request_id` | Next safe nonzero request ID for this server-owned construction session. |
| `csrf_token` | Server-issued ASCII header token, kept only for this transport lifetime. It is redacted from Catalogue's Debug output. |
| `items` | Bounded array of exact supported item records below. |

Each item contains `kind`, `label`, `category`, `price`, and nullable `thumbnail`.
`kind` is `{"type":"object","catalog_id":123}` or
`{"type":"floor","pattern":1}`. IDs above are schema examples, not a product
catalogue. The server provides actual content-derived IDs and prices.

The response is capped at 1 MiB, 8,192 items, unique kind/ID keys, 160-byte labels,
80-byte categories and a 4,096-byte printable anti-CSRF token. Thumbnail paths
are limited to 512 bytes and safe root-relative alphanumeric/slash/dash/underscore/
dot paths without dot segments, query, fragments or protocol-relative authority.
They address the application origin; deployment must serve the authorized images
there. No cross-origin URL is accepted from a catalogue. This does not claim that
HTML image fetching independently prevents a same-origin server from redirecting;
serve these paths without redirects and apply the application's content policy.

Response bytes are parsed directly by Rust serde_json. Full-width identities are
not routed through JavaScript Number. The generation and actual gateway admission
are checked before loading and again after asynchronous admission/import steps.

### POST `/v1/native-lot/construction`

Use the existing #53 binary call and reply MIME types:
`application/vnd.wonderland.construction-call` and
`application/vnd.wonderland.construction-reply`. The browser sends the memory-only
`Authorization: Bearer ...` and server-issued `X-Wonderland-CSRF` header, never a
credential URL. The server independently enforces authentication, Origin/CSRF,
current grants, pre-buffer message limits, rate limits and serialized execution.
Client-side limits do not provide these server controls.

Dispatch through ConstructionSession with an operation reserved in the **durable**
provider. Persist exact request/consent/operation correlation, atomically settle
accounting/ownership, and feed actual trusted completions into the lot executor.
Broadcast accepted TickFrames through the existing ordered native stream. An HTTP
response is not a substitute for that replication. On process restart recover the
same operation/journal; missing RAM is never proof it did not execute.

### Provider availability

No production authentication route, database, accounting journal, catalogue/content
loader, thumbnail deployment or durable settlement service is manufactured here.
Without the routes/data the UI shows unavailable. The controlled peer below is
explicitly a test harness and is not imported by any production startup path.

## Corrected original binary model admission

The first actual construction browser test exposed an older bootstrap defect:
`ModelContext` and `ModelTextureSelector` could serialize their internally tagged
records, but their derived deserializers required `deserialize_any`, which the
positional bincode reader does not implement. Empty-model earlier fixtures hid it.

The new private `model_binary` visitors read the **existing string-tagged layout**
with exact per-variant fields. Serialization is unchanged. JSON keeps the original
map/tag representation. Unknown tags, truncated records and wrong tag families
reject. Consecutive record tests check that no later record is consumed. Existing
outer native byte/work/depth/canonical checks remain in force. No alternate packet
version, new numeric discriminant or PR #50 renderer implementation is imported.
Other serialization formats are not qualified by this JSON/bincode correction.

## Execution evidence and reproduction

Use the repository's pinned toolchain and locked browser tooling. Browser-plugin
skill/runtime was absent in the execution environment, so the existing Playwright
1.56.1 / Chromium 141.0.7390.37 runner was used. Loopback services are isolated
from real accounts and saves and share fixed ports: run journeys sequentially.

```sh
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
node --test --test-reporter=tap apps/web-shell/scripts/native-*.test.mjs
node --test --test-reporter=tap crates/audio-runtime/browser/*.test.mjs
(cd apps/web-shell && trunk build --release --locked)
cmp apps/web-shell/public/native-construction.mjs apps/web-shell/dist/native-construction.mjs
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install --with-deps chromium
WONDERLAND_NATIVE_QA_OUTPUT=/tmp/wonderland-construction-qa \
  node tools/native-browser/verify-construction.mjs
```

The actual release is exercised through login, Sim selection, city map, Visit and
Buy/Build. The new journey covers catalogue categories/search/two pages/thumbnails,
real tile selection and cardinal rotation, a quote requiring explicit consent,
Pending without local geometry, committed purchase and exact selected position,
balance refresh, object presence after reconnect, floor quote cancellation, floor
commitment, lost confirmation plus lost native frames, status-only recovery without
double debit/object creation, inspector exclusion, four viewport bounds, actual
touchscreen selection/quote/cancel, and route cleanup.

The controlled catalogue has 13 authored records and primitive colored meshes;
the authority uses the actual native Build/replica engines. Prices, grants,
persistent IDs and typed completion receipts are authored test data. Its account
balance, quote reservations and completions are **RAM-only**. Persistence across
socket reconnect is tested; persistence across process restart is not. Original
human/furniture artwork, all original object behavior, production financial
transactions, independent users and physical-device acceptance are not inferred.

All inherited avatar/terrain/pose/checkpoint/audio/receipt journeys run against the
same packaged release. Historical behavioral failures are retained separately from
final passes, including the model bootstrap failure and obscured placement canvas.
The five existing optional original corpus/reference tests remain explicitly
ignored. Existing preload and future-Rust notices remain recorded. No product
source/WASM/CSS is replaced by the browser tests, and no fake timers are used.

The existing read-only Native browser workflow is extended to run this new journey
and include its result/source identities. Hosted status belongs to the eventual
published head; local execution is not a claim of hosted success, independent
review approval, parent/main merge or deployment.
