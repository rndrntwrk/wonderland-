# Source player authoring

This DOM-free crate translates supplied source state into original FreeSO
authoring requests. It does not run a VM, charge a balance, manufacture inventory,
or treat transport delivery as transaction success.

## Packet boundary

`encode_client_command(actor, &SourceCommand)` writes the original complete
VMNetCommand: one-byte type, little-endian ActorUID, then the original
BinaryWriter body. `validate_client_command(bytes, actor)` rejects actor spoofing,
unsupported/server-only command classes, malformed or trailing data, and
server-populated client fields. `decode_command_prefix` supports safely consuming
the implemented command forms inside an unframed source tick. An unknown command
cannot be skipped by scanning for a later type byte.

Implemented player tags are architecture 2, buy 3, move 8, delete 9, send to
inventory 21, place inventory 22, and roof 32. SetOutfit 33 is decoded only as a
server observation; client submission of 33 is forbidden. Architecture preserves
all ten original command types and the exact 22-byte command body. Actual source
VM verification, collision, catalog restrictions and global-link transactions
remain authoritative.

## State and identities

`AuthoringSnapshot` contains only data provided by an actual source adapter.
The authenticated avatar, optional database lot ID, packed map location, browser
epoch and native lot incarnation stay distinct. A selected object uses an
`EntityIdentity` and is revalidated when a request is prepared.

`AuthoringState::observe_snapshot(actor, source, presentation_generation)` merges
a decoded version-38 StateSync after checking its packed platform lot location
and actual selected avatar. It reads exact budget/permission, bounds, ownership,
donation and transaction bits, GUIDs and object poses. It preserves current EOD
stock and supplied catalog/resources. The generation is a local refresh/pick
token: it is never represented as an authoritative VM revision. Old picks and
world drafts expire on refresh. A generic refresh leaves unconfirmed commands
unknown and blocks duplicates.

FSOv does not carry the result of `IsUserMovable`, owned inventory, original
catalog prices or computed purchase capacity. The refresh adapter leaves those
unknown; it does not infer them from rendered geometry. `SourceObject::movable`
is `Option<bool>`. An actual source content/permission adapter may supply the
missing result. Administrator move/delete bypasses follow original Verify rules;
inventory return still requires movability and no incomplete transaction.

`install` accepts an explicit authoritative authoring projection.
`submit` captures its actor/context and observation revision. `receive` admits
only a correlated receipt for that request/context and a newer source projection
for acceptance. Rejection preserves the draft. Unknown retains pending state;
costs are never optimistically deducted. Local observation revisions order these
projections and are not server command sequence numbers.

## Wardrobe and clothing racks

An actual actor-matched `eod_enter` admits the original dresser/rack plugin and
its incarnation. Stock is decoded from the original big-endian VMGLOutfit wire:
29 bytes per record, including two-byte enums. Original 64-bit outfit assets
serialize as decimal strings, avoiding JavaScript number precision loss.

Dresser uses owned record IDs for wear/delete and `category,id` for defaults.
Default categories and the rule retaining the last outfit in clothing categories
come from the original plugin. Rack purchase uses `id,true` or `id,false`, exactly
as the original plaintext event. Prices remain source values. Rack owner price
edits retain the original 1 through 999999 validation; stock/delete/price edits
require the actual rack owner. Source stock object PID can join to a StateSync
object owner; owner-plugin visibility alone does not fabricate ownership.

`observe_eod` accepts actual stock effects and explicit source rack errors.
Generic show/default-refresh events do not confirm edits. `observe_set_outfit`
uses the target avatar UID, scope and exact asset to confirm dresser effects;
the original server command may have ActorUID zero. There is no universal source
operation ID for these events, so only a matching outstanding action/effect is
resolved. A closed/replaced dialog invalidates unconfirmed EOD actions.

Rack try-on uses the actual `rack_show` type: source clothing racks emit dynamic
costume scope25 and decoration racks emit scopes8 through11. Only that expected
UID/scope/exact asset resolves the outstanding try-on. An explicit stop-waiting
action archives an uncertain request in `unknown_operations`; it does not replay
the packet. Closing an object dialog can archive an older pending action before
sending the separate close request.

## Scene building and stale world data

`BuildTileDraft` implements a two-click equivalent of the original start/end
gesture for wall line/delete/rectangle and full-tile floor rectangles. Fill
commands use a single picked tile. Wall line length/direction follows original
ties-to-even Euclidean rounding and eight-way angle quantization; the actual
snapped end vertex is checked against source dimensions. Rectangle fields retain
source min coordinates and deltas, not tile counts. Walls use a tile's top-left
vertex, visibly identified by the browser controls. Source rectangle floors use
inclusive full cells; a single diagonal half requires an edge selection and is
explicitly unavailable from tile-only input. Source FSOv wall segments identify
diagonal tiles without guessing from rendered artwork.

Wall-side dot painting, terrain height and grass intensity tools need additional
original inputs and are explicitly unavailable from this tile-only helper.
Drafts reset on cancellation, resource/context/level changes and snapshot refresh.
Drafting never mutates the displayed architecture or computes a fake source quote.

`invalidate_world_projection` must run for unreplayed/invalid VM frames. It clears
world-derived authority and blocks raw edits until a new actual StateSync while
preserving active wardrobe stock and pending/unknown writes. EOD observations
cannot revive world freshness. Original ObjectID can be reused within the same
lot: the entity selection generation is strictly a local stale-pick guard, never
a claimed source entity incarnation.

## Original source references

- `TSOClient/tso.simantics/NetPlay/Model/VMNetCommand.cs` and command bodies
- `TSOClient/tso.simantics/Model/Platform/VMDefaultValidator.cs` and community validator
- `TSOClient/tso.simantics/Model/TSOPlatform/VMTSOObjectState.cs`
- `TSOClient/tso.simantics/NetPlay/EODs/Handlers/VMEODDresserPlugin.cs` and rack plugins
- `TSOClient/tso.simantics/Engine/TSOGlobalLink/Model/VMGLOutfit.cs`
- `TSOClient/tso.content/WorldObjectCatalog.cs` and original catalog XML

Original source and assets are not edited. Native tests identify serializer
fixtures as fixtures and include actual original catalog parsing. Passing these
tests proves the implemented boundary, not a successful live account transaction.
