# Original avatar content bridge

`wonderland-avatar-content` connects the pinned `wonderland-legacy-formats`
binary readers to `wonderland-avatar-view` and `wonderland-render-core`. It has
no browser or application-model dependency.

## Input and selection

Pass original named bytes to `inventory` to see standalone resources and original
FAR3 entry names. It indexes archive metadata without decompressing payloads.
Supply the exact skeleton name and explicit head/body `CollectionSpec` roles to
`import`. Original collection order and indices are retained. No filename
heuristic assigns a collection's game role.

FAR3 supplies original FileID and TypeID. Standalone `.po`, `.oft`, `.apr`, `.bnd`,
`.hag`, mesh and texture resources require `NamedBytes.key` from their original
identity or a source mapping. The native FileProvider `name.<packed hex
ID>.extension` convention is also recognized by `source_filename_key` and by
inventory/import automatically. Arbitrary filenames do not establish an identity.
FAR3 does not contain a GroupID; its provider resolves original file/type pairs.
As in native Avatar.cs/FileProvider, both FAR3 and standalone resolution uses
the original file/type pair; binding GroupID remains preserved metadata.
Multiple matching resources produce an ambiguity diagnostic with no chosen
precedence. Raw FAR3 and the bounded B Persist/QFS extraction path are supported.

Each `CollectionChoice.key` is the opaque `vitaboy:<original packed purchasable
ID>`; `outfit` is the original outfit FileKey to use in C `AppearanceSelection`.
Head and body are independent options. `gender` retains the source numeric value
(0 male, 1 female in the source documentation); unknown values remain unknown.
This unused original `.po` field does not establish creator compatibility: actual
official female head purchasables can also carry 0. The selected source collection
provides native creator gender context; callers must not impose male-only
compatibility merely from that raw field.
Each choice carries separate readiness for Light, Medium and Dark. A body with
gestures enabled also requires the original hand group and hand appearances.
Missing resources never create fallback avatars.

When projecting into a persistent avatar profile, use
`file_content_key(choice.outfit)` for the selected head/body appearance, retaining
the purchasable `choice.key` as collection membership. Native creator selection
resolves a purchasable to its OutfitID before submission. The renderer accepts
the actual outfit FileKey directly, even when it is absent from current creator
collections. Repeated purchasables that reference one outfit can share that
appearance option.

## Geometry and textures

`ImportedContent.compose` delegates to the C catalog and rig, then skins each
prepared mesh in the bind pose with identity world matrix. The output contains
actual positions, primary normals, UVs and triangle indices. B applies the
FreeSO coordinate normalization once. This crate copies the normalized DTO bit
fields and never applies another axis change or attachment transform from the
binding's provenance bone name.

`textures` contains encoded original PNG/JPEG bytes keyed by content SHA-256
`AssetKey`. Each rendered part references that bank. Signature detection is not
full image decoding: the browser/native decoder must reject corrupt payloads and
report failures to the application. Decoder output must be unpremultiplied RGBA,
original pixel order and original UV orientation. `validate_decoded_texture`
checks known source identity, dimensions, pixel count and limits.

Choice `thumbnail_keys` retains the original appearance thumbnail references;
`thumbnails` points only to resolved original thumbnail bytes. UV skin textures
are never used as character thumbnails. Missing thumbnails do not make geometry
unavailable; their missing/unsupported resources appear in import diagnostics.

## Limits and readiness

`ImportLimits.resources` controls bounded binary decoding. `max_files`, `max_choices`,
`max_total_input_bytes` and `max_total_resource_bytes` bound the imported set,
separately from C `avatar` render-part limits and `render` decoded-image limits.
The default 64 MiB per-resource limit applies to individual standalone/extracted
resources. Archive input uses B's 512 MiB input limit, so a 70 MiB original texture
archive is supported. Defaults do not restrict the original content to a fixed
number of outfits, heads or bodies.
Resource/name lookup uses one index, avoiding a whole content-set scan for each
binding. Collection choices have an aggregate budget, and repeated collection
specifications are rejected.

`import` returns `Err` for a fatal unsafe/corrupt container or global budget
failure. A safely read but incomplete content set returns `ImportedContent` with
`rig: None`, detailed `issues`, and/or unavailable choice skins. Successful
import is not a claim that every selection or every encoded image is ready.
`selection_issues` lists all missing dependencies reached by a particular
selection; `compose` also validates the actual C rig/mesh composition.

## Provenance and verification

The B crate is pinned to commit `04f0a407dd81acf0685453e7367764bf75b5c092`;
C render/avatar sources are pinned to `ca79bbd251491277d9fd5838d3f84d247675709a`.
The added readers use original FreeSO `Collection.cs`, `PurchasableOutfit.cs`,
`HandGroup.cs`, and `IoBuffer.cs` at
`60c6e823b8dc262d1c1f8a516970fb9b51fc516d`. Their big-endian integer layouts,
packed identity construction, opaque prefix bytes and source hand reference
order are preserved. `IoBuffer`'s default byte order is explicitly big-endian.
Official version-2 purchasables have an additional four opaque bytes left unread
by the native reader; this observed extension is retained, with other trailing
lengths rejected.

`cargo test -p wonderland-avatar-content` exercises independently selectable
head/body/skin chains using source-format byte vectors, unchanged original
texture bytes, missing and ambiguous resources, corrupt/bounded readers, FAR3
index/extraction, true thumbnail references and the RGBA decoder contract.
The fixtures verify format behavior; they are not supplied as user avatars or
claimed to be original game payloads.

### Actual original content verification

The native `probe` example was also run against the official original static
avatar subset extracted from the launcher installer: 23 original files totaling
97,445,301 bytes, including base body/head/hands resource archives and adult
skeleton files. The probe uses static archives plus the standalone `adult.skel`,
and inventories 21,425 original resources. Original game payloads remain external
to this repository.

| Explicit original collections | Choices | Ready skin choices | Example actual composed geometry |
| --- | ---: | ---: | --- |
| `ea_male_heads.col` + `ea_male.col` | 361 | 1,083 / 1,083 | Head outfit 929 + body 586: four parts, 473 vertices, 2,004 indices |
| `ea_female_heads.col` + `ea_female.col` | 461 | 1,383 / 1,383 | Head outfit 389 + body 2: four parts, 723 vertices, 2,718 indices |

Both probes successfully construct the C rig, resolve 8,394 distinct original
encoded texture/thumbnail assets, and compose head, body and both hands for
Light, Medium and Dark using their original texture references. This is native
source-content validation; browser image decoding and presentation are separate
integration gates.

One unrelated original wing mesh, `fabct_ow_b703fafit_winged-spine2-wing.mesh`,
remains rejected by the bounded B mesh reader for an out-of-range bone index.
The importer reports that issue explicitly. It does not affect readiness of
either tested pair of base creator collections and is not silently substituted.

Real-content probing exposed original FAR3 raw-entry and QFS size framing,
version-2 purchasable trailing bytes, and ignored mesh count metadata. The owning
source crates were corrected against native reader behavior and bounded
regressions before the successful probes. Original mesh count metadata is
retained; allocation, vertex data, binding ranges and limits use the actual
primary/blend counts.
