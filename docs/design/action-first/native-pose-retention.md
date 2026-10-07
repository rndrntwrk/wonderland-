# Native avatar pose retention

Continuation of PR #34 at `13580dd2d0e2fefbefb73e4d0011032f80c06fb0`.

## Player-facing correction

The native avatar adapter previously rebuilt its bind pose before every sample. A clip that ended, a rotation-only replacement, or a removed carry layer could therefore erase channels the source animator intentionally leaves untouched. This change retains validated skeletal channels between observed accepted samples, without changing simulation rules or the original source files.

The source anchors are `TSOClient/tso.simantics/Entities/VMAvatar.cs` (`FractionalAnim`, `Tick`, `Save`, `Load`) and `crates/avatar-view/src/animation.rs` (`sample_timeline` and `Clip::apply`). The original visual sampling operates on the existing skeleton. Its native port already supports a mutable input pose and applies only the channels present in active clips. The missing connection was the native player's fresh bind-pose baseline on each call.

## Boundaries and semantics

`NativeAvatarHistory` owns one bounded presentation history. `prepare` returns a detached `NativeAvatarSample`; `apply` validates the world/resource records and commits both the prepared world and the history together. A failed apply cannot change either committed state. Opaque ownership tokens reject a sample prepared before another commit or clear, including a sample from a different history instance.

Entries are keyed by exact entity generation and GUID, scoped to the world lot/epoch/content and the exact imported resource-bank instance. A changed bank or source identity starts from bind pose. Removed entities are pruned. Invalid/unavailable animation resources drop the unproven pose entry and remain explicit diagnostics; no guessed mesh or old pose is substituted.

For a new observed accepted tick, sampling begins with the last committed presentation pose. For the same tick, it begins with that tick's saved **before** pose. This lets a texture-completion wakeup reproduce the same weighted result, instead of applying another blend over an already blended skeleton. Backwards ticks and conflicting same-tick avatar inputs reject. Architecture revisions may advance without erasing unrelated bone history.

A hidden avatar can update its valid skeletal channels without allocating a visible mesh or loading textures. Showing it after a clip ends therefore preserves the pose it actually sampled. Each history is limited to 1,024 instances and 65,536 total input bones; two poses are retained per instance. These are presentation working-set limits, **not** an advertised multiplayer capacity or a total browser-heap guarantee. Existing content, mesh, texture and protocol limits remain intact.

### Checkpoint and reconnect policy

The original checkpoint does not serialize the historical visual bone pose. A newly admitted native checkpoint, connection replacement or bank replacement explicitly clears this cache; it does not invent the missing pre-checkpoint skeleton or replay unseen animation history. The last displayed model remains frozen while disconnected. After recovery, active animation channels sample from the original bind baseline; an already ended clip can no longer recover untransmitted earlier bone values. That limitation is intentional and covered by the browser witness.

This retains **observed presentation samples**, not one hidden sample for every logical tick. It does not claim render-cadence-independent history for source channels whose results depend on previously rendered poses, or source-complete animation restoration after skipped ticks. Authoritative state, RNG, timing, queues and the wire format are unchanged. Head seeking, full contacts and attachments require their separate original-source adapters.

## Browser integration

The existing native lot owns the history alongside its texture cache. A validated `PlayerUpdate::Checkpoint` advances a checked presentation generation, resets pose history, and recreates the disposable viewport without resetting the user's camera/floor/wall controls. A same-bank checkpoint retains safely decoded textures; a bank/connection replacement keeps the previous texture-lifetime fencing.

Receipt acknowledgement, acceptance versus completion, the independent fifteen-second deadline, Unknown-result retention and the no-auto-retry policy from #31/#34 are preserved. Neither sampling, texture completion nor an ended animation can settle an action receipt. Preview and the legacy refresh lane remain unchanged; native mode is still explicit opt-in.

## Verification

The new native regressions exercise ending and replacing clips, rotation-only channels, carry removal, same-tick blending, hidden avatars, generation/bank/source isolation, stale preparations, failed atomic apply, departure, checkpoint clear, working-set limits, texture wakeups and invalid clips. They use the existing original-format synthetic resource fixtures; they do not certify original human artwork or complete walking/contact fidelity.

The initial history specifications ran against the previous stateless behavior: nine of eleven failed, including the actual snap-to-bind and partial-channel witnesses. All fifteen final history regressions and the existing twelve native avatar cases pass after the correction. The actual WASM call-site check separately caught a `StoredValue::update_value` return-type error; it was corrected to preserve the fallible apply result rather than discarding errors.

The real browser witness `tools/native-browser/verify-pose-retention.mjs` uses a test-only non-looping native timeline and the unmodified content-loading path. It first requires visible movement, then checks the source authority's plateau and `EndReached` state and requires identical canvas pixels across the clip ending and subsequent idle ticks. It exercises camera changes, a real source action/receipt, narrow layout, disconnect freeze, explicit checkpoint reset and route disposal. It never changes the browser clock, injects presentation state/CSS, or substitutes a JavaScript character for the compiled native player.

The permanent read-only workflow runs this witness in addition to all existing action, avatar and receipt journeys, and retains exact source/build/report/screenshot hashes. A passing native compile is not treated as passing browser acceptance. Local browser navigation was denied by the environment before application startup; browser execution evidence must come from the named hosted runs on the published tree.

From the repository root, using the pinned toolchains and locked dependencies:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown --lib --bin wonderland-web-shell --locked -- -D warnings
node --test --test-reporter=tap apps/web-shell/scripts/native-*.test.mjs
node --test --test-reporter=tap crates/audio-runtime/browser/*.test.mjs
(cd apps/web-shell && trunk build --release --locked)
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
cargo run -p wonderland-web-shell --example native_avatar_resources --locked -- target/native-avatar-fixtures
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install --with-deps chromium
node tools/native-browser/verify-pose-retention.mjs
```

The resource generator refuses to overwrite its output files; use a fresh fixture directory or reuse the already generated identical files. Evidence is written outside tracked source. No original resources, fonts, dependencies, configuration, user saves, authoritative runtime semantics, parent branches or other swarm rendering changes are modified.

## Remaining acceptance

Original human-art cohorts, head seeking, source-complete bone/SLOT/container attachments, pet/custom rig and TS1 outfit support, synchronized gameplay audio and production distinct-player services are still outstanding. Full original FSOv/VMNet conversion and the other Buy/Build, property, social and object interfaces remain separate work. This is the retained-pose correction, not the complete game, full animation parity, physical-device qualification or independent approval. No merge or deployment is performed.
