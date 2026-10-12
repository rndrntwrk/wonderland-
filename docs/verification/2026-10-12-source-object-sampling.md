# Source-object GPU sampling increment

Original reference: `riperiperi/FreeSO@4c6b3e8f5835b228723caea3c9f683c62f244f73`.

The browser used nearest filtering even though `PreparedTexture::sampling` and
`TSOClient/tso.content/ContentSrc/Effects/RCObject.fx` specify linear minification,
magnification and mip interpolation with clamp addressing. GPU schema 5 now
carries explicit filtering and source-generated mip pixels for textured source
objects. Old schemas, architecture and Vitaboy addressing/filtering retain their
existing behavior. Linear wrap is refused until separately qualified.

## Source arithmetic and review correction

Generic GPU `generateMipmap` is not source-equivalent. Actual Chromium reproduced
a disappearing/unpickable minified sprite cutout with that approach. It was
replaced before publication with Rust-generated chains:

- SPR2 sprite selectors use `TextureUtils.Decimate`: RGB averages only nonzero
  alpha texels, integer truncation, maximum alpha.
- Custom selectors use `AvgDecimate`: the same RGB rule and alpha sum / 4.
- The historical sprite horizontal height comparison and the upload loop's raw
  dimension halving, including a zero short axis, are preserved explicitly.
- Pixel reads are bounded; mip allocation is reserved before generation. The
  browser verifies complete aligned mip chains and includes their bytes in its
  upload budget. ID framebuffer textures do not acquire color mipmaps.

Source anchors: `TSOClient/tso.common/Utils/TextureUtils.cs` methods `Decimate`,
`AvgDecimate`, `UploadWithMips`, `UploadWithAvgMips`;
`TSOClient/tso.files/Formats/IFF/Chunks/SPR2.cs::GetTexture`;
`TSOClient/tso.content/RCMeshProvider.cs::GetTex`.

This selects an uncompressed NPOT-mip-enabled WebGL2 presentation profile. It does
not qualify source DXT compression, anisotropic variants, the NPOT-disabled
profile, avatar sampler changes, physical-device performance or full content
corpus parity. The existing software rasterizer remains nearest-filtered.

## Executable evidence

`gpu_fixture` produces six source-model sampler cases through the real
`WorldRenderer`: sprite/custom cutouts, non-square images and one-pixel short
axes. It verifies independently specified final mip bytes. The actual Chromium
`sampling-browser.mjs` uploads those Rust-generated chains and checks visible
pixels and exact pick IDs. It also checks linear interpolation, minification,
NPOT textures, old-schema behavior, malformed chain rejection and alternating
filters on one shared texture in a single frame.

The first schema-5 browser run failed against the old production adapter. A
second red witness reproduced the transparent-cutout failure with automatic
mips; the explicit source chains pass. No additional unit-test suite was added.
The existing material-pass regression was updated for schema 5 and retained
mask/body filtering distinctions. Native GLES replay now records the production
filter binder and uploads the same explicit mip pixels.

These are authored source-format fixtures and actual renderer/process execution,
not redistributed production content or a claim that public multiplayer is ready.
Hosted CI and the compiled application journey are required before merge.

## Local verification, 12 October 2026

- Compiled Trunk/WASM application: 39 real browser scenarios, zero browser errors
  or failed requests. This is the fixture-backed application journey.
- Source GPU Chromium: 11 scenes and 14 context/lifecycle checks passed.
- Explicit-mip Chromium sampling journey: passed, including all six Rust cases,
  shared-texture alternating filters and malformed packet refusals.
- Native Mesa GLES: 11 scenes passed with zero ID mismatches.
- Existing JavaScript regressions: 68 passed.
- Existing Rust workspace regressions: 2,023 passed, zero failed; five existing
  ignored tests remain unexecuted.
- Workspace formatting and strict native Clippy passed. Hosted checks remain a
  merge gate; local execution does not establish production deployment.

The compiled application module SHA-256 matches the source module:
`ab3ca1a12a852b1147331337fb84cf76efcf7de90ace4215cfa10a1d3a4db75e`.
