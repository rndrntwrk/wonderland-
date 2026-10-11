# Native action outcomes and queue feedback

Continuation of PR #29 at `ce54857000c253249f0bdd7b880038f9227e6cbc`.
This increment preserves the existing action-first Sim → city → lot journey,
source identities and original FreeSO lane. No renderer branch, original assets,
dependency versions, shared CSS or player saves are replaced.

## Player-visible behavior

The native source-actions panel distinguishes **Accepted by the server** from a
runtime terminal outcome: completed, failed, stopped, cancelled or unavailable.
It shows a recent-activity history, bounded to 32 entries for the admitted actor.
A receipt is never interpreted as evidence that a behavior finished successfully.

Queue rows remain keyed by the actual action ID while their label, active state,
cancellation state and disabled controls derive reactively from the current
accepted projection. Cancelling an item does not optimistically delete it or its
running sibling. A pending/unknown request blocks additional commands, and
cancellation remains disabled while the source reports cleanup pending.

The queue row reserves room for the existing 36px round cancel control and wraps
long titles inside the card. The correction is local to the native row; the
existing general queue CSS assumed a smaller button. The browser test compares
actual text-line rectangles against the actual cancel-button rectangle.

## Runtime boundary

`NativePlayer::receive_update` returns one of:

- `Checkpoint(ReplicaCursor)`: recovery committed with no historical outcomes.
- `Ticks(Vec<TickOutcome>)`: the exact ordered outcomes after the entire accepted
  batch, its state hashes, and the admitted actor have validated.
- `Receipt(ActionStatus)`: the correlated server admission decision, not completion.

The existing `receive` state-only API remains source-compatible. Presentation
integrators should consume `receive_update` rather than a latest-value event
signal. Empty duplicate batches return no new outcomes. A bad later tick
publishes nothing from the earlier portion of the failed batch.

The native controller handles each delivered message synchronously. Terminal
activity is accumulated inside the player and only changed bounded history is
copied into the reactive UI. Other runtime events are exposed by the API but
are **not** connected to gameplay audio, headlines or avatar animation in this
increment; those consumers still need integration.

The activity reducer filters the exact admitted actor generation and ordered
queue-event sequence. A cancellation request is not terminal. `Finished` supplies
its real success/failure/abort result; `Removed(Finished)` is cleanup and cannot
create a second success. Repeated terminal events do not append duplicate history.
Matching reconnects retain observed history without rebuilding it from replayed
ticks. Closing/replacing the player clears its history.

## Verification

The adjacent machine-readable record holds exact commands, counts, source hashes,
release identity, screenshots and dispositions. Tests cover lossless accepted
outcomes, whole-batch failure, checkpoint/receipt distinction, duplicate delivery,
actor/sequence fencing, bounded history, nonterminal cancellation and close/recovery.

The actual optimized browser application runs against a loopback gateway and
native Rust authority, not a replacement HTML client. It checks an unchanged
source needs action after three intervening server ticks, explicit completion
feedback, a second browser observing accepted state, an unknown result with no
retry, exact queued-item cancellation and retained history across reconnect.
Desktop, narrow portrait, small portrait and short landscape viewports test
controls, bounds and cancellation-label separation. Screenshots are unedited.

### Correct source needs oracle

The original cursebook BHAV4107 starts with `RandomNumber(3)` and fills one of
two groups: Energy/Hunger/Hygiene or Bladder/Social/Fun. Earlier tests assumed
only the first group, which made a valid source execution appear to fail. The
fixture now resets all six needs to 50 and accepts exactly either
`[100,100,100,50,50,50]` or `[50,50,50,100,100,100]`, in that group order.
Both browser contexts must observe the same actual result. A native regression
executes 32 invocations through the real browser fixture and requires both
original branches; it was observed failing with the former three-need reset.
No original bytecode, RNG, simulation semantics or required outcome is changed.

The cancellation fixture adds **authored, test-only idle/wait routines** to the
unchanged-source needs fixture. They are intentionally labelled “Wait (test
harness)”; they are not evidence of original-object-family compatibility. The
fixture is only available through the explicitly opted-in example executable.
Both browser contexts use the same declared test Sim; neither distinct player
identity nor a multiplayer capacity limit is inferred from that test.

### Reproduction

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
cargo fmt --all -- --check
node --test apps/web-shell/scripts/native-socket.test.mjs
node --test crates/audio-runtime/browser/*.test.mjs
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
(cd apps/web-shell && trunk build --release --locked)
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install --with-deps chromium
node tools/native-browser/verify.mjs
```

Use the repository's pinned compiler, dependency lockfiles and Trunk 0.21.14.
Cargo and Trunk can use `--offline` when their dependencies/tools are cached.
`WONDERLAND_NATIVE_QA_OUTPUT` selects an isolated evidence output directory.
The existing read-only Native browser workflow exercises tracked source directly;
no generated call sites or transfer tooling are required in the final tree.

## Remaining scope

This is action-completion/cancellation feedback, **not full playable-client parity**.
It does not supply original avatar/object models, walking/contact animation,
original FSOv/VMNet translation, native production admission/content services,
durable commerce, complete Buy/Build/property/social/object dialogs, game-synced
audio, physical-device or deployed distinct-player multiplayer acceptance.
No merge or deployment is included.
