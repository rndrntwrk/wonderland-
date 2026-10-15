# Source-native lot permission prerequisite contract

## Tested base and boundary

Branch `work/native-lot-permissions-20261015` starts at PR75 head `4eba2030bd72daeec2d02721d838b4b4187635a9`, whose twelve CI workflows passed. This separate increment begins with a pure deterministic source model and reconciliation reducer. It does not yet install replicated lot state, change snapshots or accepted command tags, admit an actor, authenticate a caller, grant access, or provision a live lot.

The complete original-game goal remains unchanged. All five source roles and community donor/benefactor behavior are required. This contract is a prerequisite for the replicated-state dependency, not a claim that dependency is already complete.

## Original source inventory

`source-provenance.json` records byte equality with FreeSO commit `4c6b3e8f5835b228723caea3c9f683c62f244f73` for all eleven relevant files.

- `VMTSOLotState.cs`: owner ID, category, packed size, roommate and build-roommate sets, computed object limit, exceeded flag and donation limit (default 2,000). The original serialized lot record omits the computed limit, exceeded flag and donation limit; a future native persistence design must identify derived versus imported state explicitly. Category 11 is community: these sets mean donors and benefactors. Owner ID zero is a real source sentinel.
- `VMTSOAvatarState.cs`: Visitor=0, Roommate=1, BuildBuyRoommate=2, Owner=3, Admin=4. Reuse the existing native `AvatarPermissions` enum.
- `VMNetSimJoinCmd.cs:66–132`: community role adjustment precedes portable-state application; membership reconciliation follows it. A joining admin does not alter membership. Existing roommates do not have their build membership repaired by this join path. Only a membership-count change triggers overbudget recomputation and owner-object pending-deletion changes.
- `VMBuildableAreaInfo.cs:104–113,127–170`: category 255 has limit 10,000; community has limit 100; other lots index the 14-element per-person table by low size byte plus floor byte and multiply by at least one roommate. Persisted multitile groups are ordered by base object ID. Over-limit parts get bit 8; bit 32 is withheld while a group is used by an avatar stack, the part is occupied, or the group is a portal. Community skips these flag rewrites entirely, but computes its exceeded flag from non-donated group count.
- `VMObjectQueries.cs`: persistent-group count is distinct from entity/part count. Community user-object counts exclude donations based on the base object's flag. A persistent ID zero is not registered in the persistent-group dictionary.
- `VMGameObject.cs`: disabled bits include ObjectLimitExceeded=8, PendingRoommateDeletion=16, ObjectLimitThreadDisable=32. Ongoing inventory-return dispatch and object ticking have separate behavior; a reducer must not claim those effects executed.
- `VMTSOObjectState.cs`: per-part owner and donation flag; donation changes value and all parts, then recomputes limits.
- `VMChangePermissionsCmd.cs`: explicit permission/owner-switch commands have different reconciliation and donation exemptions from join. Do not replace join with this broader algorithm. Permission verification and database commands are separate from the deterministic result.
- `LotContainer.cs:725–773`: database refresh overwrites non-community roomie/build sets, ignoring pending roomies; community preserves donor/benefactor sets unless the owner changes, then clears them and inserts the new owner (including the source zero sentinel).

## Deterministic reducer requirements

The inputs must explicitly supply the source lot data, incoming role and persistent avatar identity, persisted multitile groups, per-part source identity/ownership/donation/disabled/occupied fields, and active-stack/portal facts. No absent set, group or owner may be invented from account identity. Accepted inputs produce a new state and ordered effect obligations; rejected inputs leave the caller's state unchanged.

