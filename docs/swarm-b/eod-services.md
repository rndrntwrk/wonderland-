# Native EOD services, clothing, Carducopia and secure trade

The ten service registrations are translated into the native EOD kernel and run
through `NativeHost`. Their private state, callbacks and immutable provider work
use private checkpoint format 4. `tests/service_handlers.rs` exercises real host
admission, output routing, persistence preparation, departure and restore; the
original C# component oracle is in `fixtures/eod/service`.

This is source translation and native-boundary acceptance evidence. Production
providers, a real VM adapter applying the accepted commands, and the complete
original application/UI remain separately unqualified. The census keeps those
states separate from native implementation and restore support.

## Registered behavior and evidence

All source references below use original pin
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. The manifest records whole-file hashes
and Git blob IDs. Every row is exercised through the actual native host.

| ID and handler | Implemented transitions | Provider/VM boundary | Named acceptance tests in `service_handlers.rs` |
| --- | --- | --- | --- |
| `0x00002000` PropertySelect | Once-only show, source signed register packing, bounded selection and name byte events, close | Native authoritative registers and object events; no provider operation | `property_selection_keeps_source_sign_extension_and_emits_every_name_byte`, `property_output_overflow_rolls_back_sequence_and_selected_value` |
| `0x00001000` FNewspaper | Show, private serialized news and payout data, close | `DynamicPayouts`, bounded typed reply with exact private bytes | `newspaper_private_payload_is_checkpoint_prepared_and_current_epoch_fenced`, `newspaper_malformed_legacy_lengths_leave_the_operation_pending` |
| `0x00001003` Bulletin | Modes 2–4, latest animation selection, native completion callback, once-only posted-state refresh | `BulletinState` for the exact trusted lot | `bulletin_source_latest_animation_and_once_only_posted_refresh_are_fenced` |
| `0x00001004` CooldownEvent | All eight avatar/account, local/global and category modes; alternate community behavior; remaining-time events and close | One atomic `Cooldown` query/reservation using trusted UTC ticks and account identity | `cooldown_all_eight_source_scopes_and_community_exclusions_use_atomic_native_queries`, `cooldown_legacy_local_account_sharing_expiry_and_bounds_are_explicit` |
| `0xAA5E36DC` Trunk | Nine source collection kinds, male/female catalog lookup, canonical selection, fallback skeleton, equip and close | `ReadTrunkCollection`; native `SetOutfit` | `trunk_requires_trusted_collection_then_closes_after_native_equip_and_costume_fallback` |
| `0x895C1CEB` DrawACard | Source 19-card default deck; modes 0–3, owner editor, title/description, add/edit/remove/shuffle/draw, durable save and continuation flag | `LoadPluginData`, revisioned `SavePluginData`; private native RNG | `draw_card_owner_editor_preserves_source_counts_strings_and_compare_and_swap_bytes`, `draw_card_uniform_unique_entries_ignore_frequency_and_restore_does_not_redraw` |
| `0xCB492685` Rack customer | Stock list and name, try-on, duplicate/category-cap validation, purchase result errors 0/1/2, successful equip/refresh, cleanup | Outfit list and atomic purchase/debit/transfer; trusted object and avatar | `rack_purchase_distinguishes_duplicate_category_limit_and_atomic_success`, `dresser_cleanup_event_precedes_disconnect_on_ui_close_and_transport_close` |
| `0x2B58020B` Rack owner | Authorized owner join, list, price change, catalog-priced stock, deletion, bounded rack name and durable retry after departure | Atomic stock/debit, owner-authorized mutation and atomic owner/customer name namespaces | `rack_owner_rejects_unauthorized_join_and_stocks_only_canonical_catalog_price`, `rack_name_denial_retains_authorized_writer_for_checkpointed_retry_after_disconnect` |
| `0x8B300068` Dresser | Owned outfits, dynamic/decoration change, default selection, keep-one category deletion, default fallback and cleanup | Avatar inventory provider and conditional native default command after successful deletion | `dresser_source_big_endian_outfits_scope_mapping_and_delete_default_acknowledgment`, `pending_dresser_default_delete_reserves_avatar_and_fences_detached_vm_commands`, `late_dresser_delete_compares_original_asset_and_preserves_newer_vm_default` |
| `0x897F82F5` SecureTrade | Two participants, five slots each, item/property/funds validation, private offer updates, acceptance reset, 150 accepted ticks, final exchange, departure | Atomic complete-offer `SecureTrade`; immutable accepted offers retained across failure/restore | `secure_trade_source_offers_are_private_delay_acceptance_and_commit_once_after_checkpoint`, `secure_trade_prepared_transaction_survives_both_departures_and_private_restore`, `secure_trade_zero_money_reset_untradable_and_property_slot_zero_are_regressed` |

