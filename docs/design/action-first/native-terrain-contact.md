# Native terrain contact and current-scene picking

Continuation of PR34 at `13580dd2d0e2fefbefb73e4d0011032f80c06fb0`.
The existing admitted avatar resources, animation, action feedback and independently bounded receipt deadlines remain in place.

## Implemented correction

Native entity presentation previously used only `(level - 1) * 2.95` as its height.
It now bilinearly samples the accepted native terrain at the exact 1/16-tile position, subtracts the appearance base altitude, scales by `3/160`, and adds story height.
Avatars retain their unshifted tile-center coordinates. FSOm objects retain their half-tile mesh offset; that offset does not move their terrain contact sample.
Out-of-world entities remain invisible, unselectable, at the inert presentation origin.

Native terrain contains explicit right/bottom boundary vertices. Its center-height grid now averages those same four native corners, using a wide signed sum and integer division towards zero. Imported legacy terrain's wrapped boundary conversion is untouched.

The original rendering visibility rule is `Hidden == 0`, not `Hidden != 1`.
All nonzero accepted hidden values now suppress native rendering and mesh selection.
The actual browser also revalidates a renderer-admitted pick against the latest visible/selectable world object, GUID and entity generation. A valid same-entity selection may survive ordinary tick latency; a hidden/replaced/deleted entity or different source lifetime may not. The existing renderer still owns frame-ticket validation.

This changes presentation only: no VM positions, routing, source behavior, accepted command rules, saved state, runtime hash or dependency version changes.

## Source references

- `TSOClient/tso.world/Model/Blueprint.cs`: `TerrainFactor`, `InterpAltitude`.
- `TSOClient/tso.world/Components/AvatarComponent.cs`: raw avatar position plus terrain altitude.
- `TSOClient/tso.world/Components/EntityComponent.cs`: object mesh/contact offset.
- `TSOClient/tso.simantics/Entities/VMEntity.cs`: `Hidden` rendering setter/load semantics.
- `crates/render-3d/src/lot.rs`: native explicit-corner bilinear geometry.
- `crates/sim-core/src/world/tiles.rs`: accepted 1/16-tile coordinates.

Native explicit boundary interpolation intentionally follows the existing native renderer rather than importing the legacy renderer's edge clamp/wrap rules. Original protocol translation is still separate.

## Reproduction

```sh
cargo test --workspace --locked
node --test apps/web-shell/scripts/native-*.test.mjs
node --test crates/audio-runtime/browser/*.test.mjs
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown --lib --bin wonderland-web-shell --locked -- -D warnings
(cd apps/web-shell && trunk build --release --locked)
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
cargo run -p wonderland-web-shell --example native_avatar_resources --locked -- target/native-avatar-fixtures
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install chromium
node tools/native-browser/verify.mjs
node tools/native-browser/verify-avatars.mjs
WONDERLAND_NATIVE_TERRAIN_FIXTURE=1 WONDERLAND_NATIVE_QA_OUTPUT=/tmp/native-terrain-evidence node tools/native-browser/verify-avatars.mjs
```

The terrain fixture is TEST ONLY. It creates a raised plane with corner heights `160 + 8*x + 16*y`; the avatar's accepted position (3.5, 3.5) therefore presents at 4.575 tile-height units. The ordinary production/native startup never enables the fixture. Hidden changes in the controlled service use accepted runtime memory commands, not DOM or pixel substitution.

## Qualification

Native numerical tests cover subtile coordinates, the object offset, upper floors, four-corner nonplanar interpolation, rectangular explicit edges, signed center averaging, non-boolean Hidden, out-of-world entities and snapshot recovery. The selection tests cover newer hidden/unselectable scenes, ordinary tick latency, and replacement source/entity lifetimes.

The browser visual fixture is synthetic geometry in original resource formats, not original human artwork. This correction positions entity origins against terrain; it does not implement foot inverse kinematics, terrain-normal alignment, carried/container/bone attachment placement, multitile group offsets or walking/contact-animation fidelity.
Production content/services, distinct-player multiplayer, original FSOv/VMNet translation, full Build/Buy/property/social interfaces and synchronized gameplay audio remain separate. No merge, deployment, physical-device or independent-review acceptance is implied.

## Local verification of this continuation

The full pinned native workspace passed **1,431 tests**, with zero failures and
five existing optional source/corpus checks ignored. The twelve added regressions
are nine terrain/contact cases and three current-scene picking cases, included
in the workspace total. The socket/deadline suite passed 20 tests and the existing
audio suite passed 59; passing the latter does not establish gameplay audio.

Formatting, strict native all-target and browser-target lint, the optimized Trunk
application, and the controlled native services all passed. The rebuilt application
was used without injected styles or substituted WASM in all four browser journeys:
16 standard action/queue/recovery checks, 10 original-format avatar checks,
12 elevated-terrain/avatar checks, and 7 real receipt-deadline checks. These are
45 recorded checks with overlapping paths, not 45 new independent features.
The four suites retain 29 unedited captures and empty recorded error lists.
The desktop raised-terrain and narrow avatar captures were visually inspected;
the red triangular model remains explicitly synthetic test geometry.

Before correction, eight of nine terrain tests failed on numerical or visibility
assertions. A separate test reproduced selection of a now-hidden entity from an
older mesh pick. Both pass after the presentation correction. Historical compiler
setup and build-environment failures are retained separately from behavioral RED
evidence. Neither original source bytes nor Cargo.lock changed.

The adjacent machine-readable record names exact commands, result counts,
artifact hashes, ignored tests and qualifications. Hosted CI is a separate
execution of the published commit; its status must be read from the actual run.
