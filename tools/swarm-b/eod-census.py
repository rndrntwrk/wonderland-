#!/usr/bin/env python3
"""Generate/check the exact source EOD registration census; no guessed UI pairs.

Only the two IDToHandler initializers define registrations. Handler classes that
exist elsewhere (for example UIDebugEOD or the stub fallback) are not registered.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE_COMMIT = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
SERVER = "TSOClient/tso.simantics/NetPlay/EODs/VMEODServer.cs"
UI = "TSOClient/tso.client/UI/Panels/EODs/UIEODController.cs"
TIMER = 0xAA65FE9E
DANCE = 0x4A5BE8AB
SIGNS = 0x2A6356A0
SCOREBOARD = 0x0949E698
DOOR = 0x0A69F29F
PAPER = 0xCA418206
PIZZA = 0xEA47AE39
MAZE = 0x4A245A22
COOPERATIVE = {PAPER, PIZZA, MAZE}
CASINO = {0xCB2819CB, 0x0B2A6B83, 0x2B2FC514, 0x1001}
SOCIAL = {0x2D642D39, 0x8ADFC7A2, 0x1005, 0x1006, 0x6C5C7555, 0x6D113845, 0xCCC5BC43, 0xEC55D705}
SERVICE = {0x1000, 0x1003, 0x1004, 0x2000, 0x2B58020B, 0xCB492685, 0x8B300068, 0xAA5E36DC, 0x895C1CEB, 0x897F82F5}
FORMAT4 = CASINO | SOCIAL | SERVICE
NATIVE = {TIMER, DANCE, SIGNS, SCOREBOARD, DOOR} | COOPERATIVE | FORMAT4
PROVIDERS = {SIGNS, SCOREBOARD, DOOR} | CASINO | (SERVICE - {0x2000})
PERSISTED = {SIGNS, SCOREBOARD, DOOR}
PREREQUISITES = {
    "VMEODSignsPlugin": "GlobalLink plugin text persistence, owner/edit permissions and source UI byte traces",
    "VMEODDanceFloorPlugin": "dance-state object events, avatar selection and multi-client source traces",
    "VMEODPizzaMakerPlugin": "original C# handler/UI traces separating native RNG policy, authoritative VM adapter and private restart rehearsal",
    "VMEODPaperChasePlugin": "original C# handler/UI traces separating native RNG policy, authoritative VM adapter and private restart rehearsal",
    "VMEODRackOwnerPlugin": "rack inventory/catalog provider, ownership and transaction failure/retry traces",
    "VMEODRackPlugin": "rack inventory provider, outfit ownership and purchase/equip transaction traces",
    "VMEODDresserPlugin": "avatar outfit inventory/equip provider and VM/UI clothing traces",
    "VMEODScoreboardPlugin": "EODPersist provider, patch ordering and color/score overflow source traces",
    "VMEODPermissionDoorPlugin": "private door-code persistence, permission/access policy and door-state object-event traces",
    "VMEODSlotsPlugin": "private RNG, payouts/budgets, owner controls and transactional reserve/refund provider",
    "VMEODTrunkPlugin": "outfit catalog/inventory and appearance/equip provider with source traces",
    "VMEODWarGamePlugin": "private game choices, lobby synchronization and round/object-event traces",
    "VMEODTimerPlugin": "authoritative TempRegisters adapter, synchronized event barrier, original C# + UI differential traces and restart rehearsal",
    "VMEODGameCompDrawACardPlugin": "persisted deck/editor data, private shuffle/draw state and owner/UI traces",
    "VMEODBandPlugin": "band lobby, sequence RNG, skill/music integration and timing/object-event traces",
    "VMEODRoulettePlugin": "private spin outcome, multiplayer bets, budget/reserve/settle/refund provider and casino traces",
    "VMEODSecureTradePlugin": "item/funds escrow, ownership validation, atomic trade/refund provider and retry traces",
    "VMEODBlackjackPlugin": "private deck/dealer state, player lobby, casino transactions and disconnect/refund traces",
    "VMEODTwoPersonJobObjectMazePlugin": "original C# handler/UI traces separating two native RNG streams and named phase guards, authoritative VM adapter and private restart rehearsal",
    "VMEODNCDanceFloorPlugin": "nightclub controller coupling, dance-floor object/tile state and server-only traces",
    "VMEODDancePlatformPlugin": "nightclub controller coupling and exact UINCDanceFloorEOD message/event traces",
    "VMEODDJStationPlugin": "nightclub music/controller integration and pattern/UI/object-event traces",
    "VMEODNightclubControllerPlugin": "cross-plugin nightclub controller lifecycle, tuning, VM events and server-only traces",
    "VMEODFNewspaperPlugin": "GlobalLink.GetDynPayouts provider, serialized news/payout limits and UI traces",
    "VMEODHoldEmCasinoPlugin": "private cards/RNG, multiplayer betting, budget/settlement/refund provider and casino traces",
    "VMEODBulletinPlugin": "bulletin service/provider, permissions/moderation, callbacks and serialization traces",
    "VMEODCooldownEventPlugin": "cooldown persistence/transaction provider, authoritative time and callback/retry traces",
    "VMEODGameshowBuzzerPlayerPlugin": "host/player cross-plugin callbacks, queued object events and UI synchronization traces",
    "VMEODGameshowBuzzerHostPlugin": "host/player cross-plugin lifecycle, scores/judgments and timer/object-event traces",
    "VMEODPropertySelectPlugin": "archive property-selection/travel provider, search bounds and VM/UI traces",
}


def source(path):
    raw = (ROOT / path).read_bytes()
    return raw.decode("utf-8-sig"), hashlib.sha256(raw).hexdigest()


def registrations(path):
    text, digest = source(path)
    marker = re.search(r"Dictionary<uint,\s*Type>\s+IDToHandler\s*=", text)
    if not marker:
        raise ValueError(f"Missing IDToHandler dictionary: {path}")
    opening = text.index("{", marker.end())
    # Parse only initializer lines; reject unknown syntax instead of quietly omitting it.
    result = {}
    start_line = text.count("\n", 0, opening) + 1
    closed = False
    for offset, line in enumerate(text[opening + 1:].splitlines()):
        line_number = start_line + offset
        stripped = line.split("//", 1)[0].strip()
        if not stripped:
            continue
        if stripped == "};":
            closed = True
            break
        match = re.fullmatch(r"\{\s*(0x[0-9a-fA-F]+)\s*,\s*typeof\(([A-Za-z0-9_]+)\)\s*\},?", stripped)
        if not match:
            raise ValueError(f"Unparsed registration syntax at {path}:{line_number}: {stripped}")
        plugin_id = int(match[1], 16)
        if plugin_id in result:
            raise ValueError(f"Duplicate registration 0x{plugin_id:08X} at {path}:{line_number}")
        result[plugin_id] = {"type": match[2], "registration": {"path": path, "line": line_number, "sha256": digest}}
    if not closed:
        raise ValueError(f"Unterminated dictionary: {path}")
    return result


def handler_anchor(name, directory):
    candidates = []
    for path in sorted((ROOT / directory).rglob("*.cs")):
        relative = path.relative_to(ROOT).as_posix()
        text, digest = source(relative)
        match = re.search(r"\bclass\s+" + re.escape(name) + r"\b", text)
        if match:
            candidates.append({"path": relative, "line": text.count("\n", 0, match.start()) + 1, "sha256": digest})
    if len(candidates) != 1:
        raise ValueError(f"Expected one definition for {name}, found {len(candidates)}")
    return candidates[0]


# Explicit behavioral cases, not registry-construction counts. The generator
# verifies every named case remains present; the aggregate verifier runs them.
NEW_CASES = {
    0xCB2819CB: ("casino/slots.rs", "slots_late_debit_after_teardown_refunds_instead_of_settling_the_selected_spin", "slots_prepared_debit_crash_replay_requires_rebound_native_authority"),
    0x0B2A6B83: ("casino/table.rs", "roulette_retries_preserve_ordered_debits_and_source_spin_duration", "roulette_retries_preserve_ordered_debits_and_source_spin_duration"),
    0x2B2FC514: ("casino/table.rs", "blackjack_real_host_masks_dealer_card_and_settles_a_complete_round", "blackjack_late_split_debit_after_teardown_keeps_the_refund_checkpoint_valid"),
    0x1001: ("casino/table.rs", "holdem_real_host_keeps_each_hole_private_then_settles_side_and_called_game", "holdem_late_call_debit_after_teardown_keeps_the_refund_checkpoint_valid"),
    0x2D642D39: ("social/war.rs", "war_final_different_pieces_continue_and_emit_one_game_result", "war_private_recovery_pauses_timer_until_controller_and_both_roles_rebind"),
    0x8ADFC7A2: ("social/band.rs", "band_all_twenty_five_sequences_complete_with_source_final_payout", "band_rebind_shows_saved_game_before_phase_controls_without_advancing_it"),
    0x1005: ("social/buzzer_player.rs", "buzzer_first_late_locked_and_timeout_match_source_tick_window", "buzzer_private_recovery_preserves_options_master_and_deferred_reactions"),
    0x1006: ("social/buzzer_host.rs", "buzzer_scores_and_winner_wait_for_their_native_callbacks", "buzzer_private_recovery_preserves_options_master_and_deferred_reactions"),
    0x6C5C7555: ("social/club.rs", "dj_button_routes_source_attribute_swap_and_never_indexes_digit_three", "nightclub_private_recovery_preserves_rng_raster_and_round_outputs"),
    0x6D113845: ("social/floor.rs", "nightclub_floor_emits_actual_graphic_commands_with_source_blink_and_heart", "nightclub_private_recovery_preserves_rng_raster_and_round_outputs"),
    0xCCC5BC43: ("social/club.rs", "nightclub_links_real_floor_dj_and_dance_queue_observations", "nightclub_private_recovery_preserves_rng_raster_and_round_outputs"),
    0xEC55D705: ("social/club.rs", "nightclub_links_real_floor_dj_and_dance_queue_observations", "nightclub_private_recovery_preserves_rng_raster_and_round_outputs"),
    0x1000: ("service/simple.rs", "newspaper_private_payload_is_checkpoint_prepared_and_current_epoch_fenced", "newspaper_private_payload_is_checkpoint_prepared_and_current_epoch_fenced"),
    0x1003: ("service/simple.rs", "bulletin_source_latest_animation_and_once_only_posted_refresh_are_fenced", "all_service_initial_reads_and_property_state_resume_identically_after_private_restore"),
    0x1004: ("service/simple.rs", "cooldown_all_eight_source_scopes_and_community_exclusions_use_atomic_native_queries", "all_service_initial_reads_and_property_state_resume_identically_after_private_restore"),
    0x2000: ("service/simple.rs", "property_selection_keeps_source_sign_extension_and_emits_every_name_byte", "all_service_initial_reads_and_property_state_resume_identically_after_private_restore"),
    0x2B58020B: ("service/wardrobe.rs", "rack_owner_rejects_unauthorized_join_and_stocks_only_canonical_catalog_price", "rack_name_denial_retains_authorized_writer_for_checkpointed_retry_after_disconnect"),
    0xCB492685: ("service/wardrobe.rs", "rack_purchase_distinguishes_duplicate_category_limit_and_atomic_success", "all_service_initial_reads_and_property_state_resume_identically_after_private_restore"),
    0x8B300068: ("service/wardrobe.rs", "dresser_source_big_endian_outfits_scope_mapping_and_delete_default_acknowledgment", "pending_dresser_default_delete_reserves_avatar_and_fences_detached_vm_commands"),
    0xAA5E36DC: ("service/simple.rs", "trunk_requires_trusted_collection_then_closes_after_native_equip_and_costume_fallback", "all_service_initial_reads_and_property_state_resume_identically_after_private_restore"),
    0x895C1CEB: ("service/draw.rs", "draw_card_owner_editor_preserves_source_counts_strings_and_compare_and_swap_bytes", "draw_card_uniform_unique_entries_ignore_frequency_and_restore_does_not_redraw"),
    0x897F82F5: ("service/trade.rs", "secure_trade_source_offers_are_private_delay_acceptance_and_commit_once_after_checkpoint", "secure_trade_prepared_transaction_survives_both_departures_and_private_restore"),
}


def native_evidence(plugin_id):
    if plugin_id not in FORMAT4:
        family = "cooperative" if plugin_id in COOPERATIVE else "existing-five"
        return {
            "family": family,
            "host_tests": ["crates/eod-runtime/tests/cooperative_games.rs"] if family == "cooperative" else ["crates/eod-runtime/tests/source_handlers.rs", "crates/eod-runtime/tests/host_scope.rs", "crates/eod-runtime/tests/adversarial.rs"],
            "recovery_evidence": "existing real-host private format 1/2/3 tests; see named cases in the listed files",
            "source_component_oracle": None if family == "cooperative" else "fixtures/eod/source-oracle",
            "policies_document": "docs/swarm-b/cooperative-eod.md" if family == "cooperative" else "docs/swarm-b/eod-source-oracle.md",
        }
    implementation, behavior, recovery = NEW_CASES[plugin_id]
    family = implementation.split("/")[0]
    test_file = f"crates/eod-runtime/tests/{family}_handlers.rs"
    test_text = (ROOT / test_file).read_text()
    for name in {behavior, recovery}:
        if not re.search(r"\bfn\s+" + re.escape(name) + r"\s*\(", test_text):
            raise ValueError(f"Missing substantive native acceptance case: {test_file}::{name}")
    component = "unchanged original handler components with authored VM/provider boundaries"
    if plugin_id in CASINO:
        component = "unchanged deck and exact BlackjackPlayer/value-table helper slices only; no complete casino handler execution"
    elif plugin_id in {0x2B58020B, 0xCB492685, 0x8B300068}:
        component = "original rack-name serializer executed; handlers/outfit packet and scope helpers source-inspected, not original rack-handler execution"
    return {
        "family": family,
        "implementation": f"crates/eod-runtime/src/plugins/{implementation}",
        "host_tests": [f"{test_file}::{behavior}"],
        "recovery_tests": [f"{test_file}::{recovery}"],
        "shared_boundary_tests": ["crates/eod-runtime/tests/native_recovery.rs"] if family == "social" else ["crates/eod-runtime/tests/service_handlers.rs::native_checkpoint_rejects_mismatched_kernel_object_callback_and_journal_before_dispatch"],
        "source_component_oracle": f"fixtures/eod/{family}",
        "source_component_scope": component,
        "policies_document": f"docs/swarm-b/eod-{family if family != 'service' else 'services'}.md",
    }


def recovery_policy(plugin_id):
    if plugin_id == TIMER:
        return "restore-private-schema-1-at-VM-barrier"
    if plugin_id == DANCE:
        return "restore-private-format-2-detached-controller"
    if plugin_id in PERSISTED:
        return "restore-private-format-2-checkpointed-intents-and-provider-reconciliation"
    if plugin_id in COOPERATIVE:
        return "restore-private-format-3-detached-game-and-seats"
    if plugin_id in FORMAT4:
        return "restore-private-format-4-prepared-operation-replay" if plugin_id in PROVIDERS else "restore-private-format-4-detached-members-and-retained-peer-dependencies"
    return "abort-and-reconcile-reservations-through-provider-before-enabling"


def build():
    server, ui = registrations(SERVER), registrations(UI)
    if set(server) != NATIVE or set(NEW_CASES) != FORMAT4:
        raise ValueError("Registration changes require explicit implementation and evidence mapping")
    entries = []
    for plugin_id in sorted(server.keys() | ui.keys()):
        native = server.get(plugin_id)
        presentation = ui.get(plugin_id)
        if native:
            native["source"] = handler_anchor(native["type"], "TSOClient/tso.simantics/NetPlay/EODs")
        if presentation:
            presentation["source"] = handler_anchor(presentation["type"], "TSOClient/tso.client/UI/Panels/EODs")
        name = native["type"] if native else presentation["type"]
        if name not in PREREQUISITES:
            raise ValueError(f"New registration requires an explicit recovery/evidence policy: {name}")
        leaf = f"EOD-{plugin_id:08X}"
        entries.append({
            "id": plugin_id, "id_hex": f"0x{plugin_id:08X}", "server": native, "ui": presentation,
            "runtime_status": "source-translated-native" if plugin_id in NATIVE else "unsupported-unverified",
            "production_provider_required": plugin_id in PROVIDERS,
            "production_provider_verification": "unverified" if plugin_id in PROVIDERS else "not-required-by-translated-handler",
            "durable_provider_verified": False,
            "native_host_verification": "source-translated-native-host-tested",
            "native_recovery_verification": "private-native-host-recovery-tested",
            "native_evidence": native_evidence(plugin_id),
            "original_runtime_verification": "unverified",
            "ui_runtime_verification": "unverified" if presentation else "not-registered",
            "recovery_policy": recovery_policy(plugin_id),
            "remaining_leaf_ids": [f"{leaf}-SOURCE-TRACE", f"{leaf}-RECOVERY"] + ([f"{leaf}-UI-RUNTIME"] if presentation else []) + ([f"{leaf}-PROVIDER"] if plugin_id in PROVIDERS else []),
            "remaining_leaf_scope": {
                "SOURCE-TRACE": "complete original application behavior, separate from bounded component/helper oracles and native policies",
                "RECOVERY": "production VM adapter and checkpoint-store restart rehearsal, separate from native host restore tests",
                "UI-RUNTIME": "original registered UI execution and delivery integration" if presentation else "not-registered",
                "PROVIDER": "real provider authority, durable full-request/result deduplication and failure recovery" if plugin_id in PROVIDERS else "not-required-by-translated-handler",
            },
            "prerequisites": PREREQUISITES[name],
        })
    return {
        "schema_version": 3, "source_commit": SOURCE_COMMIT,
        "evidence_kind": "static exact registration census; per-entry source translation, real-host behavior/recovery cases and named native policies; separate bounded original component/helper oracles",
        "server_registrations": len(server), "ui_registrations": len(ui),
        "source_translated_native_handlers": len(NATIVE), "original_runtime_verified_handlers": 0,
        "production_provider_verified_handlers": 0, "entries": entries,
    }


def rust(census):
    rows = ["// This Source Code Form is subject to the terms of the Mozilla Public",
            "// License, v. 2.0. If a copy of the MPL was not distributed with this",
            "// file, You can obtain one at http://mozilla.org/MPL/2.0/.",
            "// Source registrations: FreeSO contributors; see root LICENSE.md.",
            "// Generated by tools/swarm-b/eod-census.py; do not hand-edit.",
            "use crate::PluginId;", "", "pub const TIMER_PLUGIN: PluginId = PluginId(0xAA65FE9E);",
            "pub const DANCE_FLOOR_PLUGIN: PluginId = PluginId(0x4A5BE8AB);",
            "pub const SIGNS_PLUGIN: PluginId = PluginId(0x2A6356A0);",
            "pub const SCOREBOARD_PLUGIN: PluginId = PluginId(0x0949E698);",
            "pub const PERMISSION_DOOR_PLUGIN: PluginId = PluginId(0x0A69F29F);",
            "pub const PAPER_CHASE_PLUGIN: PluginId = PluginId(0xCA418206);",
            "pub const PIZZA_MAKER_PLUGIN: PluginId = PluginId(0xEA47AE39);",
            "pub const MAZE_PLUGIN: PluginId = PluginId(0x4A245A22);", "",
            "#[rustfmt::skip]", "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
            "pub enum RuntimeStatus { SourceTranslatedTimer, SourceTranslatedNative, UnsupportedUnverified }", "",
            "#[rustfmt::skip]", "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
            "pub enum RecoveryPolicy { RestorePrivateSchema1, RestorePrivateFormat2, ReconcilePrivateFormat2, RestorePrivateFormat3, RestorePrivateFormat4, ReconcilePrivateFormat4, AbortAndReconcileThroughProvider }", "",
            "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
            "pub struct Registration {", "    pub id: PluginId,", "    pub server_type: &'static str,",
            "    pub ui_type: Option<&'static str>,", "    pub server_anchor: &'static str,",
            "    pub ui_anchor: Option<&'static str>,", "    pub runtime: RuntimeStatus,",
            "    pub recovery: RecoveryPolicy,", "    pub native_host_tested: bool,", "    pub native_recovery_tested: bool,", "    pub production_provider_required: bool,", "    pub production_provider_verified: bool,", "    pub original_runtime_verified: bool,", "}", "",
            "#[rustfmt::skip]", "pub static REGISTRATIONS: &[Registration] = &["]
    for entry in census["entries"]:
        native, ui = entry["server"], entry["ui"]
        if native is None:
            raise ValueError("UI-only registration needs an explicit native registry representation")
        anchor = lambda data: f'{data["source"]["path"]}:{data["source"]["line"]}'
        rows.extend(["    Registration {", f'        id: PluginId({entry["id_hex"]}),',
                     f'        server_type: "{native["type"]}",',
                     f'        ui_type: Some("{ui["type"]}"),' if ui else "        ui_type: None,",
                     f'        server_anchor: "{anchor(native)}",',
                     f'        ui_anchor: Some("{anchor(ui)}"),' if ui else "        ui_anchor: None,",
                     "        runtime: RuntimeStatus::" + ("SourceTranslatedTimer," if entry["id"] == TIMER else "SourceTranslatedNative," if entry["id"] in NATIVE else "UnsupportedUnverified,"),
                     "        recovery: RecoveryPolicy::" + ("RestorePrivateSchema1," if entry["id"] == TIMER else "RestorePrivateFormat2," if entry["id"] == DANCE else "ReconcilePrivateFormat2," if entry["id"] in PERSISTED else "RestorePrivateFormat3," if entry["id"] in COOPERATIVE else "ReconcilePrivateFormat4," if entry["id"] in FORMAT4 & PROVIDERS else "RestorePrivateFormat4," if entry["id"] in FORMAT4 else "AbortAndReconcileThroughProvider,"),
                     "        native_host_tested: true,", "        native_recovery_tested: true,", f'        production_provider_required: {str(entry["id"] in PROVIDERS).lower()},', "        production_provider_verified: false,", "        original_runtime_verified: false,", "    },"])
    rows.extend(["];", "", "pub fn lookup(id: PluginId) -> Option<&'static Registration> {",
                 "    REGISTRATIONS.iter().find(|entry| entry.id == id)", "}", ""])
    return "\n".join(rows)


HOST_DOCUMENTATION = r"""

