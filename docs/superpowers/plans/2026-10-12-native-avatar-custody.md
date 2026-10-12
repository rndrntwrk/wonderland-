# Native avatar custody and source-faithful admission

Continuation of Task 4 in the approved full FreeSO Rust/browser rewrite. This
plan does not reduce the parent scope or enable a deployment. Account storage
alone does not establish playable avatar admission.

## Existing boundaries to preserve

- AccountAuthority authenticates and persists account-owned appearance records.
- LotCoordinator commits accepted batches under host-incarnation and predecessor
  fences. A cached ownership read does not supply a controller fence.
- Native Rust host prepares tentative ticks and exposes only resolved commits.
- `revalidate_action` checks server-derived principal and exact EntityRef under
  the authority lock. A packet cannot choose its own principal or actor.
- AvatarPersistState already preserves portable state. Current Spawn defaults
  are insufficient for joining with saved appearance, progression and ownership.
- The browser's native entry still depends on legacy LotReady. Replace that
  dependency with a real native provider without fabricating Aries readiness.

## Source obligations

Reference pin: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

- `TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetSimJoinCmd.cs`: server-only
  join, template/custom GUID, mailbox-relative placement/fallback, permissions,
  appearance, roommate reconciliation and player-joined callbacks.
- `TSOClient/FSO.Server/Servers/Lot/Domain/LotContainer.cs`: portable state,
  skills/jobs/motives/relationships. Avatar budget is a mirror, not the ledger.
- `TSOClient/FSO.Server/Servers/City/Handlers/RegistrationHandler.cs`: ordinary
  zero starting balance and three owned CAS outfits, with source gender-specific
  sleepwear/swimwear. Privileged grants require server moderator authority.
- `TSOClient/FSO.Server.Database/DatabaseScripts/fso_avatars.sql`,
  `fso_objects.sql`, `DA/Avatars/SqlAvatars.cs` and `VMAvatar.cs`: avatar IDs occupy
  the positive range below 16,777,216; larger IDs participate in object ownership.
  `uint.MaxValue` is the bank, not a normal avatar. Existing Rust social logic and
  WLB1 keep the same source identity distinction.

## Increment 1: source-compatible identity and durable initial ownership

- [ ] Bound new avatar allocation to 1..16,777,215 while keeping exact decimal
  external serialization. Accounts, outfits and epochs retain their wider types.
  Reject exhaustion before any avatar, wardrobe, ledger or operation row commits.
  Never truncate, hash, recycle or allocate a separate identity per lot.
- [ ] Preserve idempotent old successful create receipts even after exhaustion.
  Reject imported out-of-range avatars at admission; do not silently remap them.
- [ ] Persist initial wardrobe ownership and zero-balance ledger exactly once
  with avatar creation. Catalog availability does not establish outfit ownership.
- [ ] Run real SQLite/workerd creation at the identity boundary, lost-reply
  retries and concurrent creation. Verify no partial ownership or receipt writes.

## Increment 2: exclusive durable control and replayed Rust admission

- [ ] Account authority records one location/control claim per avatar, binding
  operation ID/digest, session identity and revision, controller generation,
  content/protocol release, target lot and expiry. Different-body retries fail.
- [ ] A second session cannot spawn a duplicate or inherit equal control. Derive
  a distinct server principal per control generation rather than reusing account ID.
- [ ] Add a bounded, versioned Rust admission transition that installs source-
  derived portable state and original join semantics. Record exact EntityRef and
  generation in the committed receipt. No JavaScript simulation or snapshot patch.
- [ ] Atomically check the current lot-side control fence with accepted batches.
  Async preparation cannot bypass revocation. Preserve existing wire byte meanings.
- [ ] Lost admission replies recover the same durable actor/receipt after host
  replacement; never infer success from a prepared but unresolved spawn.

## Increment 3: revocation, expiry and travel

- [ ] Logout, rotation and expiry initiate recorded fence delivery. Do not report
  gameplay revocation complete until lot-side acceptance has established the fence.
  Already committed operations remain resolvable after credentials are revoked.
- [ ] Implement Active -> TransferPending -> Detached -> Admitted. Source detach
  commits portable state and pending operation references with its fence. Account
  claim advances only from that durable receipt; destination consumes it once.
- [ ] Timeouts do not release custody or clone saved state. Delayed source batches
  fail after detach. Recovery resumes the recorded phase and destination.
- [ ] Preserve source outfit reset behavior through DefaultSuits/body_for_persistence;
  borrowed/naked appearance is not converted into a new owned outfit.

## Increment 4: actual browser entry and regression proof

- [ ] Connect the native authenticated provider to character grid, map, room entry
  and world controls, preserving legacy and device-preview modes explicitly.
- [ ] Two isolated real accounts: avatar selection, independent appearances,
  duplicate-controller refusal, reconnect, logout/rotation/expiry races, changed-
  body retries, lost post-commit response and replacement-host recovery.
- [ ] Interrupt each travel boundary. Assert one controller, one actor, unchanged
  wardrobe/inventory/game-currency ownership, and preserved unresolved operations.
- [ ] Review desktop and narrow-touch rendered flows and actual action execution.
  Mock authority, injected graduated/admitted state and legacy fake readiness do
  not count as successful-path evidence. Fault injection stays test-only.

## Publication gates

Independent review of cross-object serialization, operation retention, content
qualification and public-auth policy remains necessary. Cloudflare deployment,
new grants and provider spending require their separately applicable approvals.
This plan is not an implementation or release claim.
