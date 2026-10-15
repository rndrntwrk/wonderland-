# Remaining source-native admission dependencies

The scoped callback prerequisite does not implement PrepareAdmissionV2 or actor admission. Keep successful admission fail-closed; the complete original-game scope is unchanged.

## Next source implementation: replicated TSO lot permissions

Original pinned sources:
- `TSOClient/tso.simantics/Model/TSOPlatform/VMTSOLotState.cs`: persisted owner, property category (`11` means community), roommate/build-roommate sets, object limit and exceeded flag.
- `TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetSimJoinCmd.cs:69–132`: community permissions from donor/benefactor membership, portable Apply ordering, roommate reconciliation, overbudget recomputation and owner-object pending-roommate-deletion flags plus scheduler interrupts/light refresh.
- `TSOClient/tso.simantics/Model/VMBuildableAreaInfo.cs:127`: source object-limit ordering and enable/disable behavior.
- `TSOClient/tso.simantics/Model/TSOPlatform/VMTSOObjectState.cs`: owner/donated metadata that join reconciliation consumes.

Implement these as explicit deterministic runtime state/transitions with original-source cases for visitor, roommate, build roommate, owner, admin and community donor/benefactor differences. Include persisted-object ownership flags and affected scheduling/projections. Do not silently narrow to visitor-only or invent empty roommate sets. Snapshot schema changes require explicit bounded migration/refusal and exact recovery validation; do not accidentally alter V1 wire/command meanings.

Then compose source-native admission: choose template/custom avatar, mailbox-relative placement/default fallback, apply exact portable state, reconcile source permissions, register persistent identity, emit join presentation and execute every out-of-world controller callback in source order. Privileged/special actors require their own explicit source coverage, not generic spawn fallback. Bind actor/principal identities to deterministic accepted replay and versioned snapshots.

## Private authority completion before activation

AccountAuthority must export/revalidate the exact reserved avatar, session/source revisions and portable bytes through private trusted bindings. Coordinator installation must allocate bounded unique principals; V2 commit must validate current registry/control revision, lease and covering fences inside the same SQLite transaction as receipt/journal/outbox/cursor writes. Every ordinary tick, including autonomous work, requires fencing. Expiry discovery must persist a typed denial/fence obligation without rolling it back. Detach must run bounded source leave/cleanup progress and preserve unresolved logical operations before custody release. Recovery must compare runtime/coordinator registry and current fence revision before ready.

Only after real Rust/workerd/account/crash qualification may a bounded authenticated native browser adapter issue a fresh exact active grant. No public route, grant, live provisioning, migration, credentials, merge or deployment is part of this prerequisite.