The pending record validators check the provider operation against its exact
handler stage, trusted object, avatar, inventory owner, expected revision and
retained offer data. Merely naming a callback or constructing the registration
is insufficient. Tests mutate the private kernel object, callback and operation
journal and require rejection before any provider call. Additional private unit
regressions reject invented PropertySelect pending records, invalid cooldown
modes/time/account IDs, mismatched wardrobe stages/writers, and a final trade
operation differing from the frozen accepted offers.

## Source wire and time contracts

The native scoped transport is separate from each original handler's UI body.
Property IDs and trade records use the source little-endian layout. Property
names remain byte events after UTF-8 encoding. Newspaper data retains float bits;
Draw text uses source 7-bit UTF-8 byte lengths. The source rack stock packet uses
big-endian numeric fields and 16-bit enum encodings, including byte-declared
owner/source enums. Those widths follow `VMGLOutfit.Serialize`,
`IoBufferUtils.PutEnum` and `ModelSerializer` and have explicit literal tests.
No general serializer is substituted across those different formats.

Outfit scopes follow `VMSuitScope.cs`: defaults day/sleep/swim are 0/5/2;
dynamic day/sleep/swim/costume are 22/24/23/25; decorations are 8/9/10/11.
`OutfitScope::source_value()` exposes the authoritative adapter mapping.
Dresser change events keep the source dynamic arguments 100/101/102 and
decoration arguments 3/4/5/6. The close event is 2.

The host advances handlers once per accepted 30 Hz VM tick. SecureTrade's delay
is 150 accepted ticks. Cooldown expiration uses authoritative .NET DateTime
100 ns UTC ticks supplied through native input; it never substitutes elapsed
simulation ticks or process wall-clock time. The provider performs the
check-and-reserve atomically, including account and category scope. The helper
`evaluate_local_cooldown` preserves the legacy 16-byte avatar/account/until
record format but is not a production account or persistence service.

Card selection uses one private bounded native SplitMix64 stream with complete
state in the checkpoint. Source DrawACard selects uniformly among distinct list
entries, independent of the displayed per-entry frequency. Native tests compare
frequency changes across twelve seeds and compare uninterrupted versus restored
state. They make no System.Random sequence-equivalence claim.

## Explicit native corrections and policies

These differences are intentional source/boundary policies. The original oracle
retains original behavior, including the recorded bugs, so it cannot silently
turn a correction into an original-runtime equivalence claim.

