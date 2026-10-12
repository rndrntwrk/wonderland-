# Native lot host

A real Rust `GameRuntime` authority loaded from a sealed cooked release. Its only transport is a private line-delimited supervisor pipe. It must not be attached directly to a browser or unauthenticated network endpoint: commands include trusted spawning and memory writes.

Start with `wonderland-native-lot-host CONFIG.json`. The strict schema-1 config requires release_dir, independently obtained binding_sha256, lot_file, lot_sha256, mode (`Tso`/`Ts1`), and canonical decimal-string lot_id, authority_epoch, lot_incarnation, seed, utc_start_dotnet_ticks, effect_namespace. The release directory contains binding.json, manifest.json and verified digest-named `.wlp` packs. The pinned lot file uses bounded fixed-integer bincode encoding of `LotModel`, not a default fixture. Paths and files must remain immutable for this process lifetime. Observed symlinks and traversal are refused; this is not a hostile-filesystem sandbox.

The initial and `inspect` messages contain only the committed checkpoint/cursor, sealed release report and effective runtime content descriptor. These two content identities are distinct because GameRuntime attaches interaction tables.

`prepare` accepts one bounded vector of ticks containing server-admitted commands, encoded as fixed-integer bincode hexadecimal. It requires exact binding and predecessor. A separate candidate runtime executes those commands. The returned `proposal` is for coordinator validation/storage only; its ticks and effects are not permission to publish or dispatch. Unknown acceptance retains the candidate and prevents a successor. A matching `committed` digest and cursor installs it; rejection discards it. `lookup` and same-input retry return the existing pending/terminal result. Consumers deduplicate terminal results by batch ID/digest; repeated retrieval is not a new delivery.

Frames retain the existing WLR1 binary encoding. Durable build requests/results/faults and routing callbacks have a tick/ordinal-tagged binary supervisor-events lane. They remain speculative until commit. Effect dispatches use bincode hex so their 64-bit operation identities never lose precision in a JavaScript JSON parser. JSON lot/epoch/incarnation/tick/batch identities are canonical decimal strings.

Bounds: 1 MiB input lines, 256 KiB command bytes, 16 ticks × 128 commands per proposal, 8 MiB checkpoints, 16 MiB encoded proposal payload, 4,096 receipt entries and 32 MiB total retained terminal receipt bytes. Capacity exhaustion fails closed; receipts are not silently evicted.

## Actual process verification

Build the existing `runtime-bridge` executable, then `cargo build -p wonderland-native-lot-host --all-targets` and `node tools/hosted-game/verify-host-boundary.mjs`. The test release is explicitly authored, passes through the production cooker, and has its source inputs removed. The new service itself contains no fixture fallback. The test checks real behavior mutation and matching accepted-wire replay, speculative isolation, pending/commit/reject/retry/lookup, changed input, malformed identities and bounded inputs.

## Pinned restart recovery

An optional `recovery` object supplies checkpoint_file, checkpoint_sha256, tail_file, tail_sha256 and expected_cursor. Each file is bounded to 8 MiB and independently hash-pinned. The existing runtime snapshot restore verifies its content/lot/epoch binding. The tail must be a WLR1 accepted-tick packet with at most 64 frames; every resulting state hash and the final committed cursor must match. Startup emits no ready state on any mismatch. This restores the same epoch only; it is not an ownership takeover or a lease grant. A coordinator must fence the previous owner before exposing a replacement.

## Release limitations

The private companion `services/native-edge` now provides a locally qualified durable journal and verified checkpoint storage. Authenticated account grants, ownership fencing, browser publication gateway, automatic wall-clock tick scheduling and EOD/economic providers remain unfinished. The process accepts supervisor decisions because the pipe is the trust boundary; it does not prove an external database committed. Before deployment Task 3 must persist proposals/results and fence incarnations, and Task 4 must authenticate player admission. The process journey also exercises nonempty dialog effects and durable build requests with identities beyond JavaScript’s safe integer range. Full original-content coverage and actual original-BHAV process qualification remain separate acceptance gates. Never substitute this local service test for hosted gameplay completion.