## Implemented source contracts

| Handler | Trusted invocation data | Allowed inbound UI events | Source outputs and persistence |
| --- | --- | --- | --- |
| Timer `0xAA65FE9E` | Invoker's first four temporary registers | Binary `Timer_State_Change`, `Timer_IsRunning_Change`, `Timer_Set`; text `Timer_Close` | Existing timer control/update events, private Timer UI, no durable effect |
| DanceFloor `0x4A5BE8AB` | Floor object, separate native controller invoker, player's avatar ObjectID | Text `press_button`, `close` | Private `dance_show`; a parsed byte becomes the controller's event code and the authoritative avatar ObjectID becomes temp0; no persistence |
| Signs `0x2A6356A0` | Temp0 mode, temp1 maximum UTF-16 units, authoritative roommate/owner permission, persistent object ID | Binary `set_message`; text `close` | Private `signs_init` and `signs_show`; writing-state event 1; private flags/text persistence |
| Scoreboard `0x0949E698` | Persistent object ID and authoritative invocation | Text `scoreboard_setscore`, `scoreboard_updatescore`, `scoreboard_updatecolor`, `close` | Private `scoreboard_show` and binary `scoreboard_state`; object events 1/2 for scores, 3/4 for colors; private six-byte persistence |
| PermissionDoor `0x0A69F29F` | Temp0 mode, temp1 maximum fee, temp2 permission state, temp3 fee, temp4 flags, explicit edit authorization, persistent object ID | Text `set_code`, `set_state`, `set_fee`, `set_flags`, `try_code`, `close` | Private `door_init`, Edit-only `door_code`; Save/state/fee/flags/validation object events; private canonical decimal code persistence |
| PaperChase `0xCA418206` | Private seed, shared object/controller; temp0 role 1..3 or 0 for first free role; authoritative avatar ObjectID | Text `paperchase_chooseletter`, `close` | Private three-role roster, choices/history/results; packed letter/result/idle object events; no durable handler effect |
| PizzaMaker `0xEA47AE39` | Private seed, shared object/controller; temp0 station 0..3 and seven signed tuning registers; native controller callbacks 6/7/8 | Text `ingredient`, `close` (source no-op) | Role-private hands, public-to-participants contributions/roster/timers; ring/contribute/bake/payout-result/restart object events; no direct durable handler effect |
| TwoPersonJobObjectMaze `0x4A245A22` | Private seed, shared object/controller; temp0 1 for Logic and otherwise Charisma | Binary `TSOMaze_Button_Click` | Logic-only maze/color/exit/solution views, Charisma-only current cell, shared private countdown/result; success/failure object events; no durable handler effect |

