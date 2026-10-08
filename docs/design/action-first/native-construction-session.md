# Native construction session orchestration

Base: PR #51 at `9eba8e5cdb0d3cf36be5e3888957415010ac47b2`.

This is the server session between the existing pure construction quote contract
and the native accepted-tick engine. It does not enable a browser Build button,
create an authenticated service, or provide a durable money/inventory database.
The application and existing wire packet formats are unchanged.

## Integration contract

`live_wire::construction::session::ConstructionSession` belongs to the serialized
lot executor, not to an unauthenticated client request. Construct it from the
admitted runtime and the current trusted `ConstructionGrant`. One session retains
one full quote, immutable account/Sim/lot/content identity, monotonic host time,
and a request-ID high-water mark. It is intentionally not deserializable.

| Operation | Behavior |
| --- | --- |
| `new` | Bind the authenticated principal, exact actor generation, source/lot identity and content hash. Accept a server-selected quote lifetime from 1 to 300,000 ms. |
| `offer` | Reuse the real pure quote engine. Return the operation, exact consent hash/price and fixed deadline. Reject a second live quote, conflicting duplicate IDs, revoked grants and withdrawn materials. Exact repeats revalidate but do not extend the deadline. |
| `cancel` | Cancel only the exact unsubmitted request/operation. An old cancel cannot cancel its successor or undo an admitted transaction. |
| `submit` | Following explicit user consent, revalidate and apply the actual BeginBuild tick under one mutable runtime borrow. Return its accepted `TickFrame` and `TickOutcome` once. Exact repeat confirmation returns `None`, never a new tick or durable request. |
| `refresh` | Read pending/completed/rejected/reconciliation data from actual native BuildState. Match operation, quote hash and cost. Missing state stays Unknown; a mismatched outcome is an error. |
| `disconnect` | Cancel unsubmitted quotes; retain admitted operation identity as Unknown. No debit, cancellation, retry or success is inferred from socket loss. |
| `close` | End this RAM lifetime. This does not cancel an external durable operation or release its accounting obligations. |

Expiry uses the host's monotonic clock, not simulation ticks, browser timestamps
or a wall clock that can jump backwards. Equality with the deadline is expired.
Clock regression, deadline overflow and regressed runtime ticks reject. A pending
operation does not expire merely because its original quotation deadline passes.

`Phase::Outcome` contains the exact native `BuildCommitStatus`; it is not a boolean
success. Only Committed or Rejected permits replacement with a new quote.
NeedsReconciliation continues to block this session. It must be reconciled in the
operator's durable journal; this library has no setter that falsely clears it.
Loss of permission to create NEW builds does not erase an existing result, provided
the original admitted account/actor identity is still valid.

## Authority and durability

The caller allocates `operation` using its durable journal, never using the browser
request ID as a global financial identity. Bind transport to the server's session
and supply fresh trusted account/catalogue/permission providers on each operation.
Do not reconstruct a grant from a request or from `QuoteView`.

After `submit` returns `Some(admitted)`, publish its frame through the authenticated
ordered tick stream and deliver the real BuildRequested event to the durable
provider. The existing engine does not apply the floor or create the purchased
object until the trusted provider submits its matching CompleteBuild confirmation.
The session cannot generate that confirmation and never debits a balance.

The provider must persist operation identity and request/consent correlation,
idempotently commit money and ownership, check revisions in the same durable
transaction, and reconcile conflicts. Keep the session across socket replacement
or recover equivalent correlation from that journal. **In-memory duplicate
suppression is not crash-safe exactly-once accounting.** A new process must not
instantiate an empty session and blindly resubmit an unknown operation.

A returned `None` from repeated `submit` means no new frame/effect was generated;
use `refresh` and the journal for the status. It does not mean the operation was
rejected or completed. Failed admission attempts that reached runtime execution
remain non-retryable in this session until reconciled. User confirmation is an
upstream UI requirement; a method call by itself cannot prove human consent.

## Verification

`construction_session.rs` uses the actual quote engine, authority runtime,
accepted frame replay and durable-completion engine. Its account grants, prices
and completion receipts are authored test fixtures, not production payments.

The initial compiling API skeleton failed all 17 behavioral specifications. An
author-review regression then found that NeedsReconciliation was allowing another
quote: 23 of 24 tests passed before the correction, and the specific remaining
case passed afterward. The final suite adds actual replica replay for 25 cases.
It covers consent/expiry/cancellation, duplicate submissions, source-principal
transfer, permission revocation, ID overflow, pending/rejected/committed results,
reconciliation, missing state after restoration and a reused operation with a
conflicting quote. No production price or receipt stub is introduced.

Reproduce with the repository's pinned toolchain and locked dependencies:

```sh
cargo test -p wonderland-game-runtime --test construction_session --locked
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
node --test --test-reporter=tap apps/web-shell/scripts/native-*.test.mjs
node --test --test-reporter=tap crates/audio-runtime/browser/*.test.mjs
```

## Still separate

This addition does not mount a placement/cost-review interface, define the
construction quote/consent/result transport, or implement production quote-store
persistence and journal recovery. Native Buy/Build stays disabled until those
connections and actual catalogue/material/account providers are supplied. Full
original Build families, inventory storage/selling, property permissions, distinct
real-player services, original artwork and physical-browser acceptance remain
outside these library tests. No original asset, root dependency, native packet
format, shared UI, sibling branch, parent/main branch or deployment is changed.
