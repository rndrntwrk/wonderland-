# Creator authoring, interchange and city data

The portable Creator library and CLI now provide guarded upgrade JSON, standalone
avatar metadata, FSOm mesh interchange, animation clip interchange, city images,
road strokes and neighborhood JSON. The APIs operate on supplied bytes and source
models. The CLI adds the existing workspace checks and atomic file publication.
The graphical companion lives in `tools/creator-web` and consumes these APIs.

This page covers the added authoring surface. [Creator](creator.md) describes IFF
resource transactions and [sprite packages](creator-sprites.md) describes the
existing contextual SPR2/PALT editor. Original source anchors on this page refer
to FreeSO revision `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

## Shared editing contract

Every edit is bound to the exact input SHA-256. A complete candidate is validated
and reopened before publication. Failed edits leave the document and an existing
destination unchanged. The CLI rereads the source before its atomic replacement;
glTF, OBJ and patch imports also recheck their supplied interchange inputs.
An unchanged standalone document exports its original bytes, including gzip
headers, compression choices and original JSON whitespace.

`upgrades-edit`, `neighborhood-edit` and `asset-edit` accept an array of explicit
JSON path edits. Path elements are field names or canonical decimal array indices.
An index equal to the array length appends an element. Removing a field or array
element requires `remove: true` and `value: null`. For example:

```json
[
  {"path":["Files","0","Upgrades","0","Price"],"remove":false,"value":"$750"},
  {"path":["Files","0","Upgrades","0","Hidden"],"remove":false,"value":false}
]
```

Objects must have unique keys. Positional arrays cannot substitute for objects.
Unknown fields in upgrade and neighborhood JSON are retained. Standalone binary
asset metadata has an exact codec schema, so unknown or omitted fields are an
error. Numbers whose exact decimal value would change in the JSON representation
are refused before editing; the tool never silently rounds an unknown large
integer or unusually precise decimal. The source JSON remains available through
ordinary workspace reads if its values are outside this editor's profile.

Editor JSON has a hard 1 MiB limit, further reduced by caller input, resource and
aggregate working-memory limits. Parsing checks at most 32 levels, bounded strings
and a caller-bounded total number of nodes. Conservative reservations include
retained source, edit values, model copies and serialized output. For compressed
FSOm, admission uses the claimed decoded size before decompression; the codec then
verifies actual DEFLATE extent, CRC and decoded length. OBJ/MTL output is counted
before allocating its buffer, including programmatically supplied public models.

## Graphical workbenches

[Creator in the browser](../../tools/creator-web/README.md) provides persistent
workbenches for these byte-backed APIs alongside the IFF editor. Choose a tool,
open its source file, inspect the source overview and apply an explicit edit.
**Export source** downloads the current document in its original format.
**Download inspection JSON** exposes the complete admitted metadata; the inline
preview is limited to 64 KiB and overview tables show the first 100 rows.

| Workbench | Main controls and resulting files |
| --- | --- |
| Upgrades | Select a file, level and substitution; edit target/replacement or level name/price; use advanced field edits for groups and configuration; export source-shaped JSON. |
| City painter | Inspect actual map pixels, choose a semantic layer, paint a categorical color or numeric sample, draw/erase paired road edges and corners, and export PNG or opaque BMP. The pixel preview uses decoded source samples. |
| Neighborhoods | Edit an explicit GUID, name, optional description and city coordinates; view origins in source order; query the actual nearest-neighborhood rule; export JSON. |
| Assets | Select the actual source format; inspect references and bit-exact metadata; edit stored vectors; inspect a bounded projection of mesh triangles; exchange FSOm OBJ/MTL and source-bound GLB/glTF. Attach the actual skeleton for animation interchange. |
| Patches | Open the original IFF, attach ordered official/user PIFF inputs, set the exact source name, inspect applied/suppressed provenance and export the effective IFF. |

Switching tools preserves each panel's source, selection and history. Successful
transform edits keep the selected element and reload its current values. FSOm
position edits recompute visible bounds in the same transaction, including an
ordinary depth mask and excluding a portal mask. An unchanged neighborhood form
retains an absent or null optional description. The common JSON editor uses
unsigned float bits; ordinary transform controls accept finite decimal binary32
values.

Advanced field-edit drafts carry the source hash they were prepared against.
Replacing the source, editing it elsewhere or restoring a different revision
clears an older draft and displays a notice. A stale captured draft cannot be
submitted against a replacement document. Undo and redo restore complete source
bytes; unchanged submissions create no history entry. Browser unload warnings
cover changed documents and retained skeleton or patch attachments.

Each additional panel admits at most 4 MiB per source or attachment and retains
at most 32 historical revisions within 8 MiB. Its 32 MiB working budget includes
retained source bytes, history, attachments and a reservation for graphical
previews and form text. Patch attachments share a 4 MiB total and a 32-file limit.
Parsing and conversion admission can impose smaller limits for a particular
format. Invalid file reads and candidates preserve the prior source and history;
the first PIFF descriptor is decoded before an attachment is retained.

The native session boundary is
`wonderland_creator_web::workbench::{WorkbenchSession, InputKind, ReadTarget}`. Uploads use
generation tickets. `apply_neighborhood_fields` and `apply_transform` implement
the ordinary forms; `apply_json_text` implements explicit path transactions;
`import_exchange` and `export` call the same portable codecs used by the CLI.
The UI reads user-selected files and downloads results without a server-side
document store. Reloading the page ends the editing sessions.

Stored mesh projections show up to 500 triangles before skinning and material
rendering. Runtime instruction controls, connected lot rendering and complete
avatar playback retain the provider boundaries documented in the
[runtime extension contract](runtime-extension-contract.md).

## Upgrade authoring

```sh
creator --root private-work upgrades-inspect upgrades.json
creator --root private-work upgrades-edit upgrades.json candidate.json SOURCE_SHA edits.json
```

`editors::upgrades::UpgradeDocument` retains the source-shaped `Version`, `Files`,
`Subs`, `Groups`, `Upgrades` and `Config` representation. It validates tuning
targets, literal and referenced replacements, price syntax, booleans, GUIDs and
configured level ranges. Empty or duplicate file identities and duplicate config
GUIDs are refused as ambiguous authoring input. This is an authoring restriction;
the original runtime can overwrite duplicate config GUIDs.

`resolve_substitutions(name, level, lookup)` implements the source's ordered
expansion. Group defaults are inserted first, direct substitutions next, and
group-targeted substitutions last. Repeated destinations take their last value.
An invalid group reference is ignored before its replacement is parsed, matching
the original runtime and the checked-in `Content/upgrades.json`. `V` replacements
must fit signed 16-bit values. `Ctable:index` uses the supplied source tuning lookup;
a missing constant resolves to zero. The caller supplies the same global, private
and semiglobal tuning view used by the source object.

The resolver does not select an object's effective upgrade level or resolve prices
through a live catalog. JSON editing preserves those configuration fields; runtime
catalog and object-provider behavior remains a separate integration.

Sources: `tso.content/Upgrades/Model/*.cs`, particularly
`Runtime/RuntimeUpgradeFile.cs::GroupsIntoSubs/LoadSubs`, `ObjectUpgradeProvider.cs`
and `FSO.IDE`'s upgrade editor. The checked-in upgrade file is an acceptance input;
original assets are read only.

## Standalone asset metadata

```sh
creator --root private-work asset-inspect mesh avatar.mesh
creator --root private-work asset-export animation contact.anim unchanged.anim
creator --root private-work asset-edit mesh avatar.mesh edited.mesh SOURCE_SHA edits.json
creator --root private-work asset-inspect fsom object.fsom
creator --root private-work asset-inspect nbhm neighborhood.nbhm
```

`editors::assets::{AssetDocument, AssetKind}` supports `mesh`, `animation`,
`skeleton`, `binding`, `appearance`, `outfit`, `purchasable-outfit`, `collection`,
`hand-group`, `fsom` and `nbhm`. Each import uses the actual bounded legacy codec.
Floating-point fields are exposed as their unsigned IEEE 754 binary32 bit patterns,
so signed zero and unchanged transform bits remain distinguishable. For example,
decimal integer `1075838976` represents the float `2.5`.

Edits must satisfy the source's indices, counts, hierarchy, version and flag
constraints. A candidate must encode and decode to the requested complete metadata.
Invalid or nonfinite transforms cannot be committed. Mesh changes preserve other
vertices, faces, bone bindings and blend records. Animation changes retain ordered
motion properties and time-property/contact-event records, including duplicate
keys. Actual checked-in mesh and animation files exercise one-field changes.

FSOm is a gzip-wrapped source mesh format. NBHm stores ordered house placement
records and its model flag. Raw source order survives metadata editing; duplicate
NBHm house identifiers retain the original codec's last-record lookup semantics.
This API does not create a runtime avatar or resolve its referenced files.

The shared format/cooker layer additionally exposes source BCF/CMX, BMF/SKN and
contextual CFP APIs. Those containers are not aliases for complete standalone
animations. CFP requires translation and rotation counts from its BCF animation;
the original TS1 provider resolves `(XSkillName + ".cfp").ToLowerInvariant()`.
Direct Creator JSON commands for those contextual containers are not included in
this authoring tranche.

Sources: `tso.vitaboy.model/{Mesh,Animation,Skeleton,Binding,Appearance,Outfit,
PurchasableOutfit,Collection,HandGroup}.cs`, source BCF/BMF/CFP readers and
`tso.files/RC/{DGRP3DMesh,DGRP3DGeometry,DGRP3DVert,NBHm}.cs`.

## FSOm, OBJ and MTL

```sh
creator --root private-work mesh-from-obj authored.obj authored.fsom ObjectName
creator --root private-work mesh-obj-export object.fsom object.obj
creator --root private-work mesh-mtl-export object.fsom object.mtl
creator --root private-work mesh-obj-import object.fsom edited.fsom SOURCE_SHA object.obj
```

`interchange::obj::ObjModel` reads the original triangulated OBJ profile: finite
positions, UVs, optional normals and positive `position/texture[/normal]` corner
indices. Object names identify dynamic groups and source texture references:

| Object name | Meaning |
| --- | --- |
| `0_SPR_rot2_3` | Dynamic group 0, source DGRP rotation 2, sprite index 3. |
| `0_TEX_7` | Dynamic group 0, custom texture ID 7. |
| `DEPTH_MASK` | Normal depth-mask geometry. |
| `DEPTH_MASK_PORTAL` | Portal mask; excluded from the visible bounds calculation. |

The importer sorts object groups as the source does, deduplicates identical
position/UV/normal corners, reverses OBJ texture V, and generates accumulated face
normals when a submesh contains no authored normals. Degenerate generated normals
are refused. Nontriangular faces, missing texture coordinates, negative indices
and unknown directives are explicit errors. Source-ignored `g` and `s` directives
remain ignored. Material directives do not cause filesystem reads.

`MeshOverrideDocument::import_obj` edits attributes in the exported source-bound
layout. Object identity, face topology and vertex mapping must remain unchanged.
Unchanged attributes keep their original bits, including UV values that cannot
survive an unnecessary `1 - (1 - v)` conversion. Bounds are recalculated after
position edits. A no-op preserves the complete original compressed source; changed
meshes use the deterministic source-compatible gzip encoder. New topology uses the
explicit `mesh-from-obj` operation.

MTL output uses the source's diffuse/alpha PNG naming convention and material
properties. `decode_mtl` inspects that bounded profile with basename-only texture
references. It does not fetch referenced images or resolve texture-provider
aliases. glTF materials preserve source texture IDs as metadata; this tranche does
not embed sprite-derived or custom texture images into the exported model.

Sources: `tso.files/RC/OBJReader.cs`,
`DGRP3DMesh.cs::DGRP3DMesh(OBJ)/SaveOBJ/SaveMTL`, `DGRP3DGeometry.cs` and
`FSO.IDE/ResourceBrowser/FSOMEditor.cs`.

## glTF and GLB exchange

```sh
creator --root private-work mesh-gltf-export object.fsom object.glb glb
creator --root private-work mesh-gltf-import object.fsom edited.fsom SOURCE_SHA object.glb
creator --root private-work animation-gltf-export contact.anim avatar.skel contact.gltf gltf
creator --root private-work animation-gltf-import contact.anim avatar.skel edited.anim ANIM_SHA SKEL_SHA contact.gltf
```

`editors::gltf::GltfPackage` supports one embedded binary buffer. GLB import checks
version 2, declared lengths, aligned JSON/BIN chunks, padding and complete input
consumption. JSON glTF uses a canonical base64 data URI. External buffer files,
sparse accessors, compressed geometry and required extensions are outside this
profile. No URI is fetched. Dense float accessors require finite, aligned and
bounded values. This is a source-bound authoring exchange, not a general glTF
scene importer.

FSOm exchange exports the stored positions, unit normals and UVs with exact source
index buffers and dynamic/material identities. Position, normal and UV values are
editable. Index buffers, layout, source identity, scene graph and depth-mask
metadata remain protected. Invalid normals are refused instead of changing source
bits by implicit normalization. Version 1 FSOm has no stored normals and needs an
explicit authored normal field in a newer source layout before this glTF path.

Animation exchange requires the actual animation and its skeleton. It uses source
36 Hz timestamps, the original `(-Z,-X,Y) * (1f/3f)` coordinate conversion and ROOT
quarter-turn orientation. Node scales remain one; editor-only bone-length display
scales are omitted while retaining the same geometric local transform convention.
ROOT quarter turns use their exact half-component quaternion representation, so
there is no claim of byte equality to the C# exporter's trigonometric intermediate
values. Ordered motion/time properties remain protected metadata. Only sampled
transform outputs are editable; timestamps, graph and metadata changes are rejected.
Unchanged source transforms are never inverse-converted unnecessarily.
Every edited transform must re-export with the requested binary32 bits. If
multiple motion channels share one source sample, all of their requested values
must agree after the source coordinate conversion; an unchanged alias also
constrains that sample. Conflicting edits fail before the document changes.

Export admits source vector capacities, all generated geometry/channel graphs,
binary buffers and simultaneous conversion copies before allocating graph nodes.
Publicly constructed packages receive depth, node-count and retained-memory checks
before comparison or cloning, as well as the byte and accessor limits.

The exchange preserves contact-event metadata and verifies transform conversion;
it does not assemble a skinned avatar, retarget arbitrary skeletons or prove live
renderer/contact execution. These depend on the actual avatar/runtime providers.

Sources: `FSO.IDE/Utils/{GLTFExporter,GLTFImporter}.cs` and the
[Khronos glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html).

## City images, road strokes and neighborhoods

```sh
creator --root private-work city-image-inspect roadmap.bmp
creator --root private-work city-convert roadmap.bmp roadmap.png png
creator --root private-work city-paint terraintype.png painted.png SOURCE_SHA terrain 20 30 3 255 0 0 png
creator --root private-work city-road roadmap.png roads.png SOURCE_SHA 20 30 5 0 draw png
creator --root private-work neighborhood-inspect neighborhoods.json
creator --root private-work neighborhood-edit neighborhoods.json edited.json SOURCE_SHA edits.json
creator --root private-work neighborhood-nearest neighborhoods.json 20 30
```

`city::CityImage` supports lossless 8-bit pixel samples from PNG and Windows BMP.
PNG palette, grayscale, grayscale-alpha, RGB and RGBA data expand to RGBA samples.
Animated PNG and 16-bit PNG require an explicit external conversion. BMP accepts
40/108/124-byte DIB headers, uncompressed indexed 1/4/8-bit and RGB24/RGB32 pixels,
plus source-compatible RLE4/RLE8 streams. Palette indices, row extents, delta moves,
absolute-run padding and the end marker are checked. Output PNG and BMP normalize
the image envelope; they preserve the admitted samples, not source compression or
BMP padding. The older `BmpMap` path remains available for exact untouched BMP
envelopes and precise raw pixel edits.

For BI_RGB32, the fourth stored byte is unused and decoded colors are opaque, as
defined by [Microsoft's BITMAPINFOHEADER specification](https://learn.microsoft.com/en-us/previous-versions/dd183376(v=vs.85)).
Canonical BMP output writes that unused byte as zero. Transparent RGBA images must
use PNG; the BMP writer rejects them before allocation instead of losing alpha.

Categorical brush paint uses the original cosine footprint and clips in two
dimensions. Road drawing and erasing implement both sides and the original corner
removal tables for directions 0 through 3 (`+X`, `+Y`, `-X`, `-Y`). A stroke that
leaves the image fails before any pixel is published; it never wraps into an
adjacent row. CLI city authoring requires 512 by 512 dimensions, opaque samples and
the selected source layer's palette. Elevation and forest-density value painting
are available; the original temporal raise/lower and median-flatten brush state
machines are not included here.

`NeighborhoodDocument` preserves source JSON fields, explicit unique GUIDs,
locations, optional colors and `DistanceMul`. It does not generate random GUIDs or
write the server database. Nearest-neighborhood lookup uses the source's squared
distance and first-entry tie rule; the stored `DistanceMul` is not used by that
source lookup. `assignment_map` exposes the bounded 512 by 512 result for a UI.

Sources: `CityMapData.cs`, `Plugins/MapPainterPlugin.cs`,
`Plugins/NeighbourhoodEditPlugin.cs`, `Model/CityNeighbourhood.cs`,
`CityNeighGeom.cs::NhoodNearest`, and `Documentation/Crafting a City.md`.

## Source and effective patch views

```sh
creator --root private-work patch-inspect object.iff Object.iff patches.json
creator --root private-work patch-export object.iff effective.iff Object.iff SOURCE_SHA patches.json
```

`patches.json` is an ordered array such as
`[{"path":"official.piff","is_user":false},{"path":"local.piff","is_user":true}]`.
`patch_view::PatchView` invokes the actual content resolver, exposes applied and
suppressed patch provenance, and compares source/effective resource ordinals,
payload hashes and complete chunk identity. Source-name matching is exact. A
matching user patch suppresses matching official patches. Existing chunk flags
remain unchanged when a PIFF descriptor merely supplies replacement flags, as in
the original resolver. Unknown resources and unrelated bytes survive.

The content resolver currently accepts unindexed source IFFs for patch application.
Indexed resource-map editing remains supported by ordinary Creator transactions,
but patch-chain application to an indexed source is an explicit resolver gap.
Patch view/export does not synthesize new PIFF descriptors.

## Cooking and acceptance

The cooker validates actual standalone mesh/avatar/reference formats, FSOm, NBHm,
PNG and BCF/CMX/BMF/SKN through their named codecs. The newly added reference and
container formats remain noncritical Visual resources. BCF metadata does not become
a complete simulation animation without its contextual CFP samples.

Embedded FSOM is imported with no external replacement texture provider supplied.
It resolves the same-ID DGRP, the exact `GetImage(1,3,rotation)` sprite selection,
or same-IFF MTEX custom texture references. The original renderer consults an
external replacement provider before MTEX; separate PNG source entries do not
implicitly register that provider in this importer. PNG MTEX is decoded Visual
data; unsupported JPEG/BMP MTEX remains explicit opaque data. External replacement
texture providers are not guessed. The manifest includes these dependencies and
their transitive palette closure.

`tests/tools/extended_authoring.rs` and `extended_cli_e2e.py` exercise original
assets, independent literal fixtures, candidate reopen, unchanged-bit retention,
ordered events, exact source-name/user-patch precedence, malformed transfers,
aggregate admission and late atomic failures. Recooking proves changed texture
payloads affect dependent mesh cache keys while an independent control source keeps
its key. Repeated builds retain identical pack bytes. This is content-key
propagation evidence; live renderer cache eviction is a separate consumer test.

Run the complete Creator test suite and strict static checks using the commands
in [Creator](creator.md). `docs/compat/tools.json` records remaining tool and
extension gaps explicitly, including actual runtime interfaces that are still
absent. No broad W16 parity claim follows from these bounded workflows alone.
