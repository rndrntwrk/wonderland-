# Private authored original racing-banner setup

This source supplement passed the private native two-account action, shared-model
projection and persistence gates described below. It uses the retained original
FreeSO racing-banner resource and original behavior. Rendered browser pixels
remain a separate, unqualified acceptance gate. It does not implement lot purchase, roommate grants or player object
creation. The private operator-authored object owner is the first actual avatar
created and independently custody-checked by AccountAuthority. Both player
admission roles remain Visitor and lot ownership remains the source default 0.

## Trusted runner boundary

The optional private factory callback runs only on a fresh storage root, after
AccountAuthority/catalog initialization and actual account/avatar creation but
before coordinator initialization or accepting player work. It is not an HTTP,
browser, persisted manifest command or executable-path setting. Resume must not
invoke it. A failed partial setup must not silently recreate accounts.

Callback input is the frozen original manifest and its canonical SHA-256, the
pinned base host-config SHA-256, and the actual owner receipt below. It receives
no token/password. It may return only:

```json
{
  "host_config": "absolute new host-config.json path",
  "host_config_sha256": "SHA-256 of exact file bytes",
  "ownership_receipt": "absolute object-ownership-receipt.json path",
  "ownership_receipt_sha256": "SHA-256 of exact file bytes"
}
```

The runner preserves all other manifest fields, verifies receipt identity and
pins, closes the base host, opens the new pinned host and verifies zero owned
actors, admission readiness, original source-default permissions and identical
lot/content identity before coordinator initialization. It durably records the
final manifest and immutable setup/receipt/config hashes for future restarts.

## Exact owner receipt

All fields are required and unknown fields are rejected:

```json
{
  "schema": "wonderland.authored-object-owner-account.v1",
  "authority_id": "actual manifest authority digest",
  "content_release": "actual manifest content digest",
  "protocol_release": "actual manifest protocol digest",
  "account_id": "actual decimal account ID",
  "avatar_id": "actual decimal avatar ID",
  "avatar_spec_sha256": "canonical normalized ownedAvatar specification digest",
  "source": "AccountAuthority.createAvatar",
  "creation_operation": "actual createAvatar operation string",
  "canonical_create_result_sha256": "canonical entire createAvatar result digest"
}
```

Use the existing `services/native-edge/src/lot/contracts.ts` canonical JSON
semantics: recursive lexicographic object-key order, original array order, UTF-8,
no insignificant whitespace. The avatar-spec digest covers the actual successful
`ownedAvatar(token, avatar_id, false)` result with only `account_id` and
`avatar_id` removed. The create-result digest covers the entire actual result.
Independently require the custody result's account ID to match actual register/
login and its avatar ID/specification to match the successful create result.
The operation is the exact `[A-Za-z0-9_-]{16,96}` source operation. Avatar IDs
must be canonical decimal strings in 1..16777215, the source avatar-ID range.
No credential or synthetic identity appears in the receipt.

## Exporter interface

Run `native-authored-object-lot SETTINGS.json NEW_PRIVATE_DIR`. Settings schema 1
has exactly `base_manifest`, `base_manifest_sha256`, `base_host_config`,
`base_host_config_sha256`, `account_creation_receipt`,
`account_creation_receipt_sha256`, `original_banner_iff`, and `placements`, besides
`schema_version`. Input paths are absolute; hashes pin raw file bytes. The
callback can write the frozen manifest as canonical JSON for a stable byte pin.
Each placement contains `tile_x`, `tile_y`, `level`, `facing`; the bounded layout
is three adjacent north-facing source instances, intended at (2,3), (3,3), (4,3),
level 1. The original adjacency routines set attributes and graphics. No theme,
geometry, attribute or BHAV is fabricated.

The base must contain exactly the real unowned original vacation mailbox and no
avatar, within pristine source admission, under the verified four-source release
`cf3ac8024b207857eccf38b7311f19a6a9683e330d83d613c426cc0578600f99`.
The owner receipt's authority/content/protocol and the manifest's host/lot are
checked against pinned base inputs. Source provenance is recorded in
`2026-10-15-original-banner-release.json`; other releases require requalification.

The exporter uses accepted Spawn and original Init/Main for three banners, checks
source adjacency, then validates explicit source-intake ownership for the actual
avatar. It preserves mailbox owner 0, original object flags and all actor roles.
Original TTAB OBJECT_OWNER enforcement remains intact. Persistent object IDs
remain the original unallocated 0; no database purchase or allocation is simulated.

