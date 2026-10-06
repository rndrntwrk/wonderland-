# Ordered VM delivery continuation

Base: PR #21, `dbc08f1f0ce0c65a6cad9ef9f7fd34fef3c2c522`.
Branch: `feat/ordered-vm-delivery`.

## Change

The connected browser previously stored accepted native frames in a single
`RwSignal<Option<VmDelivery>>`. That is a latest-value projection, not an event
stream: when several writes are observed by one reactive run, preceding frames
are no longer available to the consumer.

`VmDeliveryQueue` now owns accepted frames in a bounded FIFO. The reactive signal
is only a wake-up notification. `ConnectedLotView` drains each retained frame in
receive order and calls the existing `SourceFrameGate` for each one. Drawing may
still coalesce; source-frame admission must not. Identical native direct messages
are retained separately, because byte equality is not a delivery identity.

The queue binds browser epoch, source epoch and lot incarnation. Lot, actor,
location and session-state transitions clear pending data in the transport.
Repeated unchanged session projections preserve it. Disconnect, reconnect,
login/logout and failed sends clear the queue. Existing callback-generation and
operation-ledger guards remain in force.

The default cap is 256 entries and 64 MiB of retained payload. Each payload is an
exact-sized owned byte slice. Entry allocation is fallible; count and byte limits
are checked before enqueueing. This is a retained-queue limit, not a claim about
peak browser memory: gateway JSON parsing, one in-flight decoded frame, source
snapshots and rendering have separate budgets.

Overload or an empty payload invalidates all pending deliveries and latches a
reconnect requirement. It never evicts a prefix and pretends the suffix is a
complete stream. Binding the same identity cannot reset that fault. The browser
closes the affected transport and leaves unconfirmed actions unknown; it does
not automatically repeat purchases or other user commands. Native decode errors,
wrong-lot updates and unsupported world clocks also stop queued consumption.

## Verification

`apps/web-shell/tests/vm_delivery.rs` covers FIFO burst delivery, identical direct
messages, count/byte limits, fault latching, bad identities, explicit reconnect,
lot/epoch changes and released capacity. Its integration case passes eight empty
ordinary ticks through the actual `SourceFrameGate`, checking all eight admitted
generations rather than only the final tick number.

Run:

```sh
cargo test -p wonderland-web-shell --test vm_delivery --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings
cargo fmt --all -- --check
```

Native/WASM test and build status must be read from the exact published commit's
Browser UI workflow. The current editing container has no Rust/Cargo toolchain;
no local Rust execution or rendered-browser acceptance is claimed. The recovered
baseline source hashes and the generated patch are checked locally. A source
inspection is not a replacement for compiled tests.

## Boundary

This closes the latest-value transport prerequisite. It does **not** implement
original FSOv-to-Rust restoration or VMNet-command translation. Connected lots
remain labelled snapshot views. `SourceFrameGate` and its source-tick semantics
are unchanged. The subsequent live-runtime adapter must consume the same ordered
stream, restore source state faithfully, handle replay/reconnect explicitly and
connect accepted actions, queues, needs, animation and sound. Original snapshots
must not be passed to `GameRuntime::restore` as if they were Rust checkpoints.

PR #21's visual layouts, avatar-picking fix and original `TSOClient`/`Other` trees
remain unchanged. No merge or deployment is part of this continuation.
