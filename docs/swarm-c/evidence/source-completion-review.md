# Independent source completion review

**Status: closed, 2026-10-06.** No confirmed critical or important findings remain in this review's assigned source scope. Both final browser PCF fixtures also pass as described below. Final assembled gates are recorded separately by their owner.

Reviewed the completion working tree based on `a4c8bbaacde50cc5b1b271639b5fe454ac317e02`, using the original source retained from baseline `4c6b3e8f5835b228723caea3c9f683c62f244f73`. Scope covered source facade geometry/FSOf formats, derivative identity and ownership, room/light/shadow production, weather/environment math, allocation admission, malformed inputs, and targeted client altitude/camera composition. This reviewer made no production changes.

The frozen [WCRC implementation](../../../crates/render-iso/src/lighting/wcrc.rs) SHA256 is `3002276dbbe1c7faace580b58ea2d92eaef5ff7d63e135160a8fa73c993b1729`. The C and client copies match.

## Findings resolved

| Finding | Resolution checked |
| --- | --- |
| Missing source WCRC sunlight path | Explicit WCRC input now executes ordered wall captures, floor/roof captures, optional Ultra outdoor objects, four RGBA8 PCF stages, outdoor SSAA and indoor bleed. WCRC presence remains supplied source state, independent of mode and Ultra. |
| Aggregate geometry could allocate beyond the remaining scene budget | Each builder receives remaining bytes/vertices/indices; retained outputs are admitted before the next builder. Raster triangle-range scratch and the additional Ultra projected target are included. |
| Mesh residency used length instead of capacity | Wall/projected mesh accessors account for actual vector capacities. Empty projected outputs are omitted and descriptor counts/capacities are bounded. |
| Room key staging reserved 67 bytes for a 68-byte header | Serialization now reserves its exact capacity before allocation. Room-map image descriptors are included in peak and retained accounting. |
| WCRC follow-up identity and budget gaps | Software-depth target sizing is explicit and fenced on consumption; raw and filtered targets have distinct identities. Maps, sun and Ultra changes reject stale captures. Target initialization work, retained floors, two filter targets, run-table scratch and noise-mip old/new allocations are admitted. |

The former per-room map scan is now one bounded traversal per floor. The exact constant-run PCF shortcut restores practical default-budget operation: it proves all possible source/noise-offset taps have identical RGBA before reusing a stage result. Its cache has one entry per stage; edge pixels execute the source taps. Review found no semantic difference in the four quantized stages, including the second stage's BA-only writes.

## Original source anchors

| Original source | Semantics checked |
| --- | --- |
| [LMapBatch.cs](../../../TSOClient/tso.world/LMap/LMapBatch.cs), lines 43–48, 503–553 and 786–917 | Explicit WCRC branch, outdoor/indoor pass selection, current target divider, two capture targets, channel masks and four filter passes. |
| [WallComponentRC.cs](../../../TSOClient/tso.world/RC/WallComponentRC.cs), lines 371–414; [RCObject.fx](../../../TSOClient/tso.content/ContentSrc/Effects/RCObject.fx), lines 176–227 | Ordered `UseOffset` channel transitions, restored cutaway heights, mask coordinates and discard. |
| [TerrainComponent.cs](../../../TSOClient/tso.world/Components/TerrainComponent.cs), `DrawLMap`; [RoofComponent.cs](../../../TSOClient/tso.world/Components/RoofComponent.cs), `DrawLMap` | Source floor/roof level flattening and height attenuation. |
| [SpriteEffects.fx](../../../TSOClient/tso.content/ContentSrc/Effects/SpriteEffects.fx), lines 187–193 and 254–402 | Point-wrap noise and the literal four PCF stages. Effect SHA256: `9731f541c6e13918021b7f93ac86ed7bee9747e56349659dbdf9ed0c38d38dd3`. |
| [LightMap2D.fx](../../../TSOClient/tso.content/ContentSrc/Effects/LightMap2D.fx), lines 181–217 and 249–279 | Positive half-texel SSAA offset, red/green outdoor behavior, and indoor bleed including negative direction values before attachment conversion. |
| [TextureGenerator.cs](../../../TSOClient/tso.common/Utils/TextureGenerator.cs), lines 133–149; [TextureUtils.cs](../../../TSOClient/tso.common/Utils/TextureUtils.cs), `UploadWithMips` and `Decimate` | Source 512-square noise, alpha-aware integer RGB averaging, MAX alpha and implicit point mip selection. Wall-style SPR masks retain base-level bilinear sampling; [SPR.cs](../../../TSOClient/tso.files/Formats/IFF/Chunks/SPR.cs), line 263, excludes them from mip generation. |
| [Blueprint.cs](../../../TSOClient/tso.world/Model/Blueprint.cs), lines 173–196 | Client center altitude matches relative BaseAlt, source edge clamps and interpolation. Source isometric camera composition was also checked; the weather/source-zoom boundary mismatch was corrected. |

## Independent numerical evidence

A separate NumPy float32 reference implements the literal source taps without the Rust constant-run shortcut, including intermediate RGBA8 conversion and BA-only writes. Both final 32×32 fixtures matched **all 4,096 output bytes exactly**: maximum per-channel error and differing-channel counts were `[0, 0, 0, 0]`.

