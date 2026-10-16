# Owned departure coordinator: retained evidence

Date: 2026-10-15.

Repository base at verification: `af8e11b2b3a595cd8b7028c1d724a4032f5e7c23`.
Source behavior pin: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

## Status and limits of this evidence

The private coordinator receipt, fence/drain, recovery-lineage and retirement lane is implemented and tested against real SQLite. The host observations in these tests are synthetic protocol fixtures. They do not execute the native source leave operation.

The native departure producer, source queue/mode-25 execution, EOD/blocking-state handling, native replay of departure state, browser integration and a real two-account leave/save/re-entry journey remain unimplemented or unqualified by this evidence. No successful source leave, playable departure, deployment or publication is claimed here.

This document supersedes the earlier implementation idea of retaining a permanent cursor link for every controlled tick. That design was rejected because a 65,536-row cap would impose a gameplay lifetime limit. The current implementation retains no departure-specific history for ordinary play with no outstanding saves.

## Changed code

- `services/native-edge/src/lot/departures.ts`: strict private proposal evidence, canonical operations, immutable save/detach receipt storage and lookup, outbox, active lineage tips, frozen detach attestations and exact control retirement.
- `services/native-edge/src/lot/owned.ts`: optional, strict `departure_v1` proposal parsing. Existing proposals remain valid.
- `services/native-edge/src/lot/coordinator.ts`: dedicated trusted drain lane, atomic receipt/registry/journal writes, departure methods and transactional recovery-tip advancement.
- `services/native-edge/src/lot/host-receipts.ts`: optional departure evidence retained in complete committed replies and compared against accepted journal evidence. Legacy replies remain valid.
- `services/native-edge/tests/owned-departure-coordinator.test.mjs`: production coordinator exercised with real SQLite and an in-memory R2-compatible test bucket.

Exact SHA-256 values for these files and relevant parser dependencies are in [coordinator-source-sha256.txt](coordinator-source-sha256.txt). The manifest describes the final files at evidence capture, not a claim that the working-tree changes were already committed.

## Private protocol and operational identity

The canonical operation is:

`authority_id + "_" + avatar_id + "_" + control_generation`

`departureOperationV1` derives this from authoritative identity. The request and evidence parsers require that exact spelling. A 64-character authority hash, source avatar ID up to 16,777,215 and decimal u64 control generation produce at most 94 characters, within the existing 96-character operation profile. User leave, logout and expiry for the same generation can therefore converge on one operation. A browser does not choose a new departure command or supply save values.

`departure_v1` contains exactly:

- Version, complete authority/account/avatar/lot/control/session/admission/export/principal/actor identity, and canonical operation.
- Lifecycle observation: tick, `leaving`, `blocked` or `detached`, and a bounded blocker code only when blocked.
- Zero or one strict source-save record, its canonical SHA-256 and event ordinal.
- Zero or one detach observation, tick, `source_leave` or explicitly qualified `forced_cleanup`, and event ordinal.
- A bounded, sorted, unique `entity_refs_after` projection from the trusted native post-state.

The parser rejects unknown fields, malformed IDs, invalid save hashes, observations outside the proposed accepted ticks, incoherent lifecycle state and contradictory entity presence. The target must be absent from the entity projection and controlled registry for detach; other controlled actors must remain present with their exact identities. Save bytes are retained independently of the live entity.

Only `commitOwnedDrainV1` accepts this evidence. It requires a previously begun operation, the exact admitted installation and an exact generation fence. It rejects `action_v1`, admissions, unrelated controlled-actor mutations and permission-binding changes. Ordinary `commitControlledBatchV2` retains its fence and actor-removal barriers and rejects departure evidence. This private lane is not exposed through a browser RPC dispatcher.

## Durable facts and trust boundary

Save and detach receipts are created only with the accepted batch's SQLite transaction. That transaction includes the batch decision, journal, cursor, registry/control revision, retained origin anchors, immutable receipt rows and retryable outbox. The decision commits hashes of the departure evidence and any resulting save/detach receipts. Denied or failed proposals create no committed source-save proof.

`lookupCommittedAvatarSaveV1` and `lookupCommittedAvatarDetachV1` accept strict references and resolve authoritative retained receipts. They compare the complete requested reference, receipt and record hashes, accepted batch decision, exact origin and retained anchor. A caller-supplied receipt or matching hash alone is not authentication.

The account-side proof reader is configured separately. Its independent account merge/release tests are documented in [ACCOUNT.md](ACCOUNT.md).

## Constant-space active lineage

After a source save commits without detach, one `departure_lineage_tips` row is retained for that control. It contains:

- The exact committed source-save reference and receipt hash.
- The current accepted host binding, cursor and checkpoint hash.
- A decimal transition sequence.
- A tip hash and the immutable accepted batch or committed recovery decision that anchors it.

Every subsequent accepted controlled transition verifies the exact current tip against the committed predecessor and prepares its successor. The SQLite transaction rechecks the exact prior row snapshot and advances it. The accepted batch decision records the successor tip hash. Unrelated ordinary batches before a save allocate no departure rows.

