# Authored original mailbox join anchor

This supplement adds an explicit private source intake tool, not a shipped lot
layout or a new public creation API. It creates no avatar, account, role or lot
ownership. Original source permissions and checked placement remain in force.

## Source contract

The original `VMNetSimJoinCmd` recognizes GUID `0xEF121974` or `0x1D95C9B0`
and calls the original default Find Location For search around that entity.
Without either object it attempts the same checked fallback tile for each join.
`mailboxvacation.iff` is the recognized single-tile object `0x1D95C9B0`;
the other recognized mailbox is multipart and is outside this narrow intake.

Private inputs are already imported original files:

- `objiff.far!mailboxvacation.iff`, 9116 bytes at offset 6481037,
  SHA-256 `f00948388f143c22c08facd2bdf73896d44e41a1de6f223155acea6d40382de9`.
- `objotf.far!mailboxvacation.otf`, 698 bytes at offset 182499,
  SHA-256 `38c78d74067c7035dba0b820f2b470e6972592edebaca2857cb75385e6e06f6d`.
- `objectdata/globals/mailsemiglobal.iff`, 2200 bytes,
  SHA-256 `f1b83b1b07daba9921f709748b7dcf97615a8380f0e0ee60c88f2ea7d47f1de3`.

No original mailbox sprite archive is present. The anchor has no replacement
model, and the renderer's missing-model diagnostic remains visible. This does
not establish a fully rendered original mailbox.

## Content import

`original_semiglobal_namespaces` assigns explicit importer owners in declared
source order, skipping all object GUIDs and sharing only equal names, effective
IFF hashes and effective semiglobal tuning. Owner IDs are importer namespaces,
not original object GUIDs. Both raw qualification and sealed export use this
same calculation. The existing person scope keeps owner 1; the mailbox gets
owner 2. The sealed manifest retains source, patch, tuning and resource hashes.
The shared original-animation input is still parsed once and supplied to both
raw and sealed import.

## Private placement executable

`native_authored_join_anchor.rs SETTINGS.json NEW_PRIVATE_OUTPUT_DIR` accepts
schema 1 settings: `base_host_config`, its SHA-256, `original_mailbox_iff`,
`tile_x`, `tile_y`, and eight-way `facing`. All input paths are absolute. The
base must be a pristine empty, unowned lot with initialized source admission.
The sealed release must contain the single-tile original mailbox definition,
OBJf init 4101 and main 4096. The source IFF is pinned to the hash above. This versioned exporter also
requires the exact source-verified sealed binding recorded below and in
`2026-10-15-original-mailbox-release.json`. The shipping loader verifies that
binding against its manifest, packs and effective runtime content. A matching
GUID or init ID from a different release is rejected. The output receipt binds
the verified release and the compiled source-intake receipt hash.

The exporter uses accepted Spawn and executes the original Init/Main, including
original global 271 and 280. It refuses faults, effects, extra entities and
out-of-world placement. It then records the source-default object owner 0,
unallocated persistent ID 0, source-default TSO disabled flags 0 and empty
registered persistent groups through validated source snapshot intake. It
reinitializes the source admission metadata without creating an actor. It never
changes avatar flags or positions. Intended private placement is tile (2,2),
facing north, in the authored 8x8 lot.

Outputs are the pinned host config/checkpoint, settings, source receipt and
result. The receipt marks the layout as authored, identifies the original
source and preserves the missing-visual limitation. A new output directory is
required; resume and live mutation are not supported. Host config and snapshot
must be built with the same simulation schema; a schema-6 checkpoint must never
be downgraded to accommodate stale schema-5 build artifacts.

## Verification status at initial freeze

The four-source cook (TemplatePerson, destination, original racing banner,
vacation mailbox) succeeds with ten original animation clips. Sealed binding:
`cf3ac8024b207857eccf38b7311f19a6a9683e330d83d613c426cc0578600f99`.
The two initial namespace tests pass. The subsequent explicit tuning identity
case is pending the next serial compile. The placement exporter compiles with
warnings denied. Execution correctly refuses a schema-6 checkpoint against a
stale schema-5 shared runtime library. Placement and two authenticated in-world
admissions remain pending the matching schema-6 library refresh; no success is
claimed yet.