Join ordering is: resolve community membership role, apply portable avatar state in the eventual runtime adapter, reconcile membership, recompute limits only when required, then update persisted objects owned by the joining avatar. Owner-object processing refreshes light even when its deletion bit is unchanged; it requests an interrupt only when flags change. `VMNetAvatarPersistState.Apply` sets avatar.PersistID to the portable PersistID at line 227. Consequently the join owner-object filter uses that portable identity after Apply, even if command ActorUID differs. The reducer avatar identity denotes portable PersistID. The future admission adapter must separately validate its binding to command ActorUID and principal; it must not substitute ActorUID into this source filter.

Source oddities remain observable: Owner does not automatically rewrite the lot's owner ID during join; Admin does not join/remove sets; an existing member's build flag remains untouched; community skips overbudget part-flag cleanup. Validation may explicitly refuse malformed/unsupported inputs, but must not silently normalize these behaviors. Deterministic ordering and bounded collections are native safety requirements, not assertions about C# serialization order.

## Native reuse and integration still required

The current runtime already has avatar permissions, entity disabled flags, `WorldObject.owner: Option<PersistentId>`, `multitile_group`, and use/for-sale projections. Build commit populates world ownership. Reuse or reconcile these fields rather than introduce a second competing owner authority. It lacks the complete lot-role model, source donation metadata and persistent-group registration/query semantics needed for this join path. Source `VM.cs:AddToObjList` (487–501) keeps VM entities sorted by local object ID; object refresh/interrupt obligations must retain that order. Generic `in_use` cannot silently substitute for the specific original avatar-stack, occupied and portal distinctions.

After the pure model is qualified, the next slice is explicit replicated adoption: add validated lot and persistent ownership/donation state; derive query facts from live native objects/threads; append bounded accepted transitions; apply scheduler/light obligations; and test authority/replica hash equality, duplicate suppression and recovery. Current simulation schema 2 already has an explicit schema-1 migration. A new layout needs a separately reviewed schema increment and a bounded old-schema migration to **uninitialized permission state**, or explicit refusal. It must never migrate absent authority into an empty visitor-only lot and treat it as admission-ready.

Private trusted account export, role provenance, exact actor/principal binding, leases/fences, registry-bound recovery, full source-native join/controller ordering, cleanup and eventual browser grants remain subsequent requirements. No public route or live activation follows from passing this reducer's tests.

## Qualification boundary

Tests execute the real Rust reducer with source-backed cases, including adversarial malformed inputs and permutation determinism. They do not execute the original C# methods. The current cloud toolchain lacks the built original assemblies and .NET/Mono compiler/runtime required for that differential; the prior green original-VM workflow covers its existing corpus, not these new cases. Keep a future original-method differential gate explicit.

## Bounded projection representation

The pure model represents game and non-game parts explicitly. Every registered group counts; only game parts receive disabled-bit or owner-deletion changes. A non-game base contributes no donated flag and fails the source base-is-game portal guard, even if another part is a portal. This preserves the original method rather than silently dropping mixed groups.

Validation bounds membership, object and group collections to 32,767 and rejects unknown flags, invalid local IDs, duplicate/missing members, zero registration keys, negative cached capacities and ordinary-category invalid size indices. Per-part persistent IDs must match the registered group key in this bounded projection. Some imported original states can violate that condition because loading registers by base; those states are explicitly unsupported and refused, not normalized or claimed impossible. The future importer must retain that diagnosis or add separately qualified semantics. Zero owner/member IDs and orphan build memberships remain representable source cases. The source's separate community permission-change cap of 100 members is an authorization rule, not a replacement for this reducer's collection bound.

The model's serde derives are for testable deterministic data, not a newly admitted wire/snapshot contract. No durable migration or public input acceptance is implied.

## Independent review

Independent review of code commit `220cf8840b70d38962093d62d62cd03258635eca` found no correctness blockers within the pure projection/reducer scope, independently checked all eleven original file byte matches, and reran all sixteen focused tests and full-workspace formatting successfully. Full workspace acceptance evidence is retained in verification.json: 2052 tests passed, zero failed, five source-oracle/resource-dependent tests remained ignored; all 52 baseline tests and formatting passed.
