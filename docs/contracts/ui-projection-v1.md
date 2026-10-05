# UI presentation projection v1

`wonderland-contracts` defines serialized presentation snapshots, intent messages, and adapter replies. `wonderland-client-app` supplies a pure reducer and the repository fixture. Neither crate depends on the browser, renderer, simulation, or a network transport. Rust 1.99.0 and edition 2024 are required.

This boundary is a preview integration contract. It is not a frozen multiplayer wire protocol, a world snapshot, or a permission system. A future authenticated adapter must identify the actor independently, authorize travel/interactions/cancellations, and validate target generation and revision against authoritative state. The `character_id` in a UI request expresses selected presentation context only.

## Exported Rust API

All contract types are exported from the root of `wonderland_contracts`. IDs are `CharacterId`, `PlaceId`, `ObjectId`, `ActionId`, and `OperationId`; each is a distinct transparent `String` newtype with `From<&str>`, `From<String>`, `AsRef<str>`, and `Display`. They compare and sort by their opaque value. `EntityRef { id: ObjectId, generation: u64 }` identifies an object's stable ID and incarnation; it never contains a DOM node or an engine entity index.

`wonderland_client_app` exports:

```rust
pub fn preview_projection() -> UiProjection;
pub fn ShellState::new(projection: UiProjection) -> Self;
pub fn ShellState::dispatch(&mut self, intent: UiIntent)
    -> Result<Vec<UiRequest>, UiError>;
pub fn ShellState::receive(&mut self, event: UiEvent)
    -> Result<(), UiError>;
```

The `ShellState` fields below are public for straightforward rendering. Treat them as read-only; apply changes through `dispatch` and `receive`. Direct mutation bypasses reducer invariants.

| Field | Type | Meaning |
| --- | --- | --- |
| `screen` | `Screen` | `CharacterSelection`, `City`, or `Lot { place_id }` |
| `selected_character` | `Option<CharacterId>` | Selected character; retained on Back |
| `selected_place` | `Option<PlaceId>` | Selected map destination; retained when returning from a lot |
| `selected_object` | `Option<EntityRef>` | Current anchored action target |
| `pending_requests` | `BTreeMap<OperationId, PendingRequest>` | Requests awaiting their matching replies |
| `queue` | `Vec<QueuedAction>` | Acknowledged interactions only |
| `projection` | `UiProjection` | Current bounded presentation snapshot |
| `last_error` | `Option<UiError>` | Initial validation, latest dispatch, or rejection feedback |

An invalid initial projection remains readable and leaves the shell on character selection. Dispatch validates the snapshot and refuses enabled transitions until valid data is supplied. Empty character/place/object lists are valid and never cause a selection panic. The bundled fixture is compile-time included; malformed checked-in JSON is a developer error caught by the fixture tests.

## Projection schema

`UiProjection` implements Serde serialization/deserialization, equality, clone, and `validate() -> Result<(), UiError>`. The checked-in example is `fixtures/ui/preview-v1.json`.

| Field | Type | Meaning |
| --- | --- | --- |
| `version` | `u32` | Exactly `1` |
| `revision` | `u64` | Positive monotonic snapshot revision |
| `city_name` | `String` | Display name |
| `characters` | `Vec<Character>` | Up to 64 presentation characters |
| `places` | `Vec<Place>` | Up to 128 selectable destinations |
| `objects` | `Vec<SceneObject>` | Up to 1,024 object presentations |

`Character` contains `id`, `name`, `availability`, `money: i64`, and `needs: BTreeMap<Need, u8>`. Every character must have exactly Energy, Hunger, Fun, Social, Hygiene, Bladder, Comfort, and Room. Serialized need keys retain those labels, and values are integers from 0 through 100. `Need::ALL` exposes the eight typed variants.

`Place` contains `id`, `name`, `population: u32`, `availability`, and `anchor: Anchor`. `Anchor { x: f32, y: f32 }` uses finite normalized coordinates from 0 through 1. Map and scene views apply their own shared illustration transform; coordinates do not encode viewport pixels, picking implementation, or camera state.

`SceneObject` contains `target: EntityRef`, positive `revision: u64`, `place_id`, `name`, `anchor`, and up to 16 `offers: Vec<ActionOffer>`. `place_id` must reference a projected place. Object IDs are unique across the snapshot. An `ActionOffer` contains `id: ActionId`, `label`, and `availability`; IDs are unique within an object. A target's incarnation changes with its generation; offer/object state changes must advance its revision.

Availability is internally tagged:

```json
{"status":"available"}
{"status":"unavailable","reason":"The coffee machine is already clean"}
```

Selecting an unavailable character or destination can expose its explanation, but Play/Visit are refused. Unavailable offers remain readable, and TakeOffer returns their reason. `Availability::is_available()` and `ensure_available()` support views.

### Validation and limits

`validate()` rejects unsupported versions, zero revisions/generations, empty or duplicate IDs, missing object-place references, duplicate offers, invalid need coverage/values, nonfinite/out-of-range anchors, or oversized collections/text. IDs are 1–64 ASCII bytes containing letters, digits, `-`, `_`, `.`, or `:`. Names, city names, and action labels are nonblank, contain no control characters, and are at most 128 UTF-8 bytes. Unavailable reasons have the same text restrictions and at most 256 UTF-8 bytes.

Validation occurs after deserialization. These limits do not prevent a deserializer from allocating for oversized input first; an external adapter must cap source bytes before decoding. Unknown JSON fields are currently accepted by Serde for forward extension; unknown enum values are rejected. No live decoding path is included in this increment.

