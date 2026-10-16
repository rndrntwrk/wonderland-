# Source route motion and animation timeline

## Scope and provenance

The authoritative runtime now drives ordinary same-level walk/run/swim line timing, source turn clips, and the existing actor animation timelines. World projection reads the accepted fractional position and continuous yaw. No wall-clock/UI timer or fabricated animation payload is involved.

Reference: source commit `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

- `TSOClient/tso.simantics/Engine/VMRoutingFrame.cs`: turn dispatch at 700–735; acceleration/blends/collision and steering at 736–964; cleanup at 966 onward; auto-run, begin/end, turn selection and timeline setup at 1054–1180; speed factors at 1200–1209. Reference file blob: `802705d830bdf9c455859ad953c59aa8b7cddd25`.
- `TSOClient/tso.simantics/Engine/Routing/VMPathLineSegment.cs`: `CalculateTotalFrames`, `UpdateTotalFrames`, `NextPointAndVel`.
- `VMAvatar.Direction` uses double-precision PI division and ties-to-even notch rounding.

Preserved details include signed 0x8000-subtile integer interpolation, distance truncation before multiplying by10, velocity ramp to8, final28-frame deceleration, pre-steering blend weights, steering thresholds, run duration /3, pool/eligible-pet duration ×2, run turns at2×, source turn thresholds/tweaks, first begin-turn tweak, final-frame EndWalk, and source three-layer reuse by the first two names. Reused resource/speed/loop/backwards ownership is captured durably; the producer only changes fields the source changes. Carry is retained.

## Existing pathfinder boundary

The native grid pathfinder still supplies line endpoints and route/collision policy. This is **not** rectangle-router/Bezier geometry or whole-frame source parity. Every accepted intermediate integer line position, including repeated truncated positions, passes existing segment and blocker checks. Original Walk/Run/Swim/turn metadata is resolved through the effective avatar tables; positive real NumFrames is required. Test metadata vectors are authored mechanics fixtures and are not original-content rendering evidence.

Missing metadata, unsupported nested/portal/chair/cross-level motion, or a changed active style/type/pool context preserve movement-only routing and expose `RouteAnimationUnavailable` plus `runtime_route_animation_unavailable`. There is no synthetic frame count, guessed missing clip, or silent alternative motion. Legacy inflight routes explicitly remain movement-only. New source timing is admitted on a new route.

## Snapshot schema5

SimState appends `source_routing: SourceRoutingState` after schema4's owned-admission state. Its order is:

1. `routes: BTreeMap<u64, RouteMotionRecord>`
2. `avatars: BTreeMap<EntityRef, SourceAvatarMotion>`

Route records are Pending(actor, auto_run), Unavailable(actor, reason), or Active(SourceRouteMotion). Active state includes captured resources/context and turn durations, original walk style, phase, line endpoints/frame counters, velocity/blend velocity, radians/target/tweak, animation/revision, source-reuse marker, exact three-layer playback ownership, and the duplicate published visual pose. The pose contains integer position/facing, continuous radians/position, velocity and turn velocity.

Schemas1–4 migrate through the existing append sequence and then append16 zero bytes (two empty map lengths). Old EntityState, AvatarState, RouteContinuation, and accepted-command layouts/discriminants are unchanged. Route completion/cancel/reset/deletion clean only the owning actor's source state/layers. External position/direction writes rebase both pose copies. Target deletion clears the active line at the deletion boundary. BuildLocked movement rolls back route/timing at both Progress and Arrived. A source-frame preflight defers all route changes while its current footprint is build-locked, preserving the pending preview revision and delaying same-position animation startup/completion cleanup until unlock. It does not bypass build locks. This is native build-transaction deferral, not source routing parity: route timing and timeline ownership/configuration are retained, while the existing avatar timeline tick can still advance clip frames during the deferral.

Validation rejects orphan/mismatched owners, invalid frame intervals/durations, non-admitted selected clips, changed owned timeline playback, nonfinite poses, mismatched duplicate poses, and retained pose yaw/position outside the source quantization envelope.

## Verification

Nineteen focused mechanics and integration tests cover walk/run/swim timing, turns, source layer reuse, carry, cancellation, snapshots/replay, old-schema migration, target deletion, external direction writes, negative-axis completion, context downgrades, malformed source state, and retained-pose tampering. Game-runtime projection tests exercise fractional authoritative positions/yaw without an appearance substitute.

Final gates (2026-10-15, Rust1.99, shared target, offline/locked/-j1):

- `cargo test -p sim-core -p wonderland-game-runtime --tests --no-fail-fast`: **739 passed, zero failed,62 suites**, including19 focused route-motion tests and the fractional game-runtime projection assertion.
- `cargo clippy -p sim-core -p wonderland-game-runtime --all-targets --no-deps -- -D warnings`: passed with the matching1.99 Clippy driver.
- `cargo fmt --check -p sim-core -p wonderland-game-runtime`: passed.
- `git diff --check`: passed.
- Independent source/ownership review: original timing/rounding/ownership findings and the retained-pose/reuse/build-lock follow-up findings were corrected; final static review found no blocking issue in the scoped implementation.

 This change itself does not claim browser/original-content end-to-end qualification or deployment; those require the independently imported original clips and renderer qualification.