The table derives from the linked server handlers and their corresponding source clients. Signs data is `u16le flags`, a .NET seven-bit UTF-8 byte length, then UTF-8 text. Scoreboard data is `u8 lhs_color, u8 rhs_color, i16le lhs_score, i16le rhs_score`. Door temp5/6 code registers are intentionally ignored, matching its source handler. There are no public door-code events.

Source quirks are explicit. Signs defaults flags to 15, computes ordinary read/write modes from roommate or visitor bits, does not infer friendship, preserves existing flags for ordinary writers, and retains the source write-mode override after read denial. Its limits count UTF-16 units: `A😀z` at three units becomes `A😀`. Splitting a surrogate pair causes the original strict BinaryWriter to throw; native writes reject that case atomically. Source-invalid sign payloads and numeric commands remain no-ops; malformed UTF-8 is rejected at the native parser boundary. Harmless trailing bytes accepted by source Signs/Scoreboard readers are accepted for loads/UI data and discarded on serialization; checkpointed write intents require canonical bytes.

Scoreboard enum names are case sensitive, but numeric byte enum values are retained. Unknown numeric teams persist unchanged state. Score changes wrap in i16 before clamping to 0..999; color changes do not echo `scoreboard_state`. Door codes accept up to nine digits; stored values use the source UInt32 parse and zero fallback. Setting a code saves it without changing the source handler's cached code. Consequently an Edit rebind can show its original cached code until a new invocation reloads storage. CodeInput validates once and disconnects; malformed or premature attempts disconnect without a validation event. Edit close emits Save before disconnect. Plain transport/VM disconnect does not invent Save events.

