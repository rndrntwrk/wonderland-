# Lossless browser VM ingress and live-session acceptance

This increment is based on PR #22 at `02c0b66e569e0702a640b84bfe4c3ae742fc5070`,
which is stacked on the published PR #21 layout/profile repairs. It preserves
those repairs, the original source, the preview and saved player data.

## Fixed delivery boundary

Previously `ConnectedUi.latest_vm` held one `Option<VmDelivery>`. If a snapshot
and subsequent ticks arrived before the world effect consumed that slot, only
the latest delivery was available. The regression's negative control shows the
lost source snapshot using unchanged original v38 golden bytes.

The socket now retains accepted deliveries in `VmInbox`. `vm_pending` is only a
reactive wake-up. The active world effect drains every pending delivery in its
socket order exactly once, even when wake-ups are coalesced. The rendering
signals may still coalesce expensive presentation work; transport input is not
discarded to achieve that optimization.

The inbox binds to the authenticated browser epoch, source epoch and lot
incarnation. Ordinary same-scope status updates preserve its backlog. Login,
logout, socket replacement, source replacement and owner cleanup drop pending
payloads. Old-scope producers/consumers cannot consume a newer lot's backlog.
The existing socket-generation fence remains in force.

Equal direct messages are retained. Only the original decoder/runtime may
classify a tick as a duplicate; the transport does not deduplicate matching
bytes or text. The source StateSync next-tick convention is unchanged.

Default backlog admission is 256 deliveries and 16 MiB of retained payload
capacity. This is a transient buffer limit, not a player, Sim, object or lot
limit. The implementation charges vector capacity, checks arithmetic and
allocation failure, and never evicts an earlier message to fit a newer one.
Overflow drops the incomplete backlog and latches recovery. The browser closes
the failed transport, clears its live authority and presents Reconnect. It does
not retry an unconfirmed write or pretend the last rendered world is current.
Malformed, wrong-property and unsupported-clock source input likewise enters
explicit recovery rather than continuing after an unread stream prefix.

## Native live-session evidence

Five additional tests use `LiveReplica` and the actual A/B `GameRuntime`, not a
parallel simulator. They execute BHAV 4110 from the unchanged checked-in
`Casino_2-Tile_Bar_CC.iff` through a declared four-attribute object definition,
interaction table and principal grant. These metadata are a harness; they do
not qualify the entire casino object or an account authentication service.

The tests establish that preparing an action does not mutate a replica;
server-accepted execution produces the same final attributes, queue completion,
projection and snapshot on two replicas; duplicate delivery does not repeat
completion events; menu selection can be revalidated after an ordinary tick;
revoked access is rejected; cancellation can remove an action before execution;
a bad later authority hash rolls back the complete batch; and reconnect can
replay the accepted tail without publishing historical events again.

The native checkpoint protocol remains **distinct** from FreeSO FSOv/VMNet.
This change does not translate original snapshots into A checkpoints or wire a
new native server into the existing legacy connected lot. Continuous original
simulation, full live content/actions, provider and specialist interface
integration remain in the [capability map](player-capability-map.md).

## Verification

The exact source, pinned compiler and public registry inputs were recovered
from the repository's verification artifact. Both downloaded ZIP hashes and
all inner archive hashes were verified before use. Local validation used
Rust 1.99.0 offline, without replacing dependencies or changing `Cargo.lock`.

| Check | Local result |
| --- | --- |
| Full native workspace | 1,345 passed; zero failed; five existing optional gates ignored |
| Inbox regression suite | 10 passed; observed failing before implementation |
| Original decoder/queue boundary | 3 passed, including the old single-slot negative control |
| Native original-action live sessions | 5 passed |
| Browser audio Node suite | 59 passed; zero failed or skipped |
| Strict all-target native Clippy | Passed |
| Strict WASM web-shell Clippy | Passed |
| WASM application check | Passed |
| Formatting and whitespace | Passed |
| Original `TSOClient/`, `Other/` and lockfile bytes | Unchanged against the captured source |

The focused tests are included in the workspace total, not additional to it.
The inherited PR #22 formatting failure and a strict-Clippy nested-condition
warning were corrected without altering replay behavior. One new test's
integer-parity expression also needed the pinned Clippy's preferred spelling.
The WASM build tools retain the existing `proc-macro-error2` future-compatibility
notice; it is not a new project lint failure.

[Machine record](vm-ingress-verification.json) retains source identities,
commands, local log hashes and the distinction between local and hosted checks.
The new adversarial browser runner is provided below. Its execution/release
status must be read from that record; a source-target compile is not browser QA.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown --locked -- -D warnings
cargo fmt --all -- --check
node --test crates/audio-runtime/browser/*.test.mjs

# After building the actual browser bundle and the owned loopback fixture:
(cd apps/web-shell && trunk build --release --locked)
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
node apps/web-shell/scripts/check-vm-ingress.mjs
```

The browser runner delays the first two actual VM callbacks into one task and
checks that the source world still appears at desktop and narrow viewports.
It then overloads that isolated connection to check explicit recovery and
removal of the stale view. Its scheduler changes delivery timing only; original
payloads are not rewritten. It uses fresh storage, the test-only account and
loopback native peers, never production accounts or user saves.

No merge, deployment, physical-device result or real-server multiplayer
acceptance is implied by this increment.
