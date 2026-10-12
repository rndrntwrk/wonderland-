# Native durable coordinator increment

Continue Task 3 of the approved full-game rewrite. Use actual local Cloudflare workerd/Miniflare SQLite-backed Durable Objects and R2, not an in-memory SQL substitute.

- Journal exact content-bound native proposals, terminal receipts, accepted cursor and pending event/effect references atomically.
- Recompute canonical Rust-compatible payload digests before acceptance; recheck predecessor inside a synchronous storage transaction after hashing.
- Split large journal payloads into bounded SQL chunks. Reject growth at explicit limits until qualified compaction exists.
- Store content-addressed R2 checkpoints with readback before publishing their manifest. Failed/superseded publication may leave an orphan but must not advertise it.
- Exercise the real Rust host plus workerd RPC: caller retry lookup, competing successor, changed payload, process/storage restart, exact replay and checkpoint corruption.
- Keep service private. No deployment, grants or public ingress. Authority takeover/epoch fencing, multi-batch recovery admission, outbox delivery/acknowledgment and compaction remain required subsequent acceptance before release.

Cloudflare API reference: https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/ and https://developers.cloudflare.com/workers/testing/miniflare/storage/durable-objects/ (checked 12 October 2026).
