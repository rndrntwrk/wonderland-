# Original playable-object prerequisite, 2026-10-15

## Result

No whole original object is qualified as playable by this artifact. The smallest practical candidate inspected is `TSOClient/FSO.Content.TSO/Content/Objects/fso_decor_2-in-1_Racing_Banner.iff` (162,870 bytes, GUID `0x6378e9e6`). Its exact original theme menu, checks, actions and graphics are present. Its initialization/main lifecycle requires **global BHAV 271 and 280**, which have no providers in the retained source census. The required authentic `objectdata/globals/global.iff` is absent. A patch file cannot replace that missing base.

The source report verifies four actual IFFs byte-for-byte against FreeSO pin `4c6b3e8f5835b228723caea3c9f683c62f244f73` and their retained Rust-decoder census hashes. It reports original OBJD, TTAB, local BHAV call summaries and visual chunk counts. This is static prerequisite evidence, not an effective-content resolver, VM execution, geometry conversion, or a new IFF decoder.

Banner TTAB 128 has action/check pairs 4102/4103 and 4104/4105. The private references close; the only external direct/root references are global 271/280. Original DGRP/SPR2/PALT/FSOM/MTEX data is retained. Those bytes still need the existing cooker and renderer import path qualified; their presence does not prove a rendered scene. Theme actions do not establish routing. The candy-border alternate has an original Go Here action and additional global 265 dependency. The tally alternate is larger and contains zero-instruction routines. The jackpot poster's apparently closed local BHAV still has semiglobal lifecycle/menu roots 8199/8201/8203 and global 265.

## Executed evidence

Run from the repository root, using a new report filename:

```sh
python3 -B tools/replay/inspect_native_playable_candidate.py --output /tmp/native-candidate-report.json
```

The probe completed twice and both JSON outputs were byte-identical. `source-report.json` is that report. It preserves source hashes and dependency facts, not copied original assets. `build-attempts.json` retains terminal build outcomes and evidence-file hashes. Two bridge builds were SIGKILLed under shared memory pressure. A third build stopped at a concurrently absent `runtime_owned_admission.rs`; that file subsequently appeared. No compiled probe test, private cook, release verification, public-refusal test or runtime acceptance test completed in these attempts. No cooked release was produced. Further compilation was paused for the shared serial build allocation.

`native_playable_object_preflight.rs` and `run_native_playable_object_preflight.py` are the pending bounded compiled qualification recipe. Its three tests and private cook remain unexecuted. Even if they pass, the recipe creates only a noncritical private inspection archive with unknown redistribution rights and unresolved gameplay tuning. It must not be admitted as a playable release. The public-cook check is intended to refuse unknown rights. `--require-playable` intentionally fails because static closure cannot establish runtime qualification.

## Smallest lawful input request

First, supply the authentic TSO `objectdata/globals/global.iff` from an installation the operator may use, retaining its exact bytes and source identity. Include its matching `.otf` if present, or the explicitly selected OTF rewrite and a record of absent/unused tuning. This unlocks inspection of banner BHAV 271/280 and their transitive closure; it does not by itself unlock full avatar admission.

For the two-user native scene, also supply the real template-person object (`0x7FD96B54`) selected by `packingslips/objecttable.xml` from `objectdata/objects/objiff.far`, its actual named global/semiglobal and tuning dependencies, and the selected avatar resource closure. That closure begins with the skeleton named by its body strings (normally `adult.skel`) and the actual portable body/head/default-suit outfit IDs, then their original appearance, binding, mesh, texture and animation references, including idle/walk resources required by the original routines. Neither the template-person definition nor a complete default skeleton/outfit closure is retained here. The exact finite leaf list cannot be derived until those authentic selector/base resources are read. Do not invent outfit selections or substitute the existing cursebook routine for an object.

An archive-preserving alternative is the operator's original `objectdata/` directory, `packingslips/objecttable.xml`, and relevant FAR3 avatar `.dat` archives under the original skeletons/outfits/appearances/bindings/meshes/textures/animations directories. Preserve directory names and resource IDs. No public redistribution permission is implied. This artifact makes no download request and contains no installation assets.

### Effective patches

The retained pin contains `Content/Patch/global.piff`, `global.str.piff`, `personglobals.str.piff`, `personglobals_wakeup.piff`, and `templateperson_burn.piff` beneath `TSOClient/FSO.Content.TSO/`. Exact hashes are in `source-report.json`. They are unapplied because the base files are missing. Record the actual ordered selected patch manifest, user-patch status, source-name matching, locale, tuning rewrites/upgrades and dynamic tuning; do not assume alphabetical patch order reproduces the original provider. There is no matching banner patch in the retained report.

### Supported local cooker destination

Stage permitted input bytes under a new private directory, for example `/tmp/wonderland-private-import/source/`, with `cook.json` at its root and only safe relative paths below it. Keep original installation layout beneath that root where supplied. The cooker is `wonderland-asset-cooker cook <cook.json> <new-release-directory>`; its schema is documented in `docs/swarm-b/cooker.md` and defined in `tools/asset-cooker/src/registry.rs`. It does not discover a whole installation automatically. Declare each source/format/rights and exact dependency graph explicitly, and verify the resulting release against its trusted manifest digest before use.

A primary `SourceSpec.patches` list is supported. A `ScopeSpec` has only `path` and `source_name`, **no patch list**. Simply setting the banner's `global` path does not apply global patches. Resolve the effective global scope through the existing content resolver with explicit scope inputs, or add and test the required adapter before claiming effective patched closure. Do not silently pass a raw base scope as patched content. Archive entries that are not IFF are opaque unless explicitly imported through the appropriate standalone decoder; archive availability alone does not establish avatar rendering.

The runtime import additionally requires verified original footprint/placement/group/routing metadata and actual provider closure. The cooker does not execute BHAV or infer that metadata from filenames. Full two-avatar admission, menu checks/actions, source-owned state change and rendering must be exercised after the missing authentic content arrives.

## Source pointers

- `TSOClient/tso.content/WorldObjectProvider.cs`: object table, objiff/objotf/objspf archive providers and source lookup.
- `TSOClient/tso.content/WorldGlobalProvider.cs`: TSO `objectdata/globals/<name>.iff`, tuning and patch-provider inputs.
- `TSOClient/tso.simantics/Entities/VMAvatar.cs`: template-person GUID, body strings, skeleton selection and default suit selectors.
- `TSOClient/tso.content/AvatarSkeletonProvider.cs`, `AvatarOutfitProvider.cs`, and `Framework/TSOAvatarContentProvider.cs`: exact FAR/file resource providers.
- `docs/swarm-f/CONTENT_PREFLIGHT.md`: earlier actual-source closure refusals.
- `docs/swarm-b/runtime-bridge.md`, `docs/swarm-c/avatar-source-notes.md`: explicit gameplay metadata and avatar composition limits.

## Deferred serial command

```sh
PATH=/tmp/wonderland-build-tools:/workspace/scratch/efdbaac59410/rust-reconstruction-20261013/toolchain/bin:$PATH \
CARGO_HOME=/workspace/scratch/efdbaac59410/rust-reconstruction-20261013/cargo-home \
CARGO_TARGET_DIR=/workspace/scratch/efdbaac59410/wonderland-browser-entry-20261014/target \
LD_LIBRARY_PATH=/workspace/scratch/efdbaac59410/rust-reconstruction-20261013/toolchain/lib \
CARGO_BUILD_JOBS=1 \
python3 -B tools/replay/run_native_playable_object_preflight.py --output /tmp/native-object-preflight-new
```

Use only after an explicit shared-build allocation. This pending recipe is not a successful validation claim.