## Host behavior and boundaries

`wonderland-eod-runtime` is a dependency-free, native-only Rust library. It has no simulation root, browser, renderer, database, authentication service or network dependency. `connect` retains the timer adapter. `connect_plugin` supplies typed authoritative inputs for DanceFloor, Signs, Scoreboard and PermissionDoor; `connect_dance_controller` binds the no-avatar dance controller separately. `connect_game_controller` and `join_game` admit the three cooperative handlers through separate trusted controller and authenticated role boundaries. Modes, owner authorization, roommate status, avatar ObjectID and persistent-object IDs must come from a trusted VM invocation. Neither UI payloads nor actor-looking numbers in text can set them. The source's pre-initialization Signs permission race is closed by rejecting writes before initialization.

The adapter calls one deterministic tick per authoritative 30 Hz simulation tick. Only Timer requests current VM registers during ticks. Signs and Door initialize after their bounded provider load completes; Scoreboard sends its show event on connection and state on load completion. Native DanceFloor supports multiple authenticated UI participants sharing one controller per floor object. Persistent handlers deliberately serialize one session/write stream per `(scope, plugin, persistent object)`; competing invocations are rejected rather than racing asynchronous C# patches. This exclusivity is a native integration rule, not original-runtime multiplayer parity evidence.

Incoming unreleased protocol version 1 retains the 57-byte scoped header. It is a native envelope, not legacy C# wire compatibility. It has no sender, recipient or Verified field. The host derives the actor through `ConnectionAuthority` and checks connection incarnation, actor, plugin, host scope, instance, host epoch, session generation, sequence and per-tick rate. Event names and payload kinds are allowlisted per plugin. A still-authenticated connection moving between scopes cannot reuse an old scope's ticket even if local counters match.

