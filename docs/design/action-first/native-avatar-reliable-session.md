# Native avatars and reliable action sessions

## Scope and source pins

This combines PR #33's original-format native avatar presentation (`5aa879438076dfe077254212fb0326350b09927c`) with PR #31's receipt deadlines (`03e3fb8697c7e1a06447681536e0333810ccbab4`) on an isolated integration branch. Neither parent branch is rewritten or merged by this work. The already implemented original rig, outfit, texture, gesture, skinning and animation-frame adapters are reused, not replaced by another character implementation.

The player path is Sign in → load Game content → choose Sim → source city map → Visit → render accepted animated avatar → select its actual mesh → invoke a source action → handle a lost acknowledgement → reconnect with the avatar, controls and action history preserved.

The native mode remains opt-in. Preview and original FreeSO refresh-only lanes are unchanged. Nothing enables a production service or automatically imports the test resource pack.

## Integration invariants

The avatar and receipt work changed the same native component and browser workflow independently. Selecting either branch wholesale would lose the other capability. This integration preserves both changes in ordinary tracked source.

The socket's `settled` method is exposed through the native host wrapper. Only a successfully validated `PlayerUpdate::Receipt` invokes it, after the runtime mutable borrow is released. Accepted animation ticks, terminal action activity and avatar texture completion cannot satisfy or extend that acknowledgement deadline. Receipt timeout marks the operation Unknown and suspends the native runtime; it never retries a command or infers rejection from time elapsed.

The avatar adapter continues to sample the current accepted frame. Timeout/disconnect must preserve the last rendered avatar rather than continue playing speculative frames. After a new admitted checkpoint, the original resource bank binds to the current native entity generation, animation resumes on accepted updates, and camera/floor/wall controls survive the disposable viewport reset. Unknown operation/history state is neither cleared by texture loading nor reconstructed by replaying old transient events.

The permanent read-only workflow retains all three browser paths: the action/cancellation/responsive suite, the receipt-timeout suite, and the resource-backed avatar suite. The latter now exercises the combined failure boundary, rather than treating separate passing builds as proof of integration.

## Regression and acceptance

The inherited ten receipt-deadline cases were executed against the avatar-only source before integration: eight failed and the original ten socket cases still passed. With the receipt implementation integrated, all twenty cases pass. Those logs are evidence of the missing cross-branch behavior, not newly discovered simulation faults.

The strengthened actual-browser avatar test additionally requires:

- Visible pixel changes while accepted avatar animation ticks continue during a deliberately missing receipt, with no server-forced close.
- The unchanged real 15-second deadline, truthful Unknown state, one transmitted action and retention of the visible avatar model.
- Identical frozen canvas pixels while disconnected; resumed changing pixels after checkpoint/resource recovery.
- Preserved selected wall visibility, exact action-history count and disabled unsafe commands while the result is Unknown.
- Explicit dismissal transmitting nothing, a separately selected action being acknowledged, and session/viewport disposal on return to the city.

The fixture uses original Vitaboy file formats and the real content loader, not a JavaScript character replacement. Its triangular visual mesh is synthetic: this does **not** certify original human artwork or full walking/contact-animation fidelity. The source needs action uses unchanged original behavior bytecode. Production startup does not import either test setup.

## Reproduction

Use the pinned repository Rust/Trunk toolchains and locked test dependencies. The normal workflow builds the application and controlled gateway/runtime, generates the standalone avatar test resources, and executes:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown --lib --bin wonderland-web-shell --locked -- -D warnings
node --test --test-reporter=tap apps/web-shell/scripts/native-*.test.mjs
node --test --test-reporter=tap crates/audio-runtime/browser/*.test.mjs
node tools/native-browser/verify.mjs
node tools/native-browser/verify-avatars.mjs
node tools/native-browser/verify-receipt-deadline.mjs
```

Browser verification uses Playwright because the Browser plugin is not available in this session. No fake browser clock, injected CSS or substituted compiled code is used. Original source resources, fonts, dependency versions, lockfile and shared UI styles are not changed. Verification artifacts are bound to the actual tested revision; compilation alone is not browser acceptance.

## Remaining work

This is the native avatar/session integration checkpoint, not the complete FreeSO rewrite. The inherited avatar adapter starts each current sample from the bind pose; historical bone retention, head seeking, SLOT/bone/container attachments, custom/pet rig mapping and TS1 inline outfits still need source-complete adapters. Complete original human-art qualification needs an authorized resource cohort. Gameplay audio, production distinct-player services/content and durable operation reconciliation, full Buy/Build/property/social/object-dialog integration and original FSOv/VMNet conversion remain separate work. Physical-device, Safari, performance and independent-review approval are not implied.
