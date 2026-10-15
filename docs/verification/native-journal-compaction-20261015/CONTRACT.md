# Durable journal and receipt compaction contract

The previous lifetime limit was 4096 accepted batches / 64 MiB of coordinator journal data, plus 4096 / 32 MiB of native host reply cache. At one batch per 30 Hz tick, the count limit stopped an otherwise healthy session in about 136.5 seconds. Merely publishing checkpoints did not reclaim either cache.

This increment changes storage tiers, not retention policy. Every exact historical coordinator batch receipt, pending event/effect reference and canonical accepted payload remains retained. Every successfully archived host terminal record retains its original input identity and full terminal reply before the host may retire it. There is no TTL, oldest-entry eviction, acknowledgment fabrication, or external effect replay.

## Coordinator journal

- Existing SQLite `batches` keeps immutable digests, exact terminal receipts and original byte lengths. Separate `journal_live` records only hot payloads. Its 4096 / 64 MiB bound remains safe backpressure during unavailable/failed checkpoint archival, not a lifetime receipt limit.
- A one-time synchronous transaction migrates existing batches into the hot index. Restart never repopulates compacted entries.
- Checkpoint publication still requires exact committed binding/cursor/checkpoint digest, bounded R2 object upload plus SHA-256 readback, then a final synchronous check of current binding, recovery fence and controlled V2 registry/revision/permissions. No asynchronous archival can weaken these checks.
- After publication, covered hot batches are packed into immutable content-addressed R2 objects, at most 64 batches and 16 MiB per object. Each canonical accepted payload is independently checked against its original digest, receipt and binding. Exact byte offsets identify each payload; full object SHA-256 and size pin the archive.
- Existing objects must match their immutable key; new or existing archive bytes pass readback before the synchronous SQLite transaction inserts archive pointers and removes only the redundant hot chunks/index entries. A failure before pointer commit preserves the live payload; transaction failure rolls back all pointer/chunk changes. An orphan R2 object is harmless and is never garbage-collected here.
- `readBatch` becomes asynchronous but returns the same complete payload. Archived reads verify bounded whole-object bytes and the per-batch digest/receipt. Missing or corrupt referenced objects fail closed. `getBatch` and duplicate commits return the exact original receipt independently of archive availability.
- The complete event/effect lanes remain in archived payloads. Outbox rows are never deleted, altered or marked acknowledged. Existing pending logical operations survive archival and owner replacement without being reissued as new work.
- Recovery selects only the bounded accepted tail after its checksum-pinned checkpoint, preserving decimal-string identities and controlled metadata. A completed, verified takeover can tier its exact old-incarnation tail too. Historical recovery grants/completions remain exact; their former 4096 lifetime cap is removed.
- Controlled denial receipts are durably retained for both lease expiry and fence-drain rejection, without silently dropping outcomes at 4096.

## Host custody transfer

The private host exports a bounded checkpoint-bound terminal-receipt prefix containing original request input hashes and complete replies. The coordinator verifies the exact published checkpoint and accepted decisions/payloads, retains the complete export in immutable readback-verified R2, and atomically publishes bounded SQLite receipt indexes. Only its typed exact acknowledgment authorizes native cache retirement. A retired batch ID never executes again; historical lookup resolves through the durable archive. Host and runner details/tests are maintained with their source changes.

## Honest limits and scope

Archive and receipt metadata/storage continue to grow. This is lossless compaction of hot caches, not unlimited provider capacity or a data-deletion policy. Provider/storage failures fail closed with live data or previously verified archives retained. The bounded checkpoint/tail and proposal profiles remain; no unbounded memory/SQL values were introduced. Outbox dispatch/acknowledgment and eventual retention/garbage collection remain separate work. No deployment, new infrastructure, grants, public ingress or asset acquisition is included.

Qualification distinguishes actual workerd SQLite/R2 storage-contract vectors from actual Rust-host/gameplay witnesses; synthetic storage vectors do not certify VM or asset fidelity.

The archive tables are a new durable layout. Rolling back to coordinator code that only understands hot chunks is unsupported after compaction; retain an archive-aware reader or explicitly rehydrate from verified archives before such a downgrade. This source-only increment does not perform a deployment or storage downgrade.

The pre-existing crash window between durable batch commit and host resolve/export does not contain a durable copy of the original action request/correlation or full terminal host reply. This increment does not reconstruct those missing fields: accepted state/effects remain in the coordinator journal, while unresolved player results remain `Unknown` and are never automatically replayed. A successfully archived host record is exact; this is not a claim that every pre-export action correlation is crash-exact.
