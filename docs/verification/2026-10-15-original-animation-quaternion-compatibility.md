# Original animation quaternion compatibility

## Verified source, not a guessed normalization policy

The original source baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
`Animation.Read` preserves the source float components, with the documented
coordinate signs. `Animator.RenderFrame` passes those components directly to
`Quaternion.Slerp`; negative fractions copy the original sample. Skeleton
matrices are constructed from the retained quaternion without normalization.

The exact FSOMonoGame gitlink is
`74f7ef558111e62a7d0a2228b6f9714980e000fa`. Its public source was inspected:

- [Quaternion.cs, lines 731–763](https://github.com/riperiperi/FSOMonoGame/blob/74f7ef558111e62a7d0a2228b6f9714980e000fa/MonoGame.Framework/Quaternion.cs#L731-L763),
  blob `d29dd0b8e9640c0e8ce564b6420a3f8f836f921b`.
- [Matrix.cs, lines 679–706](https://github.com/riperiperi/FSOMonoGame/blob/74f7ef558111e62a7d0a2228b6f9714980e000fa/MonoGame.Framework/Matrix.cs#L679-L706),
  blob `42ac0d0dc1ae613c9de2786782ee907679699048`.
- Official MonoGame `v3.6` resolves to commit
  `1c09add4a7407cf14690614d5ec36666d6c2a8ea`. Its Slerp method is identical
  (Quaternion.cs blob `db94ed32b629d07cd2279580a9b0106ca2529df0`). This is a
  source comparison, not a claim that the separately retained version-0 Mac
  assembly proves a particular NuGet binary's provenance.

The compatibility implementation keeps the strict `dot > 0.999999f` branch,
antipodal sign, source double transcendental calls and float cast boundaries.
Neither inputs nor outputs are normalized. Matrix quadratic expressions retain
the source operation order and are transposed into column-vector storage.
No original animation operands or assets are embedded in the repository.

## Admission and safety boundary

Only original avatar clip channels admit finite nonunit quaternions, with a
finite squared norm. The former global-looking unit requirement was not a
property of the original format. This change does not substitute a looser
near-unit threshold selected to fit one asset cohort.

The ordinary skeleton guard, generic `render-core::Quat::slerp`, and arbitrary
public pose edits keep their existing unit requirements. Animation writes put
private exact-bit provenance alongside each affected pose rotation. Rebuilding
accepts a nonunit value only if it still matches that recorded value, and is
finite with finite squared norm. Editing a sampled pose cannot mint provenance.

Existing file, frame, sample, motion, metadata and palette budgets remain.
Interpolation overflow, matrix overflow, hierarchy overflow and skin overflow
fail explicitly. Timeline application stages a candidate pose, so a failed
sample cannot partially publish rotations or palette data. Zero-prefix handling
retains its previously documented safety deviation.

## Conservative mesh envelope

For a bone, let `N` be the maximum admitted squared quaternion norm across its
supplied channels, with a floor of `1.001` covering the unchanged ordinary
pose guard. After antipodal adjustment, source spherical interpolation has
`d >= 0` and coefficients `a, b` satisfying
`a²+b²+2abd=1`, with `a²+b²<=1` for a convex fraction. Therefore its squared
norm is at most `max(1, norm²(left), norm²(right))`. The source linear branch
is also convex. This bound remains valid through successive convex layers.

A raw quaternion matrix is `(1-N)I + N R` for a unit rotation `R`, so its
operator norm is at most `max(1, 2N-1)`. The implementation derives this bound
per bone, adds 0.3% relative per-joint binary32 slack, and accumulates stretch
and translation reach along the hierarchy. Overflow remains an error.

All supplied clips must cover both the retained baseline and current timeline.
Nonnegative layer weights are required for this envelope's convex assumption.
Externally authored or extrapolated poses require exact skinned bounds instead.
Unclamped skin blend weights remain covered by the absolute-weight bound.

## Verification

Synthetic tests cover raw nonunit Slerp output, the distinct near-linear branch,
antipodes, signed-zero source preservation, negative fractions, raw matrix
conversion, arbitrary pose/rig edit rejection, nonfinite/norm-overflow input,
finite-norm matrix and hierarchy overflow, atomic failure, and fractional
layered conservative bounds. Existing avatar, content and render-core tests
also remain part of the focused gate.

Private qualification uses the exact approved original nine-clip cohort and
the original adult skeleton and selected male/female meshes. Its reports and
rasters stay outside Git. It verifies original file hashes and exact decoded
DTO preservation, fractional and layered poses, repeated retained sampling,
conservative containment and the existing CPU diagnostic raster. That raster
uses explicit diagnostic colors, so it is not a textured browser/GPU acceptance
claim. Live route production and browser gameplay remain separate gates.

Verified run on 2026-10-15:

- `cargo test --locked --offline -j1 -p wonderland-avatar-view`: 35 passed.
- `cargo test --locked --offline -j1 -p wonderland-avatar-content -p wonderland-render-core`: 114 passed.
- `cargo fmt -p wonderland-avatar-view -- --check`: passed.
- Private original qualification: all 9 clips, 11,292 sampled poses,
  40,515,696 skinned vertices, 6 source appearance/skin compositions; every
  checked vertex inside the supplied-clip envelope.
- 256 retained ticks per clip, with fractional draws and carry overrides;
  9 nonempty CPU diagnostic rasters. All 29 original input hashes verified
  before and after; decoded animation DTOs unchanged.
- Private report SHA-256: `a006cfa0c352225950cf3cc423eaf229451b728fe5a9c90d834f6b4ebcddd4d1`.

Strict Clippy passed with the retained matching Rust 1.99 component:
`cargo clippy --locked --offline -j1 -p wonderland-avatar-view --all-targets --no-deps -- -D warnings`.
The first invocation had no Clippy on PATH; the verified retained component
was then selected explicitly. No compiler/lint binary was acquired for this fix.