Only `PublicVmEvent` carries synchronized object events. Source UI and provider/checkpoint payloads have separate private types and redacted Debug. The public projection exposes tick metadata only. Cooperative handlers keep bounded native SplitMix64 streams and all hidden game state private; Maze has distinct handler and maze-generator streams. This is an explicit native deterministic random policy, with no System.Random sequence-parity claim. The original source's broadcast filtering is not reused to carry private door codes, hands, combinations or role-private maze views.

Cooperative games tick once per shared game on the authoritative 30 Hz clock. PaperChase preserves strict 421/91-tick checking/result thresholds and the duplicate packed-letter events on the third choice. Pizza preserves its constructor's default 120-card pool before invocation tuning, station hand inheritance, contribution/reinsertion order, no-op close and native-only callbacks 6/7/8. Maze preserves queued phase changes, last-join 6/8-second cooldown, the 300-second round, 10-second reaction, recursive-backtracking/BFS order and source color-pool behavior. Named native initialization, phase, rejoin, admission and recovery guards are detailed in [cooperative-eod.md](cooperative-eod.md), together with exact APIs, private state and remaining integration gates.

Limits bound instances and participants (including native controllers), timers, messages, rates, output queues, idle deadlines, checkpoint size, persistence records, individual plugin data and aggregate retained provider bytes. Pending, conflicted and orphaned writes count toward persistence limits. Queue and receive/tick admission are atomic: a rejected operation does not mutate handler state or consume its sequence. Provider work commits one acknowledged operation at a time because acknowledged external writes cannot be rolled back by a later provider error. Controller lifetime belongs to explicit authoritative VM teardown; restored controllers are detached and emit no button events until native rebind.

