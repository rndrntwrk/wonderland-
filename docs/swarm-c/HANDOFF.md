# Swarm C integration handoff

## Ownership and publication

C owns disposable presentation state. A owns simulation, B content decoding and
provider precedence, D the client/DOM host, E live directory/admission and F
shared contracts/integration. Engine handles, device generations and render
counters are not persistent game identities. Rendering/audio cannot execute
durable effects or advance authoritative gameplay.

Core code is published in [PR12](https://github.com/rndrntwrk/wonderland-/pull/12).
The recovered client code is [PR23](https://github.com/rndrntwrk/wonderland-/pull/23),
stacked on PR18 at `a318581f33770540808aefcf132018255a3a1d94`. Do not merge either
branch merely because its bounded smoke tests pass; preserve review of the
actual combined source and dependency contracts.

## What the receiving team can use now

| Delivery | Actual boundary |
| --- | --- |
| Source rendering libraries | Immutable frames, iso/depth/light algorithms, lot/city/reconstruction, bounded caches and source derivative workers in this branch; consume explicit validated source inputs. |
| Engine probes | Bevy WebGPU, Bevy WebGL2 and Fyrox WebGL2 share synthetic source-sensitive fixtures and asynchronous generation-checked GPU picking. They are comparison/test applications, not the complete Wonderland client. |
| Avatar/audio libraries | Rig/appearance/pose/contact algorithms, HIT/cue/codec/mixer and native/browser adapters with their recorded tests. Core availability does not supply every client host or authorized resource graph. |
| PR23 application | Rust-prepared source-3D geometry/textures, real WebGL2 color/ID rendering, asynchronous picking, source controls/import and context-loss recovery in the existing application. Its built-application gate is distinct from the engine probes. |

PR23's [client guide](https://github.com/rndrntwrk/wonderland-/blob/feat/swarm-c-client-integration/docs/swarm-c/CLIENT_INTEGRATION.md)
provides exact release/WASM identity, browser evidence and commands. Missing
source scenery remains diagnosed, not replaced with invented complete content.

## Work that is still implementation

The earlier full client continuation was not recovered. Its proposed source
sprite/hybrid/Full2D composition, advanced lighting/environment adapters,
complete accepted avatar/audio-host composition and PNG/FSOf client exports are
not all present in PR23. Continuous original VM restoration also remains outside
this client increment. These are code/integration gaps, not simply requests for
physical-device QA. Follow [COVERAGE.md](COVERAGE.md); no scope item is retired.

Consume A's actual immutable accepted fields and timeline; do not manufacture
continuous motion or authority from discrete render state. B must provide the
complete required resources, normalized coordinates, effective content/patch
identities and explicit missing-data outcomes. Source city/admission and A
accepted authority identities remain separate contracts. Connect derivative
queues and CPU/GPU ownership to actual application lifetime before claiming
client exports or facade delivery.

## Evidence and historical correction

[DELIVERY_STATUS.md](DELIVERY_STATUS.md) records the browser-runner repair,
current executed tests and exact source/artifact IDs. The previous handoff is
preserved unchanged at `evidence/pre-recovery-handoff/HANDOFF.md` because it
contains useful interface research as well as unsupported full-client delivery
claims. Its relative client links and prose are historical, not proof those
modules exist in the recovered branch.

Physical browser/GPU/audio devices, supported-device performance, live account
and destination journeys, source-corpus parity and final engine choice require
separate evidence. No merge, live credential change, production deployment or
billing change is included.
