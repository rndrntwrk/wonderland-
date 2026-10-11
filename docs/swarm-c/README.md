# Swarm C — views and sound

Swarm C owns W07–W10 and SL.5. The original 17-item scope is preserved; passing
an isolated library or engine fixture does not complete the corresponding game
feature or client integration.

## Current delivery

- [PR12](https://github.com/rndrntwrk/wonderland-/pull/12): source presentation libraries, avatar/audio algorithms, derivative workers and independent Bevy/Fyrox engine probes.
- [PR23](https://github.com/rndrntwrk/wonderland-/pull/23): the published replacement source-3D client viewport and asynchronous GPU selection, stacked on PR18.
- [Delivery status](DELIVERY_STATUS.md): exact repair, source revisions, executed browser evidence and remaining implementation.
- [Coverage](COVERAGE.md) and [handoff](HANDOFF.md): the current integration boundary, not a claim of full client parity.

The earlier comprehensive client continuation was not recovered. Old statements
that all production client composition was complete are not supported by the
recovered branch. The previous README, coverage and handoff are retained byte
for byte under `evidence/pre-recovery-handoff/` as historical records, not current
acceptance. No requested capability is retired by this correction.

## Core and verification entry points

The core paths are `crates/render-core`, `render-iso`, `render-3d`, `avatar-view`
and `audio-runtime`. Core reference libraries retain Rust 1.75; isolated engines
retain their pinned workspaces/toolchains. Client PR23 retains its existing
workspace toolchain. Authoritative A state and original `TSOClient`/`Other` are
not modified by the browser-runner repair.

Run `tools/swarm-c/verify.sh`, `verify-native-wasm.sh` and `verify-audio.sh` from
the repository root with their documented toolchains. Engine build/execution
commands are in `probes/engine-bakeoff/README.md` and its `web/GPU-PICKING.md`.
Actual client reproduction and the release bundle are documented in PR23's
`docs/swarm-c/CLIENT_INTEGRATION.md`.

[VERIFICATION.md](VERIFICATION.md), per-module source notes and the evidence
folders retain dated results. Do not promote an older pass to a newer revision,
or replace a failed page screenshot with a passing private readback. The current
headed-software WebGPU repair is separately recorded in [delivery status](DELIVERY_STATUS.md).
Both PRs remain unmerged and undeployed. Physical hardware, live services,
complete authorized source cohorts and final production-engine selection remain
separate acceptance work.
