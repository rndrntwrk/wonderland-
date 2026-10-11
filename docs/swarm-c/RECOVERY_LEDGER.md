# Swarm C recovery ledger — 2026-10-07

Canonical starting point: PR28 `4ebaf5590e46441918dabeab79218df622ad2db6`.
Local archive: `d52689c8db00d66bb544f043efb55da1338fd74a`; the two intervening
upstream files are documentation only and will not be replaced.

The previous `c-completion/repo` source was not retained across the session.
Only four evidence files survived. No claim is made to recover that exact code,
to replay its nine cases, or to have fixed tests whose source is now missing.
This branch reconstructs the lighting integration from the published source
libraries, closes the same admission/panic failure classes with new tests, and
preserves a cumulative source patch and GitHub branch rather than reports alone.

## Approved constraints
Preserve original sources and all existing screens. Rendering cannot authenticate
actors, commit effects, change the VM clock, or become a dependency for E's
transport and database claims. C acceptance does not mean full original-game
parity, live-service acceptance, or physical-device performance.

## Execution
1. Reproduce malformed-lighting admission on published PR28.
2. Restore unchanged published render-iso library; implement bounded lighting
   document validation, safe atlas sampling, and CPU/GPU composition.
3. Run native/WASM and existing GPU/PNG regression checks; publish code and evidence.
4. Publish E's W13.1/W15.1 starting scope against existing contracts, not renderer IDs.

Ruling: The lost source cannot be counted as delivered. Start from the latest
published client and reuse the existing C lighting algorithms; do not reconstruct
unrelated avatar/sprite/facade code from unsupported prose. Those missing paths
remain explicit work rather than being silently retired. Cost: their application
integration still has to be completed and tested separately.
