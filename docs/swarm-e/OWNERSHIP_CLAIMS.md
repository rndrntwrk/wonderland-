# Swarm E — W15.1 ownership-claims starting leaf

## Scope and contract

This is the first native persistence leaf, independent of C's renderer. It is
stacked on the existing PR18 integration baseline
`a318581f33770540808aefcf132018255a3a1d94`; it does not modify that branch, the
root Cargo workspace/lockfile, shared contracts, or any original source.

The user's work-package plan starts E at W13.1 and W15.1. This leaf implements
the acquire/renew/transfer/release and stale-writer part of W15.1. It is **not**
the complete legacy database mapping, WSS gateway, lot actor, account service,
economy, save system, or all Swarm E.

`crates/persistence` is an isolated native Rust workspace using SQLx 0.8.6 with
MySQL/MariaDB. Its local key/nonce/claim types are storage adapter values, not a
replacement version of F's shared IDs. The future authenticated lot/claim owner
maps shared IDs into these values. No browser payload may supply an owner nonce.

## Source anchor and deliberate extension

The inspected original `TSOClient/FSO.Server.Database/DA/AvatarClaims/SqlAvatarClaims.cs`
at source baseline `4c6b3e8f5835b228723caea3c9f683c62f244f73` uses previous-owner
conditional transfer and owner-checked removal. The new additive table is not a
byte/schema-equivalent port of `fso_avatar_claims`; it supplies the explicitly
planned lease/fencing extension. Original claim-to-avatar/location mappings,
legacy collations/triggers and authorized migration remain W15.1 follow-up.

## Invariants

- Unique `(claim_kind, entity_id)` rows separate lot and avatar IDs. Unsigned
  64-bit IDs and fencing epochs preserve their full widths.
- Each acquisition after vacancy/expiry, and each transfer, advances the retained
  epoch. Release clears the owner, never the epoch or row. Exhaustion rejects;
  no wrapping or deleting/recreating rows to reset authority.
- Ownership is a server-incarnation nonce plus epoch. A claim object is opaque
  and non-deserializable; its expiry is only advisory. Every mutation checks the
  actual database record and database clock after acquiring the row lock.
- Renewal cannot revive an expired/stale owner. Concurrent acquisition serializes
  through the unique InnoDB row; transfer/release use the same lock.
- `fenced` runs trusted business DML on the same SQL transaction/connection that
  holds the claim lock, rechecks validity after the callback, then commits.
  Handoff cannot commit between the check and protected write. Callback failure
  or observed expiry rolls back its business writes.
- Constructor admission checks InnoDB, required unsigned/binary columns and the
  exact composite primary key; a non-transactional lookalike is rejected.

## Integration requirements and limits

The database is the single primary writer, not a read replica. Configure a
bounded connection pool, acquisition timeout, lock wait timeout and transaction
work deadline in the consuming service. Use an operator-generated unique actor
incarnation nonce. Authenticated session identity and actor permission checks
happen outside this repository, before use. Expiry requires operational database
clock discipline; backward/forward wall-clock corrections are not a consensus
protocol. Fencing prevents stale committed writes; it does not physically stop
an old process or validate arbitrary external effects.

`fenced` callbacks are trusted native server code. They must execute only bounded
transactional **DML** through the supplied connection: no DDL/implicit commits,
manual transaction commands, external side effects, or inconsistent multi-claim
lock order. Restrict the runtime SQL role: no schema changes and no deletion of
claim rows. The explicit additive `migrate` method is an operator action, not an
automatic side effect of opening/acquiring a claim.

A preflight ownership check followed by unrelated DML is not a fenced write.
W15.2 must supply stable logical effect IDs, ownership/money invariants and an
outbox in this transaction. The epoch is a credential, **not** an effect ID.
Restart/retry/worker changes must reconcile committed outcomes by stable IDs.
Snapshot restore must not overwrite financial truth. Those paths are not yet
implemented here.

## Verification and safe reproduction

The initial pure policy regression was observed failing before its guard was
implemented. Actual SQL checks must run against both real MySQL and MariaDB;
no mocked database or missing-URL skip can establish acceptance. The CI retains
the actual Cargo.lock, compiler, server version and Docker image identity.

Use only a disposable database named with the suffix `_claimstest`. The tests
**change table engines and insert/update rows**, so they refuse other names.
Use a fresh database on each complete run; run serially because one test checks
rejection of MyISAM and restores InnoDB. Never point them at a production export.

```sh
export DATABASE_URL='mysql://test-user:test-password@127.0.0.1:3306/wonderland_claimstest'
cargo test --manifest-path crates/persistence/Cargo.toml --locked -- --test-threads=1
cargo clippy --manifest-path crates/persistence/Cargo.toml --all-targets --locked -- -D warnings
```

The suite contains five unit tests and eleven real SQL tests: two namespaces and
full-width IDs; sixteen contending acquisitions; renew/transfer/release fencing;
expiry/reacquisition; rollback on callback error; committed write observed by a
new connection pool; handoff waiting for a protected write; observed expiry
before commit; database time sampled after lock wait; epoch exhaustion; and
non-InnoDB admission. A new pool is a durability observation, not an actual
server-process restart or production failover test.

## Next E work (not silently counted complete)

1. Independently review command/claim authority and transaction safety. Integrate
   the leaf through F's workspace/contract owner only after review.
2. W13.1: inspect the existing `services/browser-gateway` before adding another
   gateway. Preserve the existing client wire ABI through a versioned adapter;
   add authenticated WSS origin/size/sequence/backpressure/heartbeat tests.
3. W13.2/W14.1: native lot-owner scheduler and account/avatar join grants consuming
   this store; renderer and DOM IDs never authorize actors.
4. W15.2/W13.3: atomic effects and durable outbox, commit-before-delivery recovery,
   stable operation identity independent of lease changes.
5. W15.3/W13.4: immutable snapshot state-after-N with matching tail-from-N+1,
   committed outcome reconciliation and private EOD exclusion.

No production migration, secret change, deployment, merge or independent approval
is included. Final CI observations belong in the dated evidence record, not in
assumed completion language.