- **Both-channel, nonconstant 4×4 noise:** `wcrc-pcf-probe.json` SHA256 `43de53147e69325d115ebfed85a4cf36cb21fc9940427388d7e22bdd1f8143b0`; flattened final RGBA/oracle SHA256 `7705f08f98514a1e3539ec99236c9f38ac1aae114a8f17d43d7d96ba3b494d58`.
- **Source 512×512 noise:** `wcrc-noise-probe.json` SHA256 `26d22893050b91a0ff0ff76bee58e50f7e114332931e1029121b1a9ead9091ef`; flattened final RGBA/oracle SHA256 `f626cf0ae55659dbccf53e77bbf0ad58666bbe33efbdd1dd024e1effc02f17a2`. The independent reference selected mip 7 and reproduced every preceding alpha-aware `Decimate` level.

The browser reviewer subsequently executed both final fixtures on actual WebGL2. This reviewer read both completed reports: all 4,096 bytes agree exactly for each fixture, and stage two preserves raw RG. A separate implicit-noise lookup agrees exactly with mip 7 after uploading all ten source-style mip levels. The reports bind the same frozen C hash above and are preserved in the client tree at `apps/web-shell/tests/evidence/gpu-review/reports/pcf-result.json` and `pcf-noise-result.json`. These runs use Chrome/ANGLE SwiftShader.

After the final lint-only cached-predicate change, the root's nine passing WCRC tests regenerated both captures. This reviewer reran the independent CPU reference on those outputs: both complete fixture hashes and both 4,096-byte output hashes above remain unchanged, with zero differing bytes. The browser metadata refresh is guarded by equality of all original GPU inputs, noise/raw/output arrays and the original effect digest; it preserves the recorded GPU readbacks and states this provenance change explicitly.

The source owner reports nine [WCRC tests](../../../crates/render-iso/tests/lighting_wcrc.rs) passing. Its 64×64×5 fixture contains a wall quad, upper-floor quad, roof quad and optional Ultra outdoor object triangle, with source-size noise and unchanged default budgets:

| Quality | Capture work units | Whole-scene work units | Retained sunlight bytes | Retained atlas bytes |
| --- | ---: | ---: | ---: | ---: |
| Normal | 29,584,044 | 41,690,764 | 20,321,680 | 12,197,504 |
| Ultra | 117,836,381 | 164,997,309 | 81,285,520 | 30,486,656 |

Separately, this reviewer read the completed `lighting-allocation-final.log`: the single-test [counting-allocator executable](../../../crates/render-iso/tests/lighting_allocation.rs) passed. A 64×64×5 nonuniform WCRC wall/noise capture plus atlas measured **26,773,926 bytes peak against a 30,000,000-byte limit**. Actual retained allocations were **26,422,800 bytes**, exactly equal to the sunlight and scene residency getters. The same executable retains the rejecting-mesh peak regression. WCRC stage admission was also inspected algebraically: two targets plus one `u32` line table, with only remaining bytes available to the previous/next noise mip pair.

## Limits

These are controlled source-policy fixtures. Raw capture geometry was checked against source and analytic tests; this record does not claim an exhaustive original-engine GPU capture comparison or licensed-content qualification. Default-budget success covers the stated sparse geometry fixture; complex lots may reach the explicit work/byte limits. The work counters are bounded work units, not frame-time guarantees. Browser PCF evidence, physical-device qualification and exact-source aggregate build results belong to their separate records. No heavy builds were run by this reviewer.

## Final publication boundary audit (2026-10-06)

The independent final audit found no unresolved publication blocker after the
CI launcher correction. Both new engine browser tests select the full Chromium
channel used by the C runner's `--no-shell` installation and retain the explicit
local executable override. The corrected raw-readback runner completed twelve
actual mapped WebGPU copies with exact RGBA and device-loss cancellation;
[the final local log](continuation-2026-10-06/webgpu-launcher.log) records the
browser and SwiftShader adapter. This check does not qualify page presentation.

- Both completion checkouts preserve `TSOClient/` and `Other/` exactly against
  original baseline `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
- The completion changes leave the authoritative A implementation and shared
  authority/contracts unchanged. C remains Rust 1.75, its isolated engines remain
  Rust 1.95, and the existing client remains Rust 1.99.
- No target, dist, packaged WASM, ELF, or dependency directory was a staging
  candidate. The only intentional client binary evidence consisted of the three
  reviewed compressed JSON fixtures.
- Locked builds, formatting, strict client Clippy, and failure propagation are
  retained. The C color thresholds remain MAE 4, RMS 12, and at most 3% of
  components differing by more than eight; stable-interior IDs require zero
  mismatches. The reviewed PCF allowance stays one byte, while both recorded
  cases actually match every byte.

The [final C evidence bundle](continuation-2026-10-06/index.json) retains the
441-test reference pass, all eighteen native/WASM records, the pinned-authority
rerun, source-codec results, toolchain identity, and 87 checked source hashes.
The client aggregate, release and assembled browser results are recorded in its
separate stacked branch.