A takeover advances active tips in the same `finishControlledRecoveryV2` transaction that commits the takeover marker and new lot state. The prior tip must match the exact old binding, cursor and checkpoint. The committed recovery marker and retained controlled-recovery receipt anchor the successor tip hash. Rehashing a mutable tip cannot substitute arbitrary ancestry because its immutable decision anchor must also match.

Detach requires either:

1. A save and detach in the same accepted batch with matching origin and save-before-detach event order; or
2. An independently verified active tip whose exact binding/cursor is the detach proposal's committed predecessor.

Detach freezes a lineage attestation containing the original save reference/hash, validated predecessor tip (or the same-batch case), and detach origin. Its hash is committed in the same immutable batch decision. The active tip is then deleted. Lookup verifies the frozen attestation, original save proof and detach decision without replaying an ever-growing tick chain.

Receipt-origin anchors are retained only for batches producing save or detach receipts. Existing immutable receipts, tombstones and frozen attestations survive hot-journal compaction. Active tip count is bounded by outstanding saved-but-not-detached controls, rather than elapsed play time. Permanent operation/evidence capacity still fails closed rather than deleting history; it is not charged once per ordinary tick.

## Outbox and release retirement

`pendingOwnedDeparturesV1` exposes retryable immutable save/release references, with save ordered before release for each control. Coordinator detach does not itself release account custody.

`retireReleasedAvatarControlV1` accepts a strict detach reference, verifies the retained coordinator proof and independently queries `lookupReleasedAvatarControlV1` through the configured account namespace and realm. It compares complete identity, operation, save/detach hashes and committed detach. No raw caller release receipt is accepted.

After verified account release, one SQLite transaction moves the matching active control into immutable history and removes only that control's active row, lease and outbox rows. Fences and old proofs remain. Duplicate retirement returns the recorded release and cannot remove a replacement generation or principal. Permission/membership bindings remain unchanged.

## Retained verification results

### Focused coordinator tests

[coordinator-sql-tests.log](coordinator-sql-tests.log): 63 tests, 63 passed, zero failed. This retained full run includes the canonical-operation implementation. One additional explicit canonical-boundary regression was appended afterward and run separately against the final files: [coordinator-canonical-operation-test.log](coordinator-canonical-operation-test.log), 1 test passed. Together these cover the 64 focused tests in the final test file; the logs are not presented as a single 64-test run.

Coverage includes:

- Exact durable begin/fence identity, same-tick save/detach, blocked lifecycle, lease-expiry denial and trusted drain without a live lease.
- Unknown/malformed evidence, mismatched identities and save data, valid `action_v1` smuggling, target entity absence, peer preservation and unchanged permission bindings.
- Before/after injected failures at journal, origin-anchor, save/detach, frozen-attestation and outbox insertion boundaries; no partial proof or registry advancement.
- Source save followed by 384 accepted drain transitions, two actual committed coordinator takeovers, detach, and actual R2 journal compaction. One active tip and one save-origin anchor remain while draining; detach removes the tip and retains one frozen attestation plus two receipt-origin anchors. Authoritative lookup succeeds after compaction.
- Missing/unrelated origins, rehashed mutable tips, corrupted recovery anchors and rehashed frozen attestations fail closed.
- Tip-update and takeover-update failure rollback, verified account release, stale retirement against replacement control and retirement fault rollback.
- Complete host replay evidence retention and rejection of missing or altered departure evidence.
- 65,537 actual ordinary coordinator commits create zero departure operations, saves, detaches, outbox entries, origin anchors, active tips or frozen attestations. This stress test simulates the pre-existing hot-journal retention step while keeping permanent batch decisions. Actual R2 compaction is tested independently in the departure/recovery test above.
- Canonical operation derivation, convergence across equivalent generation identity, maximum 94-character spelling and rejection of browser-chosen operations.

The 65,537-commit retained run completed in approximately 57.3 seconds. It is a coordinator protocol regression, not native simulation performance evidence.

### Existing regression suites

[coordinator-regression-tests.log](coordinator-regression-tests.log): existing action-outcome and host-receipt tests, 128 passed, zero failed. These preserve ordinary input fencing, action receipt durability, legacy receipt compatibility and archive behavior.

### Static checks

[coordinator-static-checks.log](coordinator-static-checks.log): TypeScript `--noEmit` and `git diff --check` both exited zero. No native build, deployment or credential/environment change was required for this coordinator evidence.

## Reproduction

From the repository root:

```sh
node --test services/native-edge/tests/owned-departure-coordinator.test.mjs
node --test services/native-edge/tests/action-outcomes.test.mjs services/native-edge/tests/host-receipts.test.mjs
services/native-edge/node_modules/.bin/tsc --noEmit -p services/native-edge/tsconfig.json
git diff --check
sha256sum -c docs/verification/native-owned-departure-20261015/coordinator-source-sha256.txt
```

These Node tests use SQLite and synthetic private-host observations. They do not substitute for the missing native producer and real source departure journey.
