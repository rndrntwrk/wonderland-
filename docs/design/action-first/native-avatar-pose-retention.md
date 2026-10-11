# Native avatar pose retention across accepted ticks

## Integration base and delivery

This is a direct source patch against PR #34, `13580dd2d0e2fefbefb73e4d0011032f80c06fb0`, in `rndrntwrk/wonderland-`. It is locally implemented and tested, **not published, merged or deployed**. No GitHub writing action was exposed in the session that produced the patch. Every modified file's preimage was checked against its blob ID returned by GitHub at that pinned revision. New files are supplied as ordinary source, not a CI assembly step.

The local workspace was reconstructed offline from previously downloaded source/compiler/registry artifacts and the existing avatar/receipt source inputs. It is not represented as a fresh remote clone or as a byte-identical Git tree: the capture predates some documentation and browser-witness-only changes. The patch contains only its explicitly listed changed files and preserves those unrelated remote changes. Fresh CI and rendered-browser acceptance on the resulting integration branch remain required.

## Correction

The earlier native adapter always started the avatar at its bind pose before sampling the current animation. When an animation ended, disappeared or only wrote some bone channels, earlier accepted transforms were lost. Reapplying a weighted blend on every draw would produce a different error: display cadence would change the pose.

The source-derived `wonderland_avatar_view::PosePlayer` already owns retained bone transforms and commits its timeline once per tick. This patch connects that existing implementation to native play rather than adding another animation engine. Its original prefix-weight, ended-layer, carried-pose and safety semantics are unchanged. The original `VMAvatar.FractionalAnim` and shared `crates/avatar-view/src/animation.rs` are the source/reference anchors; neither is edited here.

An additional correction is necessary at delivery: a validated network packet can contain several ticks, and the reactive renderer can coalesce notifications. Sampling only the packet's final state loses bone writes from the intervening frames. The new optional presentation entry points capture every accepted intermediate avatar input and deliver the complete trace before any render notification.

## State flow

1. `LiveReplica::apply_batch_with_avatar_frames` applies the actual accepted native ticks and verifies their resulting hashes. It captures read-only avatar inputs after each validated nonduplicate transition. A later failure rolls back the complete simulation batch and returns no partial trace or outcomes.
2. `NativeWire::receive_with_avatar_frames` and `NativePlayer::receive_presented` carry that trace through the existing generation, packet, checkpoint and admitted-actor checks. Existing `receive`, `receive_update` and `apply_batch` interfaces retain their behavior without allocating per-tick avatar captures.
3. The native-lot socket callback feeds each returned frame to `NativeAvatarPoseHistory` synchronously. A render wake may coalesce afterwards, but the bone history has already observed every accepted tick. Receipts contain no pose samples and still settle only their independent acknowledgement deadline.
4. `NativeAvatarProjection::prepare_retained` uses that committed pose for the existing appearance composition, skinning, source-identity checks and model preparation. Textures, camera changes and repeated draws at the same tick cannot compound blending. The existing stateless `prepare` remains available and preserves its former behavior for independent snapshot callers.

There is no new wire packet, serialized simulation field, authoritative command, dependency or source-content format. Capturing presentation cannot change state hashes, native events or action decisions. No rendering-time call executes animation markers or advances the VM.

## Lifetime and recovery policy

Retention is scoped to the imported bank identity, native lot/epoch/content revision and exact entity generation/GUID. Invisible avatars still receive accepted updates. Removed entities are dropped; a recycled object ID cannot inherit an earlier Sim's pose. Stale or conflicting same-tick inputs reject without replacing the committed cache. A malformed timeline invalidates only that avatar's presentation; it cannot reuse an apparently valid old pose.

An explicit checkpoint contains native state but no serialized bone-retention history. Recovery therefore clears this presentation cache and seeds **only the final recovered frame**. It does not replay historical presentation events or manufacture the transforms from missing pre-checkpoint ticks. A changed resource bank, an observation gap, a changed source identity or unavailable capture also restarts from bind pose. A replaced bank resets even when its skeleton hash happens to match.

This means an ended animation can visibly reset after checkpoint recovery if its retained bone channels were only known before that boundary. That limitation is deliberate and documented; exact historical pose restoration requires an explicit source-compatible checkpoint/history contract and is not claimed here. While disconnected, the existing displayed world stays frozen until a valid new world is published. This patch does not add speculative motion or automatically retry actions.

## Presentation budgets

An optional capture allows 16,384 avatar records across one batch and 8 MiB of encoded metadata plus fixed-record accounting. Exceeding the budget discards the **whole optional trace**, not an accepted simulation prefix; normal gameplay transitions still commit. The browser clears retained history, diagnoses the reset and seeds from its current accepted state.

