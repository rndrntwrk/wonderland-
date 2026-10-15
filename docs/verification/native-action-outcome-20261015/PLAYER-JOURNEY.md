# Remaining integrated player journey

The delivery target remains one actual Rust/WASM browser application with two
real signed-in users: select their own avatars, enter one shared lot, move,
interact, leave/save and reconnect to persistent results. Test harnesses qualify
boundaries only. No original-content gameplay claim follows from these tests.

## Already connected

- `services/native-edge/src/account/authority.ts` and
  `apps/web-shell/public/native-account.mjs`: real account creation/sign-in,
  owned avatars, source-based portable exports and controlled entry.
- `tools/native-browser/owned-demo.mjs`: same-origin account/owned-avatar adapter,
  private Rust host, authoritative serial tick/action decisions and authenticated
  browser transport. No fixed actor or public supervisor pipe.
- `services/native-lot-host/src/{runtime,player}.rs`: source-ordered owned
  admission, trusted actor mapping, Invoke/Cancel, checkpoint/replay/takeover.
- `apps/web-shell/src/native_lot.rs` and game-runtime live wire: accepted state,
  source action menus, avatar/world rendering, recovery and exact receipt status.
- PR78's checkpoint journal/archive plus exact host reply custody remove the old
  short-session hot-cache limit without dropping prior decisions or outbox data.
- This candidate closes the durable-commit/pre-export action decision gap for
  new action proposals and a browser retaining the same owned session capsule.

## Source work still required

1. Terrain GoTo: port the original `VMNetGotoCmd` through native player command,
   accepted transition and UI tile picking. Create original GUID `0x000007C4`
   and enqueue its actual selected interaction/Param0. Use existing placement,
   interaction and route machinery. Do not replace source behavior with teleport
   or expose lower-level route commands to browsers.
2. Leave/save/custody: connect `VMNetSimLeaveCmd`/`VMAvatar.UserLeaveLot` semantics
   to the partially ported avatar lifecycle, cancel/leave queue, blocking timeout
   and EOD disconnect. Persist source portable state and accepted actor removal,
   then idempotently release account custody through a durable receipt. The
   current runner's leave refusal remains until that chain is qualified.
3. Installation-to-playable manifest: reuse asset-cooker, resource providers,
   content resolution and existing WASM renderers. Connect actual installation
   selectors and dependency closure into a pinned release/appearance/source-lot
   manifest. The cooker currently expects explicit declarations; it does not
   automatically turn an installation archive into a complete playable release.
   Effective global/semiglobal patch selection and tuning, object placement/
   footprint/group/routing metadata, avatar outfits/skeleton/animations and
   source join metadata must all be bound and validated.

## Data-dependent acceptance

Authentic content acquisition/execution permission and resulting byte coverage
are separate prerequisites. A reachable archive is not itself a qualified
manifest; missing assets are not a reason to halt independent source work.
Validate imported provider closure first, then run the existing two-account
browser acceptance path on the real application. Require both rendered owned
avatars, source movement, an original object's shared state change, leave/save,
and reconnect after a real host loss. No test-authored routine, replacement app
or inferred successful action satisfies this acceptance.
