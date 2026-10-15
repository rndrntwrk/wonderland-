# Replicated lot permission metadata: initialization and migration

## Scope

This increment starts at draft PR76 head `a6c3b73f66620f4eefd92444ca9059de90ba11b3` and integrates only the reviewed source lot metadata, not the disconnected object-query projection. The complete source role/community model is retained. Existing native world ownership and group topology remain authoritative; live stack/occupation facts are never stored as a duplicate projection.

The initializer is a trusted accepted-journal operation, not authentication, source-role discovery, a public admission API or an actor grant. It does not run join reconciliation, modify avatar roles, dispatch inventory returns, apply scheduler/light obligations, or claim a playable lot. Full source-native admission remains dependent on the explicit source inventory in the prior contract.

## State and accepted transition

Append an explicit `Uninitialized` / `Initialized(LotPermissionState)` field to SimState. New runtimes start uninitialized; TS1 must remain uninitialized. Do not interpret absent authority as an empty visitor-ready lot.

Append initialization at command tag 27, preserving tags 0–26. Only a valid TSO state with uninitialized metadata may initialize. Validate the supplied metadata before assignment. A second initialization, even identical metadata in a later tick, rejects atomically; the existing same-tick accepted replay mechanism handles duplicate delivery. Two initializers in one tick reject the whole candidate tick.

Preserve the source's full packed size, zero owner/member identifiers, orphan build memberships and supplied cached values. The original lot serializer omits ObjectLimit, LimitExceeded and DonateLimit. This command therefore imports explicit trusted native metadata; it is not a generic FSOv importer. Recomputing omitted original-save caches requires the later complete persistent-object query, not a fabricated empty world. Membership bounds, cached-capacity bounds and ordinary-category size validation remain the reviewed model's explicit refusal rules.

## Snapshot compatibility

Increment simulation schema 2 to 3 while preserving envelope format 1. Schema 1 migration appends its existing two empty-map encodings plus the canonical uninitialized marker; schema 2 appends only that marker. Verify the original header/payload schema match before rewriting. Refuse unknown versions, noncanonical data, malformed discriminants, trailing bytes and oversized inputs. Check the original checksum and declared payload size before migration; keep bounded allocation and complete semantic graph validation afterward. Finally check the fully reconstructed current state against the global current-state serialization cap: accepting a legacy input near 64 MiB must not produce a state that immediately fails hashing or snapshot persistence. This applies after schema-1 queue reconstruction as well as the schema-3 marker append.

Retain last_tick_digest, completed tick, epoch and owned state. Migration does not synthesize an accepted tick or reset duplicate history. State hashes change because the layout deliberately changes; compare peers under schema 3 rather than claiming byte-identical old hashes.

The prior schema-1 fixture that stripped 16 bytes from the current snapshot requires explicit correction or a frozen legacy encoder. Tests must demonstrate both schema-1 and schema-2 migration, preserved interaction queues, restored duplicate suppression and subsequent initialization.

## Required acceptance

- Fresh TSO and TS1 are explicitly uninitialized.
- Full metadata survives authority/replica accepted replay and snapshot recovery.
- Same-tick duplicate delivery has no effects, including after restore.
- Later identical/conflicting initialization, invalid metadata and TS1 initialization fail atomically.
- Schema-1 and schema-2 migration preserve prior owned state while leaving permissions uninitialized.
- Header/payload mismatch, malformed marker, truncation, trailing data and huge collection hints reject.
- Existing command encodings remain stable and the new command is appended.
- Full workspace, formatting, source-boundary provenance and independent review remain required before publication.

## Next implementation

Add authoritative source donation and persistent-group registration metadata while reusing WorldObject.owner/group topology. Derive distinct avatar-stack use, occupied and portal facts from live state; refuse unknown owner rather than converting None to zero. Then apply the reviewed reducer through bounded accepted transitions, update native disabled flags/revisions and execute qualified scheduler/light effects. Snapshot validation must bind every metadata reference to the real entity generation and group. Only afterward compose portable apply, complete source join/controller order, actor/principal registry and private admission/fencing/recovery. No live grants or deployment are part of this increment.