The retained-pose cache caps presentation at 1,024 avatars and 65,536 bones. These are memory/work safeguards, **not account or multiplayer population limits**. Over-budget or unsupported appearances are diagnosed rather than changing gameplay. The limits are not a complete browser-heap or performance guarantee: native state, rollback copies, source clips, renderer geometry and texture allocations remain separately bounded/profiled by their owners.

## Executed local evidence

| Gate | Result |
| --- | --- |
| Locked native workspace | **1,439 passed, zero failed; five existing optional checks ignored** |
| Added native tests, included in that total | **20**: 12 retention cases, four accepted-replay cases, two player/wire cases and two capture-boundary cases |
| Native socket/deadline tests | **20 passed**, zero failed or skipped |
| Existing audio tests | **59 passed**, zero failed or skipped |
| Formatting and strict native all-target Clippy | Passed |
| Actual web-shell WASM component Clippy | Passed |
| Optimized ordinary WASM conformance probe | Built and executed in Node WebAssembly |
| Complete native/WASM record | **18,973 identical bytes**, zero host imports, non-shared memory |
| Comparator negative controls | Seven equal-but-wrong semantic records and one changed-output record rejected |

The executable probe uses the real `GameRuntime`, `NativePlayer`, binary wire, `PosePlayer` and native geometry adapter. It processes the same 18 accepted frames in packet sizes 1, 3 and 18; all complete reports match. It also checks 120 repeated draws, latest-tick duplicates, an ended animation's retained translation, and a checkpoint reset without invented bone history. Its geometry and setup are explicitly synthetic original-format fixtures, not original human artwork or a second game simulation.

The probe compiles the exact DOM-free `native_avatar.rs` source by path. Linking the entire web-shell library initially retained unrelated wasm-bindgen exports and was correctly rejected by the no-import check. The corrected probe does not provide dummy host imports, transform its WASM, or weaken the comparator. The separate strict WASM check compiles the actual native-lot browser connections.

Meaningful historical failures are retained: the initial stateless retention path failed three tests; an adapter that captured only the final state failed the intermediate-frame/duplicate cases; and restoring former bind-pose sampling makes the rotation-only channel witness fail. Restoring the retained path returns it to green. A missing API, initial Rust module-path error and the whole-library probe import failure were compilation/probe setup failures, not passing gameplay evidence. The pre-existing future-Rust `proc-macro-error2` notice remains.

**Not executed for this patch:** a new Trunk application build, fresh Chromium/Playwright interaction journeys, hosted CI, real human-avatar resource qualification, physical devices, Safari, sustained multiplayer/performance tests, or independent code review. Passing the older PR #34 journeys is not represented as testing this new source. The original five ignored gates remain named in the machine-readable handoff.

## Reproduce

Use the repository's pinned toolchain and locked dependencies. Run from its root; select an empty output directory:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
node --test --test-reporter=tap apps/web-shell/scripts/native-*.test.mjs
node --test --test-reporter=tap crates/audio-runtime/browser/*.test.mjs

mkdir -p /tmp/wonderland-pose-check
cargo run -p wonderland-web-shell --example native_pose_retention_probe --locked \
  > /tmp/wonderland-pose-check/native.json
cargo build -p wonderland-web-shell --example native_pose_retention_probe \
  --target wasm32-unknown-unknown --release --locked
node apps/web-shell/scripts/verify-native-pose-retention.mjs \
  /tmp/wonderland-pose-check/native.json \
  "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/examples/native_pose_retention_probe.wasm" \
  /tmp/wonderland-pose-check/wasm.json
```

The added read-only `Native pose retention` workflow repeats the focused tests and complete native/WASM comparison after publication. It has not run on GitHub for this patch. Existing full Browser UI and Native browser gates remain separate.

The comparator refuses to overwrite an existing WASM output. It verifies complete byte equality, not only selected numerical fields; wide original resource IDs are not round-tripped through JavaScript before the equality check.

## Review and remaining integration

Before merging, build the full Trunk application and rerun the existing native action/avatar/receipt journeys on the final branch, including an ended or partial-channel animation delivered in a multi-tick packet. Check unchanged mesh picking, textured model lifetime, pause/reconnect, selected controls and acknowledgement deadlines. Keep original resources, other swarms' terrain/audio/renderer branches and default legacy/preview behavior intact.

Head seeking, bone/SLOT/container attachments, source-complete rig/outfit coverage, historical bone checkpointing and original human-art acceptance remain. Gameplay audio and terrain contact are separate workstreams. This patch neither supplies production distinct-player services or durable reconciliation nor completes Buy/Build, property, social or specialized object interfaces. It does not claim completion of the full FreeSO rewrite.
