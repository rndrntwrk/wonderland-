# Action decision recovery qualification

This source candidate is based on exact PR78 commit
`39983bf3dad370e55f2d2dd30bc036b2347a7740`. It closes the host-crash window after
an action's durable decision but before host resolution/receipt export. It does
not change the published PR77/PR78 branches or deploy the application.

## Outcome

- New native player proposals bind exact action bytes, typed input/control and
  both native decision receipts into their canonical digest.
- The coordinator stores the selected outcome in the same SQLite transaction as
  its accepted batch or durable denial. The immutable decision row independently
  anchors the outcome SHA; missing/corrupt anchored evidence fails closed,
  including first archival of rejected receipts.
- A same-owned-session browser retains only a memory capsule across verified
  takeover and asks for the original decision after a matching fresh checkpoint.
  It never resubmits the action. Account/session/control changes cannot adopt it.
- Query authorization preserves normal security expiry/fence enforcement.
  Decision lookup does not execute game actions or renew custody/activity.
- Older actions without evidence, page reload that loses the memory capsule, or
  missing history remain Unknown. A successful receipt proves command acceptance,
  not successful completion of a source interaction.

## Evidence

`action-outcome-runtime.log/json` records actual production Rust Host and
persistent workerd/SQLite/R2 integration using explicitly authored protocol test
content. The test executes native Invoke/Cancel, kills the host with SIGKILL after
both accepted and rejected durable decisions but before resolve/export, restarts
workerd/host and adopts a recovery incarnation. Exact original WLA1 receipts
survive. Query/retry does not advance cursor/outbox; replay is refused and a
revoked session is denied before historical receipt lookup. The authenticated
runner dispatcher used by the application is exercised directly.

`HOST-QUALIFICATION.md`, host logs and source hashes cover all-target host build,
strict lint and 11 host tests, including history retention beyond 4096 batches.
`browser-capsule.md` and adjacent logs cover 7 new owned-recovery tests, 20 existing
player tests, 31 provider/socket/deadline tests and the WASM build check. Final
strict game-runtime all-target and WASM application lint are recorded separately.
`runner-tests.log` covers 20 action-result/authentication/retirement/manifest and
recovery helper tests. The coordinator/archival suite covers 128 bounded tests,
including transaction rollback, denial evidence, corruption and legacy absence.
Matching Rust 1.99 formatting was executed; the existing future-incompatibility
notice from proc-macro-error2 is unrelated to this candidate.

These are focused source/contract checks, not a claim that the whole workspace
or original-content rendered game was qualified locally. The process harness
cannot substitute for the complete actual-browser two-account journey.

## Next application work

See [PLAYER-JOURNEY.md](PLAYER-JOURNEY.md) for the integrated target and existing
code paths. The next source feature is terrain GoTo with the original GUID 0x7C4
creation/menu/queue semantics. Full owned leave/save/custody release and the
installation-to-qualified-manifest adapter remain explicit source work. Original
content acquisition and evaluation use only the owner's approved private input;
no proprietary assets are included in this patch.
