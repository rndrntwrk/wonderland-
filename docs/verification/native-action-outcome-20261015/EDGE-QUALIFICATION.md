# Native edge action-outcome qualification

Date: 2026-10-15 UTC. Qualified working-tree base:
`39983bf3dad370e55f2d2dd30bc036b2347a7740`.
This receipt describes the uncommitted edge implementation at verification time;
`edge-source-sha256.txt` identifies its exact source, tests, package scripts and README.

## Results

- **PASS:** 128 lightweight tests across `tests/action-outcomes.test.mjs` and
  `tests/host-receipts.test.mjs`. Complete retained output: `edge-action-tests.log`.
- **PASS:** `npm run check` in `services/native-edge` (`tsc --noEmit`).
- **PASS:** Prettier check of the four modified/new lot TypeScript modules, three
  modified/new lightweight test/driver files, and `package.json`.
- **PASS:** `git diff --check -- services/native-edge`.

The final TypeScript, formatting and diff-check commands were rerun after all
functional edge changes and the final formatting-only line wrap. Their terminal
output, tool versions and exit codes are retained in `edge-checks.log`.

## What the edge tests establish

The lightweight suite imports the production coordinator with only the
`cloudflare:workers` base class stubbed, then executes its transactions against
real Node SQLite. It covers:

- Exact accepted/rejected WLA1 evidence retained atomically with the decision,
  cursor, journal and outboxes, without host resolve or R2 receipt export.
- Injected failure immediately before and after the action row INSERT, proving
  rollback of accepted state/outboxes and rejected expiry fences/denials.
- Exact-request cross-batch conflict rejection and unchanged outbox/cursor;
  different request bytes may reuse a rejected sequence.
- Bounded, exact Rust fixed-integer WLC1/WLA1 layout, original typed-input and
  request hashes, u64 sequence precision, all five authenticated control fields,
  actor generation and unchanged actor registry.
- Read-only original-incarnation lookup during expiry and recovery, synthetic
  successor-boundary lookup, and old proposals without action evidence.
- Independent durable-decision SHA-256 anchors over the whole selected outcome
  and input hash, including corrupted status plus syntactically matching WLA1,
  tick/cursor/binding/digest/input changes, and missing or altered decision rows.
- First archive export checks for accepted/rejected receipts and input hashes;
  a retained decision anchor with missing outcome evidence fails closed.
- Live and archived payload compatibility, legacy archives, and exact action
  lookup retained independently of journal/R2 bytes.

## Scope and remaining qualification

These are storage-contract and corruption/fault tests, not a VM execution proof
or provider-wide failure simulation. No workerd or Cargo process was started for
this edge receipt. The separately retained `HOST-QUALIFICATION.md`,
`action-outcome-runtime.log` and `action-outcome-runtime.json` cover the real Rust
host/workerd process-kill, persistent restart, takeover, authenticated dispatcher
and session-revocation integration journey. Publication and any final aggregate
checks are owned by the coordinating task. This worker did not commit or deploy.
