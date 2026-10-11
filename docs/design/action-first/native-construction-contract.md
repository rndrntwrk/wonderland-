# Native construction quotation and admission

Base: PR #49, `3d645334e85e9acf644e7955b13b611cd8d4a9f4`.

This increment connects restricted player selections to the existing native
Build engine. It is an authority library, **not an enabled browser Buy/Build
surface, authenticated construction service, or durable accounting provider**.
The native button stays disabled until those connections are supplied. No preview
Home save, original VMNet authoring path, artwork or dependency version is changed.

## Public boundary

`wonderland_game_runtime::live_wire::construction` accepts floors, cardinal solid
walls, terrain vertices, source-catalogue purchases, and moves/removal of existing
non-avatar objects. A removal is a native object deletion, not inventory storage,
a resale interface or a promised refund. Diagonal/half-floor wall tools, portals,
roofing and complete original Build families are not exposed by this boundary.

`ConstructionRequest` contains the admitted player/lot identity, relevant world,
permission, account and catalogue revisions, a client correlation ID, and user
selections. It contains **no price, payer, permission grant, object footprint,
reserved entity ID, durable receipt or accepted completion**.

`ConstructionGrant` must be constructed by an authenticated server from current
session, ownership, account, catalogue, permission and material providers. Its
lack of a deserializer is an API safeguard, not authentication. Do not populate it
from fields in a browser request. The initial contract supports self-funded
construction: the payer/owner must match the admitted avatar. A trusted grant can
permit managing others' objects through the existing native permission rules;
shared budgets, gifts and alternate payers are not modelled here.

`quote(runtime, grant, operation, request)` reads the authority runtime and derives
an immutable native `BuildPreview`. `operation` is a server-allocated durable
journal identity, distinct from the client's request ID. Purchase references are
previewed using a copied native allocator; the live allocator is not reserved or
mutated. Source catalogue definitions replace provisional object rules/geometry,
and trusted native prices determine the quote. Existing occupied wall bits are
preserved, never accepted as user-editable input. Unsupported wall conversions
are rejected rather than silently discarding their state.

`ConstructionQuote` has private fields, binds the original authenticated principal,
and must be retained server-side. Its
`consent()` exposes the request ID, preview hash and quoted cost to be reviewed by
the player. The UI must ask for explicit confirmation; calling this accessor is
not evidence of human consent.

`confirm(runtime, current_grant, stored_quote, consent)` checks exact consent,
revalidates current admission and source content, and recreates the native quote.
Any relevant changed revision, allocation, material, geometry, permission, price
or budget must still validate and produce the same quote. Ordinary simulation
ticks alone do not expire an otherwise unchanged offer. The result is only
`AcceptedCommand::BeginBuild`, never `CompleteBuild`.

## Server integration order

1. Authenticate the native lot session and obtain trusted provider values. Read
   the binary request with `decode_request`, not through lossy JavaScript numbers.
2. On the serialized authority executor, bind the current grant, allocate a unique
   durable operation ID, calculate the quote, and store it under this admitted
   session and correlation ID. Show its exact price/hash and reject unavailable
   source material rather than substituting a demo catalogue.
3. Following explicit user confirmation, obtain fresh provider values and call
   `confirm` on the serialized authority turn immediately before submitting its
   returned `BeginBuild`. Do not allow another mutation between validation and
   admission. The native engine independently checks the submitted preview.
4. Process the resulting `RuntimeEvent::BuildRequested` through the durable
   accounting/inventory provider. That provider must check account revisions,
   enforce operation idempotency, atomically persist charges/ownership, and return
   the actual receipt and persistent IDs. These guarantees are external to this
   module and are **not** supplied by an in-memory test receipt.
5. Only a trusted server completion handler may submit `CompleteBuild` with that
   exact receipt. The existing engine distinguishes application, rejection and
   reconciliation. Do not report a paid operation as finished merely because
   BeginBuild or a debit was accepted. A construction conflict after durable
   commit may require reconciliation rather than another charge.
6. On disconnect or an unknown result, consult the operation journal; do not
   allocate a new operation and blindly retry. The request ID alone is neither a
   receipt nor a globally unique money-operation identity. Publish final accepted
   state and a correlated user result only when the actual outcome is known.

Quotes and confirmation checks do not themselves modify a balance, reserve
inventory, send a network message, persist anything or execute a tick. The caller
must implement the authenticated transport, bounded quote store, catalogue
presentation, quote expiry policy, durable journal/reconciliation and player UI.
This module does not claim those missing pieces are finished.

## Binary request

The independent version-one request starts with eight bytes `WLCB\x01\r\n\x1a`,
a little-endian u64 exact payload length, and the pinned fixed-integer,
little-endian bincode request structure. Existing WLR1, WLC1 and WLA1 packets are
unchanged. Unknown versions, trailing bytes, zero identities, empty operations
and more than the native 1,024-edit ceiling reject. Full-width integer IDs survive
round-trip, including values beyond JavaScript's precise Number range.

The parser caps input at 128 KiB and uses the existing guarded decoder with
131,072 item visits, 8 MiB conservative allocation credit and depth 64. It compares
canonical serialization against original bytes without a second full copy.
These are decode-admission limits, not whole-process memory guarantees. The
server/transport must enforce message caps **before** buffering and rate-limit
requests. Confirmation transport and presentation of server-generated catalogues
are not specified as additional message families in this increment.

## Reproduction and evidence scope

```sh
cargo test -p wonderland-game-runtime --test construction_contract --locked
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
```

Tests run the real native geometry, source-quote, accepted-tick and completion
engine. Authority grants, prices, floor availability, simple catalogue-object
metadata and completion receipts are explicitly authored fixtures; they are not
production account authentication or actual persistent payments. They cover
multi-purchase identity allocation, actual entity/thread creation, move/delete,
rejection and duplicate completion, consent mismatch, stale source/revisions,
revoked authority, resource withdrawal, invalid geometry and bounded codecs.
Original BHAV fixture bytes are unchanged.

The optional original-corpus/reference tests and browser/server qualification
remain distinct from passing this native suite. This is not original asset,
physical-device, full-game or deployed multiplayer acceptance.
