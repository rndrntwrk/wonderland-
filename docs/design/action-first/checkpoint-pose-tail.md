# Checkpoint-tail avatar reconstruction

Base: combined player PR #47, `0c9f2c194bd6280d16cf6c4438941c1974da2690`.
Resumes the existing test-first branch `fix/native-checkpoint-pose-tail` at
`58843911ea9549a7418bfb74ee6931f5f7089eb2` rather than recreating the player.

## Corrected behavior

Recovery previously validated and executed a supplied checkpoint tail with visual
capture disabled. The wire adapter then returned only the final recovered pose.
A clip could start in the checkpoint, run, and end entirely inside that supplied
tail; its reconstructible intermediate bone channels were lost. This differs
from legitimately unavailable history before the checkpoint.

`LiveReplica::install_checkpoint_with_avatar_frames` now captures the validated
checkpoint seed plus each nonduplicate accepted tail frame using the existing
bounded `AvatarCapture`. The candidate state and trace stay private until every
checkpoint, tail hash, anchor and recovery-position check succeeds. The ordinary
state-only method disables capture and preserves its existing cursor-only API.

The existing replay function separates visual capture from outcome publication.
Recovery still publishes **no historical TickOutcomes**. NativeWire forwards the
optional trace with `Received::Checkpoint`; NativePlayer keeps that checkpoint
variant, and the existing native-lot host clears old bones before consuming all
supplied frames. No extra host mailbox, schema change, simulation tick, source
command, action receipt or audio replay is introduced.

The same capture limits remain: 8 MiB of accounted presentation metadata and
16,384 avatar records across the trace, alongside existing packet/tick/command
limits. A presentation limit failure discards the whole optional trace while
allowing valid simulation state to commit. It does not expose a partial pose
prefix or advertise a multiplayer limit. The new cap test uses 257 explicitly
out-of-world fixture avatars over a seed and 64 ticks; original game capacity,
placement and routing are not inferred from that stress case.

## Acceptance evidence

The recovered 269-line specification matched Git blob
`73fe4686224267c92185f66a7f9c7f48cacca9de` before local formatting and expansion.
On unchanged PR47 production code, six of its eleven tests failed on missing
seed/intermediate visuals. Its invalid-checkpoint and obsolete-callback safeguards
already passed. The corrected version passes all sixteen final cases, including
maximum tail length, missing ticks, conflicting/behind anchors, optional-budget
exhaustion, duplicate suppression and unchanged unknown-action status/history.

The original-format pose conformance probe now additionally recovers its seed
plus eighteen real ticks. Its clip ends inside the tail. The retained geometry
matches ordinary live replay, while a separate cold final-state checkpoint still
resets unavailable older channels. The complete native and ordinary WASM output
is compared; three additional negative controls reject a missing tail, a reset
model masquerading as recovery, and replayed activity.

The new real-browser test first failed against PR47's unchanged hosted release:
live replay retained the pose, but checkpoint-tail recovery returned bind-pose
pixels. Against the rebuilt application, both produce identical final canvas
bytes and authority hashes on elevated terrain. Later idle ticks and camera
rotation retain the reconstructed pose. A separately selected source action
works, the 390x844 view has no horizontal overflow, and both sessions dispose
their sockets and viewport. The test uses actual encoded Rust checkpoint/tick
messages and the ordinary Game content/import/login/Visit/Reconnect controls.
No product WASM, UI, CSS, clock or wire bytes are replaced.

All eight inherited browser journeys are rerun against the same release, including
actual audio output, receipt deadlines, cancellation, hidden-avatar selection
and full-packet pose retention. Counts, command exits, source and report hashes
are in `checkpoint-pose-tail-verification.json`. Local checks are separate from
whatever hosted runs the published commit subsequently produces.

## Reproduce

Use the pinned Rust toolchain and existing locked browser tooling. Keep controlled
services isolated from real accounts and saves; fixture processes share fixed
ports and must not be run concurrently.

```sh
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
node --test --test-reporter=tap apps/web-shell/scripts/native-*.test.mjs
node --test --test-reporter=tap crates/audio-runtime/browser/*.test.mjs
(cd apps/web-shell && trunk build --release --locked)
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
cargo run -p wonderland-web-shell --example native_avatar_resources --locked \
  -- target/native-avatar-fixtures
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install --with-deps chromium
WONDERLAND_NATIVE_QA_OUTPUT=/tmp/checkpoint-tail \
  node tools/native-browser/verify-checkpoint-tail.mjs
```

The existing read-only Native browser workflow runs all nine journeys; the Native
pose retention workflow also runs the checkpoint regression suite and expanded
native/WASM probe. Neither workflow regenerates production code.

## Boundaries

This reconstructs **supplied** checkpoint/tail inputs only. The checkpoint still
has no serialized historic bone transforms or running sound-engine state. Partial
channels, weighted blends or sound loops depending on unavailable earlier history
cannot be reconstructed by inventing values or retaining another connection's
state. The equality test deliberately supplies a seed before its clip runs.
Historical sounds and completion messages must remain silent, and an Unknown
receipt must not become Accepted just because a checkpoint was received.

The red mesh and sound resources in these tests are authored fixtures encoded in
original formats, not original human/audio artwork. Chromium/software-rendering
checks are not Safari, physical-device, production-service or performance approval.
The five existing optional corpus/reference tests remain unrun. A local attempted
standalone budget run timed out waiting for another Cargo build lock; its full
suite rerun subsequently executed and passed the test. These are distinct results.

This change does not implement the still-disabled native Buy/Build interface,
complete inventory/property/social/object dialogs, original walking/contact and
content qualification, production distinct-player services, durable reconciliation
or original FSOv/VMNet translation. No original source/assets, Cargo.lock,
dependency versions, parent branches, main or deployment configuration changes.
