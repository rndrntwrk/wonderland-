# Source-owned departure: account persistence evidence

Date: 2026-10-15. Source pin: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
Reviewed working tree based on `af8e11b2`; account changes intentionally remain uncommitted for the combined account/coordinator review. No deployment or native departure success is claimed.

## Implemented account boundary

`services/native-edge/src/account/lot-save.ts` adds a strict, source-pinned save schema and SQL-backed store over the existing AccountAuthority tables. It does not accept writable portable state or select caller-provided proof URLs. A required injected reader retrieves immutable committed receipts; production wiring uses the configured COORD namespace and canonical lot ID.

- Exact 24-column source whitelist: big-endian signed motives; SleepState zero; unchecked byte/u16 casts; signed division by 100 and i16 lock reload; selected daywear/default suits/current persistent body; source skills/job/tickers.
- Existing schema-1 source/spec rows are required. Missing migrations never invoke creation constructors or fabricate source state.
- Only source person slots 0–5, 7 and 10–21 change. Other slots, identity/spec fields except daywear, permissions/membership/moderation, creation dates and ignored avatars remain unchanged.
- Only the current job is upserted. Only supplied dirty relationship indexes are changed; trailing columns, untouched targets, unrelated jobs and empty vectors are preserved. Persistent object targets may use the full nonzero u32 range.
- Saved default suits must currently be owned. Current body may be a source content outfit outside purchased wardrobe. No budget, ownership, inventory, account or session mutation comes from the save.
- Proof lookup precedes writes. Frozen export/claim/session/admission identities, progression baseline and wardrobe identity are verified, then every source/spec/claim/export/outfit value used for the merge is rechecked synchronously in the transaction after awaited work.
- Source merge, immutable receipt and permanent reservation-operation tombstone commit atomically while custody remains fenced.
- Release independently requires a trusted committed detach plus the exact durable account save receipt. It deletes only the matching current claim/export/lease. Monotonic generation allocation, operation history and immutable receipts remain.
- Exact save/release retries return their old receipt even after re-admission, without touching newer control. Cleanup works after logout/expiry/session rotation. Source-save/detach lineage across host takeover remains the trusted coordinator's responsibility.

`AccountAuthority` wires this store through `account/departure-proof.ts`. Ordinary accounts remain available without COORD; departure verification fails closed. The reservation tombstone check runs after the existing exact-active-claim retry branch and before any new allocation. No public browser save route was added. The private `requestAvatarControlFenceV1` requires the exact owning session plus avatar, generation, admission operation and reservation digest. It fences input without renewing session activity, changing source state or releasing custody; stale requests cannot fence a newer admission.

## Source correspondence

- `TSOClient/FSO.Server/Servers/Lot/Domain/LotContainer.cs`: `SaveAvatar`, `StateToDb`, `StateFromDB`.
- `TSOClient/FSO.Server.Database/DA/Avatars/SqlAvatars.cs`: `UpdateAvatarLotSave`, `UpdateAvatarJobLevel`.
- `TSOClient/FSO.Server.Database/DA/Relationships/SqlRelationships.cs`: per-index `UpdateMany` behavior.
- `TSOClient/tso.simantics/NetPlay/Model/VMNetAvatarPersistState.cs`: persistent body normalization and SleepState clearing. The account helper requires `body_for_persistence`; it does not infer native displayed outfit normalization.

## Verified tests

1. `cd services/native-edge && node tests/owned-leave-save.mjs`: 17 passing Node SQLite cases. The harness extracts the real AccountAuthority DDL rather than maintaining a toy schema. Coverage includes source casts, whitelist/excluded-field preservation, malformed/oversized/unknown fields, missing migrations, wrong/forged/rejected/prepared proofs, concurrent source/generation replacement, post-lookup snapshot changes, trigger-injected transaction failures, independent release gate, stale/new-generation isolation, permanent operation conflicts, permitted ledger/wardrobe-price changes, merge capacity failure, and cross-incarnation receipt acceptance under the trusted-reader contract.
2. `cd services/native-edge && node tests/owned-leave-save.mjs --workerd`: passed against actual workerd SQLite, actual AccountAuthority and real existing password WASM. Real registration/login/avatar creation/reservation/export were followed by configured coordinator-fixture lookup, owner/session-bound admission-specific fencing, exact fence retry without renewal, save, exact active-reservation retry, logout/release, workerd restart, terminal operation rejection, newer-generation re-entry using saved values, stale prior-admission fence refusal, age-derived skill locks, and old receipt replay without altering the replacement claim. The coordinator in this test is explicitly a fixture; no native actor or source leave is simulated by these assertions.
3. `cd services/native-edge && node --test tests/departure-proof.test.mjs`: 6 passing configured-reader tests, including no URL/namespace selection or fallback and exact returned receipt/digest binding.
4. `cd services/native-edge && ./node_modules/.bin/tsc --noEmit`: passed.
5. `git diff --check`: passed at account handoff.

Independent read-only review found no blocker in the account parser, merge, save, release, idempotency or transaction paths. Source hashes and captured test output accompany this report. These hashes identify the tested account files; a later combined integration edit requires rechecking affected tests.

## Explicit remaining boundaries

This evidence proves the account save/release lane and its configured lookup routing. It does not prove source UserLeaveLot execution, same-tick pre-removal capture, mode25 reference unwinding, EOD/blocking timeout behavior, native protocol/snapshot migration, coordinator accepted-execution provenance or ancestry, durable outbox delivery, never-admitted cleanup, actor-control retirement, browser departure UX, or the real two-account leave/re-entry journey. Those remain separate implementation and integration requirements. No successful native departure is fabricated to satisfy account tests.
