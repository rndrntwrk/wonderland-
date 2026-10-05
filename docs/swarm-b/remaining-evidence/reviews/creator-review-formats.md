# Independent Creator review by formats completion

Date: 5 October 2026. Reviewer: `/root/formats_completion`; author:
`/root/creator_completion`. Review does not modify author-owned source. Root alone
stages and publishes. Scope is the current remaining-implementation work against
original source pin `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

## Disposition

Independent review signed off after targeted fix verification on 5 October
2026. All four findings, including both directions of CR-F4, are resolved. The
original independent process probes were relinked against the author's fixed
libraries; all five relevant author regression cases were also independently
executed successfully. No blocking finding remains in the requested Creator
review scope. Final aggregate/build gates remain with the coordinator.

## Findings

### CR-F1 — glTF construction allocates before its JSON admission

Original reviewed locations: `tools/creator/src/editors/gltf.rs:129–134` and
`378–416`. `from_fsom` constructs nodes, meshes, materials, accessors and buffer
views for all geometries; `Builder::finish` clones the latter through `json!`
before `json_support::encode` finally applies the JSON budget. The binary budget
does not bound the much larger JSON graph for many small geometries.

Independent valid-input reproduction: 10,000 one-vertex geometries, unit normals,
source payload 600,052 bytes, total decoded budget 8,388,608 bytes. Encoding the
source payload succeeds. `from_fsom` eventually returns `editor JSON output limit
exceeded`, after process VmHWM rises from 4,264 kB to 237,432 kB. Thus the eventual
error does not enforce the promised pre-allocation browser/native memory bound.

Required correction: admit complete retained source, graph, temporary and binary
allocation before construction, and avoid serialization clones. Author reports
a nonallocating graph estimate plus structural admission and moved final arrays,
with a targeted failing regression observed before the fix.

### CR-F2 — shared animation samples silently overwrite conflicting edits

Original reviewed location: `tools/creator/src/editors/gltf.rs:633–649`.
Source motions may refer to overlapping global sample-array ranges. glTF exports
these as independent channel output arrays. Import writes each changed channel
back into the shared source slot in turn, while marking both arrays accepted.
The later channel silently wins; an unchanged alias can also be changed
implicitly by another channel.

Independent valid-input reproduction: two translation motions for BoneA/BoneB,
one sample each, both `first_translation_index = 0`; one shared translation
`[1, 2, 3]`. Change the exported X values to 4 and 5. Import succeeds, but
re-export gives `[5, 5]`, silently losing the first requested edit.

Required correction: compare all assignments to every shared source sample,
including unchanged aliases; reject conflicting values and edits that cannot
re-export their exact required f32 values. Author reports that correction and a
targeted failing regression observed before the fix.

### CR-F3 — BI_RGB32 padding is incorrectly interpreted and written as alpha

Original reviewed locations: `tools/creator/src/city/image.rs:335` and
`517–544`. For uncompressed 32-bit BI_RGB, the high byte is unused, not alpha.
Decoder currently reads it into alpha; encoder writes requested alpha into a
40-byte BITMAPINFOHEADER with compression zero, where interoperable readers
ignore it.

Independent literal input: a 58-byte 1×1 BMP with BGRX `[30, 20, 10, 0]`.
Creator returns RGBA `[10, 20, 30, 0]`. Independent Pillow identifies RGB and
returns RGBA `[10, 20, 30, 255]`. This can make ordinary RGB32 city maps fully
transparent and silently drops alpha when exported files are read elsewhere.

Primary specification: Microsoft BITMAPINFOHEADER documentation,
https://learn.microsoft.com/en-us/previous-versions/dd183376(v=vs.85).
The original application source `TSOClient/tso.files/ImageLoader.cs` delegates
BMP loading to `FSO.Windows/Program.cs` BitmapReader, which uses System.Drawing
and LockBits Format32bppArgb. An attempted independent C# runtime probe could not
compile because the System.Drawing assembly is absent; no original-GDI runtime
result is claimed.

Required correction: decode BI_RGB32 as opaque and either emit an interoperable
alpha format with explicit masks or explicitly reject transparent BMP writes.
Author accepted the finding, opened the primary specification, observed the
literal regression fail, and is implementing opaque BMP plus explicit alpha
rejection (PNG retains alpha).

### CR-F4 — sparse OBJ dynamic group ID bypasses pre-allocation admission

Original reviewed locations:
`tools/asset-cooker/src/interchange/obj.rs:433–435`, with late validation at 453.
The code admits the ID against max_entries, then grows the dense outer FSOm
vector to ID+1. Its actual retained allocation is checked only after conversion.

Literal 58-byte OBJ:

```text
v 0 0 0
vt 0 0
vn 0 0 1
o 99999_TEX_1
f 1/1/1 1/1/1 1/1/1
```

With default max_entries=100000 and max_total_decoded_bytes=524288, decode and
the preliminary text encoding succeed. to_fsom allocates 100000 Vec headers
(approximately 2.4 MB), then fails `LimitExceeded at byte 0: Vitaboy retained and
output allocation`. Independent process VmHWM rises from 1324 kB to 3596 kB,
exceeding the 512 KiB budget before rejection.

Required correction: preflight actual dense group slots plus retained model,
geometry output and conversion scratch before resizing or constructing them.

#### CR-F4 follow-up — reverse FSOm→OBJ corner expansion

After the sparse-ID fix, inspection of `ObjModel::from_fsom` found the same late
admission pattern in the reverse conversion. Independent valid FSOm input has
one vertex and 10,000 indexed triangles. Its 120,100-byte payload passes the
source codec under a 524,288-byte total budget. from_fsom builds the expanded OBJ
corner arrays, then fails `OBJ retained allocation limit exceeded`; process
VmHWM rises from 1600 kB to 2436 kB before rejection. Required correction is to
preflight retained source plus expanded corners, groups and output before
constructing the OBJ model. Reproducer is `review-formats/obj-export-memory.rs`,
and the observed pre-fix failure is `obj-export-memory-current.log`.

The author added source-capacity and expanded-array/map admission followed by a
nonallocating eventual-text-length count before model construction. The original
independent fixture now rejects at `FSOm to OBJ conversion admission exceeds
aggregate working memory`, with VmHWM 1532→1612 kB. Evidence is
`obj-export-memory-after.log`; the checked-in reverse-direction regression was
also independently executed successfully.

## Independent fix checks completed

Fresh probes were relinked directly against the author's current Creator/cooker
rlibs, with no duplicate dependency build. Results:

| Original finding | Independent current result |
| --- | --- |
| CR-F1 | Same 600,052-byte FSOm input and 8 MiB limit reject at graph admission. VmHWM stays at 4516 kB, instead of the pre-fix approximately 237 MB. |
| CR-F2 | Original conflicting alias edits reject with an explicit shared-source translation conflict. Author's checked-in test also covers unchanged aliases, consistent changes, rotations and exact re-export. |
| CR-F3 | Literal BI_RGB32 decodes `[10,20,30,255]`; encoder emits zero padding and independent Pillow reads it opaque. Nonopaque BMP output rejects; PNG retains `[10,20,30,128]` exactly. |
| CR-F4 sparse ID | Original 58-byte source rejects at conversion admission before table allocation. VmHWM 1372→1440 kB. |
| CR-F4 reverse expansion | Original 120,100-byte source rejects before expanded OBJ allocation. VmHWM 1532→1612 kB. |

The author reports the full Creator gate at 70 executable plus two doc tests
after these first four fixes. That broad suite was not redundantly executed by
the reviewer. Exact rechecked hashes and small before/after logs are retained in
`review-formats/`; source changes and fresh independent results resolve each
reported original rather than relying only on error-return assertions.

Final independent execution of the five checked-in focused regressions passed:
graph admission; shared translation/rotation aliases and consistent exact
re-export; RGB32 semantics and alpha policy; sparse dynamic groups; expanded OBJ
corner admission. Their complete output is `creator-fix-independent.log`.
Current reviewed source hashes are in `creator-final-file-hashes.json`.

## Reproduction evidence

Temporary independent crate:
`/workspace/scratch/378e4c36af7b/creator-review-formats/`.
Its `src/main.rs` has `memory`, `alias` and `bmp` modes. Dependencies are the
checked-out Creator and legacy-formats crates. Command:

```sh
PATH=/root/.cargo/bin:$PATH CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo build --offline --manifest-path /workspace/scratch/378e4c36af7b/creator-review-formats/Cargo.toml --target-dir /workspace/scratch/378e4c36af7b/formats-review-target
/workspace/scratch/378e4c36af7b/formats-review-target/debug/creator-review-formats memory
/workspace/scratch/378e4c36af7b/formats-review-target/debug/creator-review-formats alias
/workspace/scratch/378e4c36af7b/formats-review-target/debug/creator-review-formats bmp
```

The separate `obj-sparse.rs` was linked directly against the same reviewed
asset-cooker and legacy-formats rlibs to avoid rebuilding unrelated crates.
`obj-sparse` is its standalone executable. Literal BMP is `rgb32.bmp`.
Author-owned regression tests will be the checked-in reproducible fix gates;
these temporary process probes independently establish the reported failures.
Copies of every input source and completed observation log are now retained in
the report-relative `review-formats/` directory. That directory also contains
the literal RGB32 input and independent Pillow results. The old review Cargo
targets were cleaned to release space; checked-in author regression tests and
these small inputs remain reproducible.

## Other reviewed behavior

- JSON input: bounded bytes, depth, nodes and strings, duplicate keys rejected;
  unknown numeric values that serde cannot reserialize exactly are rejected.
  Edit admission includes source and edits before cloning; aggregate reparse
  checks final edited structure. No additional blocking issue found.
- Source-bound mesh OBJ edits: no-op export preserves original gzip bytes;
  comparison against the exported baseline retains untouched UV and float bits;
  source mesh clone preserves original submesh, vertex and index order.
- New OBJ conversion: sorted object-group traversal matches original
  DGRP3DMesh.cs's explicit OrderBy; mixed present/missing normals match the
  original DGRP3DGeometry.cs hasNormals branch. This is not a renderer claim.
- PIFF view uses the actual ordered resolver and its user-patch precedence,
  retains source/effective ordinals, checks duplicate resource keys, and uses
  source-aware indexed-IFF rebuilding before reopening the candidate.
- CLI edits re-read guarded source hashes before publication; filesystem writer
  uses exclusive temporary creation, complete write/sync, boundary recheck and
  atomic rename with cleanup on failure. No additional blocking issue found in
  reviewed publication paths.

Initial reviewed SHA-256 values:

```text
json_support.rs ed1873300dff5020a95cbd5fcbdccad0038e9e7f4b58c46ce166276e868caaa6
editors/gltf.rs 54000d1b5f7f25e9c7206619d27d1175a39e2bfea27177bc6e8cc5e1f4127e8c
city/image.rs 6853b9123030c67bbee9a47a5ad657d240522f91dae53f972effe7888c62da6f
patch_view.rs deef1c796a128f680e237924be005dede23617205265ed6599f649c2d15d3ccb
interchange/obj.rs 3f0121ea75445ec3725c2a5f78ed88e42952416071e26cd1c7302439134d67de
```
