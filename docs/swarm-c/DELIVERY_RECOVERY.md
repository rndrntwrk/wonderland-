# Swarm C delivery recovery — 6 October 2026

## HTTP runner correction

Published input: `5d533eafe0ff1737e1d0b2b309e6426bc634c281`.
Hosted run: `37402221893`; observed Bevy WebGL2 job: `112071727277`.

The real GPU picker logged six scenes, 60 picks and 18 interruptions, but its
Node test failed with `ERR_HTTP_HEADERS_SENT` from the asynchronous fixture
server. The server committed HTTP 200 before awaiting `readFile`, then attempted
HTTP 404 when a missing request failed. This is a test-transport defect, not a
reason to weaken image, identity or lifecycle assertions.

The shared fixture server reads and resolves before sending a successful status,
serves an empty optional favicon, rejects escaped/symlink paths, and avoids writes
after an abandoned response. Seven unit/real-HTTP regressions cover the failure,
concurrent WASM/module loads, missing resources, path decoding, HEAD and methods.
The missing-file regression was demonstrated failing against the original
listener before the fix; all seven pass after it on Node 22.16.0. The browser
runner executes them before packaging or GPU tests. Syntax and whitespace checks
also pass. No graphics shader, fixture or comparison threshold was changed.

A local attempt to execute the retained exact Bevy WASM was blocked by managed
Chromium with `ERR_BLOCKED_BY_ADMINISTRATOR` on localhost before application
execution. This is not recorded as a browser pass. The published workflow rerun
must supply current browser evidence, separately from earlier local results.

## Source preservation and missing client

The source-handoff workflow archives only tracked rewrite source inputs with
commit, tree and archive SHA-256 identities; no Git credentials or untracked
workspace files are included. It preserves C and the exact PR18 input separately.
It is not recovery of the previously claimed but unpublished client continuation.

The earlier `feat/swarm-c-client-integration` handoff had no published ref or PR
when checked. Until its actual code is recovered or rebuilt and published, the
links in older C handoff records are intentions, not delivered client evidence.
No merge, deployment, full original-client parity or physical-device acceptance
is claimed by this correction.