| Named policy | Original source anchor | Native behavior and regression |
| --- | --- | --- |
| `SERVICE-OWNER-ADMISSION` | `VMEODRackOwnerPlugin.OnConnection`; DrawACard management handlers and invocation registers | Owner authorization comes only from trusted input; management writes and owner joins fail before output/state admission. Owner/editor tests cover denied actors. |
| `SERVICE-ATOMIC-INVENTORY` | `VMEODRackPlugin` purchase callbacks; `VMEODRackOwnerPlugin` stock callbacks | One typed provider operation validates the canonical object/asset/price/owner and atomically debits and transfers/stocks. No fabricated success or independent debit-and-grant calls. Purchase/stock failure cases are covered. |
| `DRESSER-DELETE-CONFIRMED-DEFAULT` | `VMEODDresserPlugin.DeleteOutfit`, original lines 49–64 | The source changes a default before the asynchronous deletion result. Native replacement follows successful deletion only. Failure leaves the default unchanged. |
| `DRESSER-LATE-DEFAULT-COMPARE` | Same `DeleteOutfit` callback, especially `GetValue` and first alternate selection | A clothing deletion retains its original asset and first alternate. `SetOutfitIfCurrent` compares the VM's current default with that original asset, preserving a newer choice. Native `ObserveDefaultOutfits` refreshes the retained VM observation; UI cannot supply it. The delayed-delete regression tests both originally-default and originally-nondefault deletions. |
| `DRESSER-PENDING-AVATAR-RESERVATION` | Native transaction boundary around the source deletion callback | Until the retained clothing default continuation resolves, another native group cannot admit that avatar, even after departure or restore. Cross-group checkpoint validation enforces the same restriction. This is a native concurrency policy. |
| `NATIVE-VM-CONTINUATION-REBIND` | Source object/appearance callbacks; `native_host::Emission::drain` | A detached closing group may finish bookkeeping, but cannot consume a controller event or typed VM command. Rebinding the recorded controller replays the exact prepared provider request and admits the continuation once. |
| `WARDROBE-LOAD-DENIAL-TERMINATES` | `VMAbstractEODRackPlugin` constructor and `Tick` initialization | A typed denial of the initial name load closes the group, including the stage-zero request with no avatar. It cannot leave an unrecoverable uninitialized dialog. |
| `SERVICE-REVISION-PREFLIGHT` | Draw/rack revisioned native persistence boundary | Reject a write whose next revision would be unsupported before dispatch; a successful external write cannot become forever unacknowledgeable. `terminal_revision_is_rejected_before_a_durable_write_is_dispatched` covers the boundary. |
| `DRAW-UNICODE-SCALAR-BOUNDARY` | `VMEODGameCompDrawACardPlugin.GetCurrentDeck`, original substring and strict `BinaryWriter` encoding | Truncation ends before an incomplete Unicode scalar at 40 UTF-16 units. The original oracle records `EncoderFallbackException` for a split surrogate; the native regression keeps the valid prefix. |
| `DRAW-EMPTY-DECK-BOUNDED` | `VMEODGameCompDrawACardData.GetCurrentCard` and draw output | Empty source decks yield no card. Native output uses a bounded empty payload, without inventing a selection or RNG result. |
| `TRADE-ZERO-CLEARS-OFFER` | `VMEODSecureTradePlugin.TradeOffer`, case `m`, original `amount != 0` guard | `m0` clears a previous money offer and resets acceptance/delay. The original trace records zero source updates. |
| `TRADE-PROPERTY-SLOT-ZERO-UNIQUE` | `VMEODSecureTradePlugin.TradeOffer`, property duplicate search/index check | Reject a property already offered in slot zero as well as other slots. The source fixture deliberately records the original two-property bug. |
| `TRADE-UNTRADABLE-BOUNDARY` | Same `TradeOffer` inventory/property validation and administrative-preview branch | Untradable GUIDs and malformed/out-of-range slots fail closed at the native provider boundary. An administrative preview does not authorize an exchange. |
| `TRADE-FROZEN-ATOMIC-COMMIT` | `VMEODSecureTradePlugin.TryCompleteTrade` | Only the exact two accepted offers may form the final atomic provider exchange; its journal survives both departures and replays under the new host epoch. |

The native avatar reservation covers admissions in this host. A production
inventory/VM integration must also serialize outside inventory writers and apply
conditional outfit commands at the matching accepted VM barrier with authoritative
ownership validation. The crate does not claim cross-service atomicity that it
cannot establish. All provider contract and VM integration qualification remains
open until exercised with those actual systems.

## Bounded state and failure recovery

The service kernel caps callback maps, lists, text, card entries, trade object
payloads and operation bytes before retaining them. Draw has at most 300 distinct
entries and preserves source byte-sized saved current index behavior; edit
frequency accepts 1–99 while source add can carry a byte value through 255.
Inventory lists/catalogs are bounded at 4096 records, rack stock at 20 and avatar
clothing at five per category, with canonical ownership and unique IDs checked.
Trade has five item slots per participant and bounded private item/property data.
Provider replies and complete private kernels are additionally subject to host
per-record and aggregate limits.

Every external operation, including a read, first enters the immutable private
journal and must be included in a successful checkpoint. A retryable predecessor
blocks later operations in the same group. The provider must durably deduplicate
the entire request and exact terminal reply; a retry after an external commit or
restart uses the same origin operation ID. Output overflow or invalid callback
application leaves that request pending. Reads need stable reply replay too,
since their values determine the next validated action.

`provider_predecessor_retry_blocks_later_operations_in_the_same_group` and
`provider_private_output_overflow_retains_exact_prepared_request_for_replay`
exercise these paths through real host state. The crate supplies a typed native
boundary and in-memory acceptance adapters, not a database, inventory server,
account store, moderation service or durable provider implementation.
