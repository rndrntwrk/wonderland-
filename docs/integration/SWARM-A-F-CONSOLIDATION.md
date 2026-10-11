# Swarm A–F source consolidation

Integration starts from master 4fd3b471b8d273dbebbf293282bfd6b898fc1970 (PR57). That merge is not a claim that all swarm work or production functionality was complete.

## Preserved source

- PR54: independent persistence claims/fencing workspace and its disposable SQL integration workflow.
- PR46 plus its PR40/42/43/44/45 ancestors: reference and differential tooling.
- PR48: original-object content prerequisite qualification. The expected result intentionally retains incomplete static cohort blockers.
- PR24: bounded FIFO delivery of legacy VM callbacks and fail-closed overflow recovery. Newer native pose/effect fences remain intact.
- PR6 (8a0e251d19e222a0a6833d7408ca629f674e1729): missing independent Swarm A replay/reference tooling, without replacing the newer simulation implementation.
- PR12 (bf3d6432494ab436d86708a283be90d1a74267c1): absent Swarm C source, native audio adapters, engine probes, cooking/reference tools and historical evidence. Existing core code remains; missing environment and source-facade APIs are added rather than resetting their modules.
- PR37 is superseded by the newer accepted-frame native pose history and scope/reset fences already integrated through later work. Its old implementation is not reapplied over those fixes.

Historical C recovery workflows that push branches or retrieve old handoffs are preserved as text under historical-workflows, not activated. Historical reports retain their original provenance and are not current release receipts. Active restored workflows use the current 1.99 toolchain; isolated dependency locks align flate2 with the current workspace requirement.

## Fresh local verification, 11 October 2026

- Combined workspace all-target compile and formatting pass.
- Final optimized browser/WASM build passes.
- Actual compiled frontend with controlled gateway: both desktop and mobile retain two same-task VM callbacks; 257 queued events cause fail-closed disconnect, show reconnect, and remove every stale playable canvas. No page errors. The historical software-only selector now accepts the current verified WebGL2 renderer too, waiting for renderer initialization.
- Swarm A actual native/WASM simulation replay: 64 WASM executions, 5,840 exact tick comparisons and 320 complete snapshot comparisons, with no host imports.
- Swarm C actual native/WASM presentation replay: 18 synthetic scenarios pass, with no host imports or shared memory.
- Static preview server now serves .mjs as JavaScript for browser module loading.

These are local integration results, not proof of original full-object parity, production multiplayer, hosted SQL claims, every hardware/engine backend, or a Cloudflare deployment. Hosted workflows and content differential runs must qualify this final branch before merge. The controlled gateway and authored fixtures must never be presented as the production service.

## Integration review corrections

The restored legacy FIFO initially queued legacy VM frames in native-lot mode, which has a separate authoritative socket and no legacy consumer. An immutable client-mode flag now prevents that unused queue while retaining gateway/session state processing. Actual native desktop/mobile browsers survive 300 original legacy callbacks and continue advancing accepted native ticks; legacy mode still disconnects safely on overflow. Both journeys are wired into their hosted workflows.

The boundary map was re-reviewed for the integrated resource codecs, frozen WLB1 appearance serializer, checkpoint avatar reconstruction, accepted terrain/visibility, and separate construction protocol. Current pins are refreshed with semantic notes; previous source and boundary maps remain historical records. This does not promote inspection to an ABI freeze or original-engine equivalence.