## Checkpoints and provider reconciliation

Timer-only snapshots retain private format/schema 1, including its existing byte vectors. Generalized snapshots without cooperative or new native families retain private format 2; snapshots with cooperative games use private format 3. A snapshot with any new native family uses private format 4, with an exact format 2/3 legacy inner record. Per-handler schemas remain 1. All formats require a quiescent VM/event barrier with public and private delivery queues drained. The adapter must apply VM events and commit the VM checkpoint at the same barrier; merely removing an event from a queue is not proof the VM applied it. `PrivateCheckpointStore` must authenticate/integrity-protect the private bytes, atomically write them and retain a trusted latest stamp. No production checkpoint store or encryption implementation is supplied.

Format 2 retains source state, recorded participants, native controller identities, bounded persistence bindings and immutable plugin-data write intents. It validates field/count/size bounds, schemas, unique identities, idle deadlines, canonical intent bytes and consistency between live source state and effective persisted/pending state. Signs read redaction and Door Edit's intentional stale code cache are handled as source-specific cases. A mismatched format/schema/stamp or unsupported registration fails closed.

Format 3 additionally records each game's controller, exact participant seats, phase, countdowns, random stream state and complete hidden handler state. It rejects dangling/duplicate roles, mismatched handler identities, impossible cards/results, invalid maze graphs/colors/solutions and impossible queued phases. Every controller and retained UI participant restores detached. Game messages, callbacks and ticks remain paused until the recorded controller and every retained participant rebind. A retained seat lost during partial recovery aborts the whole group; controller teardown closes every seat. Role rebind reconstructs only that role's private view, without rerunning source initialization or random generation.