Outputs: settings, actual owner receipt, source-intake receipt, world document,
checkpoint, host config and `object-ownership-receipt.json`. The final receipt
schema is `wonderland.authored-object-ownership-receipt.v1`; it includes authority/
content/protocol/lot identity, all input/output hashes, cursor, owner account and
avatar IDs, `entities:4`, `rendered_banner_objects:3`,
`unrendered_original_join_anchors:1`, `owned_actors:0`,
`avatar_roles_changed:false`, and the detailed source-intake object.

## Original behavior and visuals

The original IFF SHA-256 is
`260a9db1ed017eacf2604beaf92dbf0cafae6c651b1f0296605eab2429503ab7`,
GUID `0x6378E9E6`. Original TTAB actions 4102 and 4104 set Theme ID to 0 or 2
and notify original main threads out of idle. Source adjacency determines the
piece and live Graphic. Endposts with piece >1 expose the original theme actions
subject to OBJECT_OWNER/ROOMMATES permissions. Default Green/Start endpoints
offer Set as Finish; the actual permissioned action completed in the private
acceptance run.

Original embedded DGRP 100..105, FSOM 100..105 and same-IFF MTEX texture data
supply the models. Exact original geometry, UVs, normals and PNG pixels are
retained. No sprite download or replacement texture is needed for this retained
banner. The separate original mailbox SPF remains absent. The renderer selector
uses source `BaseGraphicID + Graphic` with original u16 wrapping to choose the
same effective-IFF DGRP family, and refuses absent/ambiguous variants. This is
a conservative unavailable result outside the qualified source family: the
original renderer can retain its previous mesh when RefreshGraphic returns
false, whereas this stateless projection does not claim that cached fallback.
All six live graphic variants of this bounded original banner are present.

## Executed private acceptance

The fresh factory callback created two real accounts/avatars and bound the three
banners to the first independently verified avatar before coordinator startup.
Original Init/Main created the three-piece adjacency. Both actual owned actors
entered in world through the original mailbox and retained Visitor roles.

The owner selected original Local TTAB index 1, Manage/Set as: Finish. The
NativePlayer/WebSocket action was accepted and completed without external effects
or thread faults. The observer had no theme offers. Original behavior changed the
selected endpoint Theme attribute 0 from 0 to 2 and Graphic from 3 to 5. Both
clients subsequently selected the exact same original DGRP 105 instead of 103.
The original model assets, authoritative attributes and ownership survived host
restart, original source leave/save/control release, and same-avatar re-entry
under a new control generation. The returning owner received the original Start
offer. No test command wrote the banner attribute or graphic.

Both real sockets then closed for 85,001 ms without account activity, reconnect or
inspection. Their actual 60-second leases expired; the normal idle sweep drove
both original source departures, immutable saves and control releases. Final
state had zero owned actors/placements, no halt or delivery error, and unchanged
banner theme/owner. All three dedicated save/release outboxes were drained. Six
generic `events` notification rows remain and are not claimed delivered.

`2026-10-15-original-banner-acceptance.json` pins the private evidence hashes.
Private account/session/state data and asset bytes are excluded from Git. The
actual authored setup remains distinct from purchase, roommate and player
object-creation flows. Missing mailbox SPF remains explicit.

## WLB1 compatibility repair

The exact 1,132,152-byte original banner bootstrap exceeded the generic decoder's
work-item budget because each pixel was decoded as a nested four-channel array.
The WLB1-only model-image reader now decodes the existing pixel-count prefix and
each four-byte pixel as a fixed little-endian u32, then restores the same four
channels. WLB1 already mandates fixed-width little-endian bincode. All serializers,
JSON and generic image codecs are unchanged; byte/count/depth/appearance limits
are unchanged. Per-pixel allocation credit also bounds the temporary conversion.

The captured old bootstrap opens through NativePlayer and re-encodes byte-for-byte.
Tests cover non-symmetric channel bytes, mixed DGRP/Standalone/Vitaboy and texture
records, unchanged JSON, huge counts, truncated pixels and credit exhaustion.
Sealed content descriptor and post-original-TTAB GameRuntime descriptor are
required separately; the exact release provenance guard remains fail-closed.
