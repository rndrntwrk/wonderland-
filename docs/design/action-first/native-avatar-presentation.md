# Native avatar resource presentation

Base: PR #30, `0df5bf4c10272e2dc9fee976cf4d2b744e9d7e16`.

## Implemented path
`NativePlayer::avatar_visual_frame` reads admitted current state without advancing
the simulation. The projection retains native entity identity, source GUID,
source mutation revision, head/body outfits, skin tone, gestures, scale/display
flags and current animation/carry layers. It excludes unrelated social records,
pending animation events and mutable pre-checkpoint skeleton history.

`NativeAvatarProjection` resolves the imported original-format rig, meshes,
bindings, appearances and textures, then samples existing Vitaboy skinning at the
accepted frame. The native lot now uses the existing Game content loader's bank.
Neither preview outfits nor a fabricated character is substituted for missing
resources.

The initial implementation supports the canonical adult template with its
`adult` rig, numeric/named outfits, current bound appearances/decorations and
gesture sets. Unsupported custom/pet rig mapping, TS1 inline outfits and
container/bone attachments remain diagnosed. Content is user-imported and
format/budget-validated; this increment does not authenticate an operator's
complete visual asset release.

## Identity and resource lifetime
Preparation requires an exact accepted `WorldRevision`, entity generation, GUID
and native mutation revision. Application checks the original world-object record
again and validates a private candidate before replacing any presentation.

Avatar animation ticks can advance without incrementing the source entity's
mutation counter. The presented avatar's `visual_revision` therefore uses the
accepted native tick. This is only a renderer version: the source entity identity,
simulation counters, native content digest and `WorldRevision` are not modified.

Loading/changing resources within one tick is not another authoritative frame.
A bank/decode/reconnect change creates a fresh disposable viewport lifetime;
the existing renderer's stale-frame checks are unchanged. Camera/floor controls
survive that replacement, while old raster/pick work is disposed.

Texture decoding is sequential and budgeted, scoped to the imported bank and
connection lifetime. A late decode wakes a fresh projection of the newest
accepted state; it never attaches an old prepared pose to a newer world.
Disconnect stops visual progression. No game events, marker queues, actions
or durable effects are dispatched by this adapter.

## Reproduction
Use the pinned Rust, Trunk and Node versions from the repository.
```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown --lib --bin wonderland-web-shell --locked -- -D warnings
cargo fmt --all -- --check
node --test apps/web-shell/scripts/native-socket.test.mjs
node --test crates/audio-runtime/browser/*.test.mjs
(cd apps/web-shell && trunk build --release --locked)
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
npm --prefix tools/native-browser ci --ignore-scripts --no-audit --no-fund
tools/native-browser/node_modules/.bin/playwright install chromium
node tools/native-browser/verify.mjs
cargo run -p wonderland-web-shell --example native_avatar_resources --locked -- target/native-avatar-fixtures
node tools/native-browser/verify-avatars.mjs
```
Use a fresh output directory for the fixture exporter (create-new semantics).
`WONDERLAND_AVATAR_QA_FILES` selects that directory and
`WONDERLAND_NATIVE_QA_OUTPUT` selects browser evidence output. These runners use
loopback services and must run sequentially because they share ports.

## What the evidence establishes
Native tests cover source identity, current poses, missing textures, visibility,
scaling/tint, invalid gestures/clip metadata, duplicate/stale records and the real
renderer/FrameStore's generation checks and depth-tested selection.
The browser loads synthetic standalone-format resource files through the real
UI, selects the actual rendered mesh, observes pixels changing with accepted
animation frames, freezes on disconnect and restores resources on reconnect.

The test asset is deliberately a tiny triangular mesh, **not original human
artwork**. Its binary formats are real; its artistic content and animation are
authored test fixtures. No production startup imports them. The separate inherited
browser suite still exercises original behavior bytecode, needs, cancellation
and recovery. Browser screenshots are software-rendered tests, not physical-device
performance or production multiplayer qualification.

## Remaining boundary
Current timelines are sampled from the original bind pose. Historical bone
retention, head seeking, complete attachment/inline-outfit/pet adapters,
walking/contact-animation qualification, original FSOv/VMNet translation,
gameplay audio and production distinct-player services remain. The existing
original-server lane remains refresh-only. Buy/Build/property/social/dialog
completion is not supplied by avatar presentation.

No original source assets, fonts, dependency versions, lockfile, shared styles,
concurrent renderer branches, user saves, merge or deployment are changed.