Writes are not sent to a provider until `checkpoint_to` successfully records their exact intent. Each `PluginWriteId` includes scope, plugin, persistent object, origin epoch, instance and operation sequence. Its actor, expected storage revision and bytes are immutable. After restore, the same original ID and bytes are retried under the new host's separately fenced identity. The provider must durably deduplicate the complete request, authorize actor/object/plugin access and atomically compare-and-set the record revision. A commit followed by a lost response is retried under the same ID, never under a new request key.

`restore_from` requires the exact trusted checkpoint stamp and a strictly greater host epoch. No old transport connection is restored. Persisted participants cannot rebind while pending writes or provider reconciliation remain. `drive_persistence` replays checkpointed intents and compares the provider's actual revision/existence/bytes with the checkpoint's expected binding before enabling UI rebind. Divergence returns `PersistenceDiverged`; it never overwrites the provider's newer state. A definitive provider conflict remains bounded and blocks the session until the trusted adapter chooses `abort_conflicted_writes`, which discards only confirmed-not-applied work and disconnects the affected session. Ambiguous retry work cannot be discarded by that API. `abort_unreconciled` permits a detached divergent participant to end only after pending intents have been resolved; a fresh invocation can then reload.

Rebinding uses a scoped `InstanceAddress`, fresh transport authentication for the recorded actor and a rotated session generation. It does not emit another VM connect event. Source initialization/progress and timer state are preserved. Detached participants keep their bounded idle deadline; a timeout may close the UI participant while its checkpointed provider work remains queued for explicit reconciliation. Controller rebind requires its recorded scoped address and invoker.

## Remaining production gates

All thirty native translations still need complete original C# application/UI traces, production VM-adapter integration and restart rehearsal. The separate [existing-five original-handler component oracle](eod-source-oracle.md) compiles selected unchanged C# host/server/handler components with boundary stubs; its differential evidence is distinct from executing the original application/UI or qualifying a production provider. The cooperative three currently have source-derived native-host acceptance evidence. The three persistent handlers additionally need a real private provider satisfying fencing, access control, atomic CAS, durable idempotency and failure recovery. Tests use an in-memory provider model; this is evidence about the host boundary, not a durable service implementation. The emitted native record/handler status is distinct from original-runtime, UI-runtime and production-provider verification in the JSON census.

The additional 22 handlers execute through `connect_native`, authenticated `join_native`, typed native callbacks and `drive_native_provider`. Their format 4 snapshot stores exact source state, hidden RNG/cards, member bindings, peer references and immutable operations. Every member and controller restores detached. Provider replies require exact scope/origin operation identity and current host epoch; rejected output admission retains the same request for replay. Financial obligations survive departure. Required peer dependencies pause even output-silent ticks, while independent groups can progress. A closing source may deliver cleanup to a locally bound peer to remove a detached dependency. Typed VM commands and controller continuations require controller rebind.

See [casino contracts and corrections](eod-casino.md), [social source and recovery policies](eod-social.md), [service contracts and corrections](eod-services.md), and [native host/provider integration](eod-native-boundary.md). Their source manifests and test cases are mapped per registration in the JSON census. Native source/host/recovery acceptance is implemented; full original-application, registered UI, durable production-provider and VM-adapter restart qualification remains open. The four casino handlers have a bounded unchanged deck/Blackjack helper oracle; that oracle does not execute the complete casino handlers or the original poker provider. `effects::EffectOutbox` remains a separate foundation and is not the native-family operation journal.

## Evidence and validation

