# Original animation metadata release adapter

The private original-content qualifier accepts an optional fourth argument:
an original-animation input manifest. Omitting it preserves the prior behavior
release flow. It never downloads assets or modifies source animation samples.

The version-1 JSON object has `schema_version: 1` and an `animations` array.
Each entry provides `owner` (u32), `table` (u16), `index` (u16), `resource`
(original ASCII `.anim` provider basename), `path` (relative to the manifest),
`byte_len`, and `sha256`. All fields are required and unknown fields are rejected.
The selection reference is the actual effective STR owner/table/index. It also
forms the existing runtime `AnimationKey`; the route producer resolves the
provider basename rather than depending on an invented routing identifier.

## Source and identity guarantees

- The bounded original decoder derives NumFrames as the maximum motion frame
  count. The adapter requires a positive representable count.
- Time properties retain source motion/list/item encounter order. They are
  deliberately not sorted by timestamp. The first duplicate property key wins,
  matching the source `PropertyList` indexer. Negative times are rejected because
  the existing runtime DTO cannot represent them.
- Each exact original payload is retained as a restricted, noncritical visual
  `VitaboyAnimation` resource in the same cooked manifest. All packs, including
  the visual pack, are exported.
- The metadata derived from those same bytes enters the existing sealed runtime
  binding. After sealing and loading, the exporter checks its provider identity
  against the effective STR reference and checks the actual runtime metadata
  against the decoded original. Failure occurs before output files are written.
- The qualifier's separate direct-import content and snapshot outputs use the
  same verified animation inputs, so they cannot silently omit the cohort that
  was admitted to the cooked release.
- The receipt records the original and metadata digests, manifest resource ID,
  source STR reference, frame count and event count. A browser original-content
  input must match that original digest and provider filename. The receipt does
  not assert that an independently selected browser file was loaded.
- Original samples and private input manifests remain outside Git. No raw
  original content is embedded in these generic sources or test fixtures.

The adapter limits a manifest to 1 MiB, 64 clips, 16 MiB per clip, and 64 MiB in
aggregate, in addition to the existing source decoder and cooker limits. It
rejects case-folded duplicate providers, duplicate STR references, changed
bytes, basename mismatches and paths resolving outside the manifest directory.

## Verification

Five synthetic adapter tests cover original frame derivation, provider identity,
first-match duplicate properties, event encounter order, negative times,
inconsistent frames, portable paths, duplicate inputs, changed-byte rejection,
effective STR mismatch and runtime metadata mismatch. The adapter and existing
qualifier compile with warnings denied against the shipping dependency set.

A private ten-clip adult land-route cohort was decoded from the already imported
original archive, including its running reference. The existing cooker and sealed
loader successfully admitted the new unified release. Original byte hashes,
effective STR references and loaded metadata matched for every clip. The previous
behavior-only release was left untouched.

The generic `native_original_motion_qualification.rs` probe accepts a private
release directory, its trusted binding digest, the original avatar GUID and a
private JSON output path. It uses the sealed loader and genuine source terrain
menu/SourceGoto queue, with no behavior edits or position mutation. On the
reviewed combined source, Walk completed at tick 113 and Run at tick 51. The
original 47-frame clockwise 90-degree turn occupied 47 Walk ticks and 24 Run
ticks. Both runs checked the original three-layer resources, exact speed/weight
rules, noninteger continuous positions, successful route/action completion and
final standing. Authority, replica and snapshot-restored state matched at every
tick. No missing-animation fallback, VM fault or external effect occurred.

The direct-import helper was also aligned with the shipping interaction motive
advertisements. Its complete content descriptor now equals the sealed release
loaded through GameRuntime, with all ten animation metadata entries present.

These checks establish cooking, metadata identity and isolated genuine source
route completion. They do not establish live browser animation, private desktop
transfer, two-account acceptance, publishing, deployment or a full merged
workspace regression pass.