## Intents and reducer behavior

`UiIntent` is tagged by `intent` with payload in `data`, using snake_case variant names:

| Variant | Payload | Effect |
| --- | --- | --- |
| `SelectCharacter` | `CharacterId` | Select an existing card on character selection |
| `Play` | None | Require selected available character, enter City locally |
| `SelectPlace` | `PlaceId` | Select an existing destination in City; another selection invalidates pending travel |
| `Visit` | None | Require available selected destination; emit Travel, remain in City |
| `SelectObject` | `EntityRef` | Select an object in the current lot; another selection invalidates pending interactions |
| `DismissObject` | None | Clear the anchored menu and invalidate pending interactions |
| `TakeOffer` | `{ target, action_id, expected_revision }` | Verify current selected generation, revision, offered action, and availability; emit Interaction |
| `Cancel` | `{ operation_id }` | Refer to an acknowledged queue item; emit distinct Cancellation and mark it pending |
| `Back` | None | Invalidate in-flight work and current lot queue; Lot → City or City → CharacterSelection |

Back from a lot keeps the selected character and destination, clears the object selection, and clears this lot's acknowledged presentation queue. This is local view cleanup, not a claim of remote cancellation. City Back keeps the character card selected and clears the destination.

Repeated Visit while travel is pending emits no additional request. A duplicate TakeOffer already pending or acknowledged for the same target/action also emits no request. Repeated Cancel while its cancellation is pending emits no request. Wrong-screen actions, missing selections, unknown IDs, stale generations/revisions, invented actions, and unavailable actions return a typed error and do not submit requests.

At most 32 requests can be pending, and at most 32 acknowledged/pending interaction slots can exist together. Pending interactions reserve queue capacity. Cancellations remain possible when the acknowledged queue is full. Request sequences use checked arithmetic and are never reused during a shell's lifetime.

## Requests, queue, and replies

`UiRequest { operation_id, projection_revision, kind }` contains a unique `ui-{sequence}` operation ID and the current global projection revision. Operation IDs are scoped to a `ShellState` lifetime. An adapter must dispose of callbacks when replacing the shell; it must not replay a previous shell's operations into a newly constructed shell.

`RequestKind` is tagged by `kind`, using snake_case names:

| Variant | Fields |
| --- | --- |
| `Travel` | `character_id`, `place_id` |
| `Interaction` | `character_id`, `place_id`, `target`, `action_id`, `expected_revision` |
| `Cancellation` | `queue_operation_id` |

`PendingRequest { request, status }` has `RequestStatus::Pending`. Successful interactions enter `QueuedAction { operation_id, target, action_id, expected_revision, label, status }`. Queue status is `QueueStatus::Active` or `QueueStatus::CancellationPending`. A pending request never masquerades as an acknowledged queue item.

`UiEvent` is tagged by `event`, using snake_case names:

| Variant | Fields | Application |
| --- | --- | --- |
| `Accepted` | `operation_id`, `projection_revision` | Matching Travel enters its lot; matching Interaction creates exactly one queue item |
| `Rejected` | `operation_id`, `projection_revision`, `reason` | Remove matching pending work; retain City for travel; restore active queue status for cancelled work that was rejected |
| `CancellationAcknowledged` | `operation_id`, `projection_revision` | Must identify the cancellation request, not the original interaction; remove that request's queue item |
| `ProjectionUpdated` | `projection` | Validate and apply strictly newer snapshots; invalidate all pending work |

Acceptance is not cancellation acknowledgment. Generic `Accepted` for a cancellation is ignored while waiting for the explicit acknowledgment. An acknowledgment referring to a travel or interaction operation is ignored. Duplicate, unrelated, old, or wrong-revision replies are harmless no-ops; the reducer does not surface them as player errors.

A newer valid snapshot clears all pending requests, resets surviving cancellation-pending queue entries to active, and drops acknowledged entries with changed/removed target generation or object revision. Removed/unavailable selected characters return to character selection; removed/unavailable destinations return a lot view to City. Changed/removed selected target generations or revisions clear the object menu. Invalid snapshots leave current state untouched and return `InvalidProjection`; equal/older valid snapshots are ignored.

`UiError` variants are `InvalidProjection(String)`, `NoSelection`, `WrongScreen`, `UnknownSelection`, `Unavailable(String)`, `StaleTarget`, `UnofferedAction`, `Rejected(String)`, and `OperationLimit`. All implement equality, Serde, Display, and the standard Error trait. Rejection feedback is bounded to 256 characters by the reducer.

## Fixture adapter boundary

`preview_projection()` provides Maya, Jules, Nico, Amara, and Leo; Quack's Creek with Harbor Café, Park, Arcade, and Home; and a Harbor Café coffee machine with Make coffee, Clean, and Inspect. Clean is unavailable with a concise reason. All eight needs are included for every character.

The next browser layer must clearly show `UI preview`. It may delay fixture replies, but must echo each request's operation ID and projection revision exactly. Travel and interaction requests produce Accepted/Rejected; cancellation produces CancellationAcknowledged/Rejected. Pass replies only to the shell that created the request. Acknowledgments demonstrate presentation flow only; this crate performs no timers, network requests, money transactions, simulation, or rendering.

## Native verification

From the repository root:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

The core tests exercise real reducer transitions and serialized JSON round trips, including rapid input, unrelated replies, stale identity/revision, Back navigation, rejection, explicit cancellation acknowledgment, invalid projections, and bounded queue capacity.