The crate's source-unit tables cover source event/register mappings, numeric parsing, UTF-16 text limits, source persistence formats, malformed data, permission modes, transitions, close ordering and private state reconstruction. Actual-host tests cover controller routing, all thirty dispatch paths, malformed/unauthorized messages, checkpoint-before-write ordering, lost-response replay, provider divergence/conflict and private rebind. Cooperative acceptance covers complete three/four/two-role games, exact phase timing, role privacy, source quirks, original epoch rejection, complete private restore, malformed checkpoint state, timeout/revocation and join/callback/tick rollback including random state. Existing timer, scoped wire, replay, recipient, rate, queue, checkpoint and effect-outbox regressions remain present. `fixtures/eod/timer-source-traces.md` continues to distinguish source-derived expectations from original-runtime execution. The separate existing-five, social and service Mono component oracles and bounded casino helper oracle record selected unchanged original C# behavior; they do not execute the complete FreeSO application or UI.
"""


def markdown(census):
    lines = ["# EOD source registration and native coverage", "",
        f'Source baseline: `{SOURCE_COMMIT}`. Generated by `python3 tools/swarm-b/eod-census.py`; verify with `--check`.', "",
        f'The source has **{census["server_registrations"]} server registrations and {census["ui_registrations"]} UI registrations**. The census takes the union of the two actual `IDToHandler` dictionaries. It preserves missing UI entries and unusual server/UI pairings. `VMEODStubPlugin`, `UIDebugEOD`, and unregistered handler files are not added as registrations.', "",
        f'**The native host implements all {census["source_translated_native_handlers"]} registered server handlers.** Per-entry evidence below distinguishes source translation, substantive real-host state transitions and private recovery from original-application, UI-runtime and production-provider qualification. The new four casino, eight social and ten service handlers use native private format 4; the earlier five and three cooperative handlers retain their existing formats and tests. No plugin has full original-runtime conformance evidence. All real production provider and VM-adapter qualification remains open.', "",
        "## Exact registrations", "",
        "The JSON companion `fixtures/eod/registration-census.json` contains numeric IDs, registration-line anchors, definition-line anchors, SHA-256 source digests, separate native/UI status, recovery policies and concrete remaining leaf IDs for every row.", "",
        "| ID | Registered server source | Registered UI source | Native status | Original runtime | Recovery |", "| --- | --- | --- | --- | --- | --- |"]
    for entry in census["entries"]:
        def linked(data):
            if not data:
                return "**No UI registration**"
            a = data["source"]
            return f'[`{data["type"]}`](../../{a["path"]}#L{a["line"]})'
        status = "Source-translated native" if entry["id"] in NATIVE else "Unsupported; unverified"
        policy = "Private format 1, 2 or 3" if entry["id"] == TIMER else "Private format 2/3; detached controller" if entry["id"] == DANCE else "Private format 2/3; provider reconciliation" if entry["id"] in PERSISTED else "Private format 3; detached game and seats" if entry["id"] in COOPERATIVE else "Private format 4; immutable provider replay" if entry["id"] in FORMAT4 & PROVIDERS else "Private format 4; retained bindings and peer dependencies" if entry["id"] in FORMAT4 else "Abort; provider reconciliation gate"
        lines.append(f'| `{entry["id_hex"]}` | {linked(entry["server"])} | {linked(entry["ui"])} | {status} | **Unverified** | {policy} |')
    lines.extend(["", "## Per-registration native behavior and recovery evidence", "", "The cases below execute real host state transitions. Named source corrections are kept in each family's policy document; original-runtime equivalence is not inferred from native acceptance.", "", "| ID | Native behavior case | Private recovery case | Policies |", "| --- | --- | --- | --- |"])
    for entry in census["entries"]:
        evidence = entry["native_evidence"]
        behavior = ", ".join(evidence["host_tests"])
        recovery = ", ".join(evidence.get("recovery_tests", [evidence.get("recovery_evidence", "")]))
        lines.append(f'| `{entry["id_hex"]}` | `{behavior}` | `{recovery}` | [{evidence["family"]}](../../{evidence["policies_document"]}) |')
    lines.extend(["", "## Explicit remaining leaves", "",
        "The remaining leaves refer to full original application/UI execution and real provider/VM-adapter restart qualification. They do not negate completed source translation and private native-host restore acceptance. A bounded C# component/helper oracle is narrower than the `-SOURCE-TRACE` gate. Purely local handlers have no provider gate; handlers issuing reads still require exact durable result replay at their declared provider boundary.", "",
        "| Plugin | Open leaf IDs | Prerequisites |", "| --- | --- | --- |"])
    for entry in census["entries"]:
        leaves = ", ".join(f'`{leaf}`' for leaf in entry["remaining_leaf_ids"])
        lines.append(f'| `{entry["id_hex"]}` | {leaves} | {entry["prerequisites"]} |')
    lines.extend(HOST_DOCUMENTATION.splitlines())
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail on drift without writing any files")
    args = parser.parse_args()
    census = build()
    outputs = {
        ROOT / "fixtures/eod/registration-census.json": json.dumps(census, indent=2) + "\n",
        ROOT / "crates/eod-runtime/src/registry.rs": rust(census),
        ROOT / "docs/swarm-b/eod-coverage.md": markdown(census),
    }
    drift = []
    for path, text in outputs.items():
        if args.check:
            if not path.exists() or path.read_text() != text:
                drift.append(path.relative_to(ROOT).as_posix())
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
    if drift:
        raise SystemExit("EOD census drift: " + ", ".join(drift))
    print(f'EOD census {"verified" if args.check else "generated"}: {census["server_registrations"]} server, {census["ui_registrations"]} UI, {len(census["entries"])} unique IDs; 0 original-runtime verified')


if __name__ == "__main__":
    main()
