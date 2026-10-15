# Durable player-action decision recovery

Base: PR78, `39983bf3dad370e55f2d2dd30bc036b2347a7740`.
This candidate does not change published PR77/PR78 or deploy anything.

## Root cause

The runner commits the native proposal at the durable coordinator before asking
its Rust host to resolve. The original WLC1 action bytes were held only in the
host's pending candidate, and exact WLA1 outcome bytes were exported after
resolution. A host loss after durable commit but before resolution/export could
therefore retain accepted game state without retaining the browser correlation.
A takeover also creates a fresh browser player scope, intentionally dropping old
runtime state. Fixing storage alone would not fix the visible result.

## Bounded repair

New action proposals carry digest-covered exact action bytes, native-generated
accepted/rejected receipt bytes, typed input hash, and source-owned control
identity. The coordinator records the chosen outcome atomically with the batch
or durable denial. Existing proposals without this metadata remain readable;
absence never implies rejection or acceptance.

A browser recovery capsule retains only pending request metadata and sequence
high-water, not the old runtime. Reimport requires the same authenticated entry,
source/lot/avatar/principal/actor and replicated owned control, plus a validated
new checkpoint. Logout, leave, account/selection change or incompatible control
must discard the capsule. Exact WLA1 validation still checks the original bytes
and accepted tick against restored state.

Read-only action-decision query: `native_action_result`, original decimal `lot_incarnation`,
exact lowercase `action_hex`. The trusted runner derives account, principal,
avatar, control generation and session from the authenticated socket, revalidates
live control, and performs private coordinator lookup. Normal authorization can
record or drain expiry/fence obligations; this route is not globally free of
security-state writes. It never submits the old
action, renews activity, fabricates a result, or infers a result from a checkpoint.
Unknown evidence remains Unknown. An accepted decision means the command was
accepted at that tick, not that the interaction finished successfully.

Identity uses original incarnation + exact control + exact request hash. A
rejected command sequence is not a global uniqueness key: it may legitimately be
reused by a newly created client according to existing native semantics. The VM's
accepted-command sequence checks and existing batch identity remain authoritative.

## Required acceptance evidence

- Real host process killed after durable decision and before resolve/export;
  durable restart/takeover preserves exact action outcome without replay.
- Accepted and rejected decisions, lost replies, duplicate reads and restart.
- Different owner/session/control/action bytes/incarnation cannot retrieve a
  receipt; no game-action execution/replay, custody renewal or activity extension
  in the authenticated lookup route. Normal security expiry/fencing is enforced.
- Browser pending capsule survives only verified same-control takeover; accepted
  receipt cannot appear before the corresponding restored state.
- Missing historical metadata remains Unknown and never causes action replay.
- Existing action, account, journal/archive, host and browser boundary regressions.

## Remaining source implementation queue

Original content blocks faithful original-game and rendered qualification, not
all source implementation.

Movement: original `VMNetGotoCmd` creates source object GUID `0x000007C4` at the
selected source tile/level, selects its original interaction and parameter, then
uses the normal interaction queue/routing. Native terrain picks currently have
no equivalent PlayerAction. Existing generic BeginRoute or teleporting are not
substitutes. Implement the exact source command path; qualify it with its actual
OBJD/TTAB/BHAV and global/semiglobal closure plus routing metadata.

Owned leave: source `VMNetSimLeaveCmd` calls `VMAvatar.UserLeaveLot`. Native avatar
lifecycle already has source routine 8373, interaction 173, 1800-tick forced
removal, request_leave and portable-state extraction. Runtime currently ignores
several cancel/enqueue/timeout/EOD requests and supplies false leave flags. The
durable coordinator forbids removing an existing owned actor, while the account
boundary lacks committed portable-save/custody-release. Wire those existing
source semantics and an idempotent durable save/removal receipt. Missing content
must not be treated as deliberate routine absence to force-delete the avatar.
Authentic leave routines and avatar resources remain necessary for original-game
and rendered end-to-end acceptance.
