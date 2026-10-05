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
NATIVE = {TIMER, DANCE, SIGNS, SCOREBOARD, DOOR}
PERSISTED = {SIGNS, SCOREBOARD, DOOR}
PREREQUISITES = {
    "VMEODSignsPlugin": "GlobalLink plugin text persistence, owner/edit permissions and source UI byte traces",
    "VMEODDanceFloorPlugin": "dance-state object events, avatar selection and multi-client source traces",
    "VMEODPizzaMakerPlugin": "four-player lobby, ingredient/oven state, timing and object-event traces",
    "VMEODPaperChasePlugin": "three-player lobby, private combination RNG, round timing and object-event traces",
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
    "VMEODTwoPersonJobObjectMazePlugin": "maze RNG, two-role lobby, movement/timing and object-event traces",
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


def build():
    server, ui = registrations(SERVER), registrations(UI)
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
            "production_provider_verification": "unverified" if plugin_id in PERSISTED else "not-required-by-translated-handler" if plugin_id in NATIVE else "unverified",
            "original_runtime_verification": "unverified",
            "ui_runtime_verification": "unverified" if presentation else "not-registered",
            "recovery_policy": "restore-private-schema-1-at-VM-barrier" if plugin_id == TIMER else "restore-private-format-2-detached-controller" if plugin_id == DANCE else "restore-private-format-2-checkpointed-intents-and-provider-reconciliation" if plugin_id in PERSISTED else "abort-and-reconcile-reservations-through-provider-before-enabling",
            "remaining_leaf_ids": [f"{leaf}-SOURCE-TRACE", f"{leaf}-RECOVERY"] + ([] if plugin_id in NATIVE else [f"{leaf}-NATIVE"]) + ([f"{leaf}-PROVIDER"] if plugin_id not in {TIMER, DANCE} else []),
            "prerequisites": PREREQUISITES[name],
        })
    return {
        "schema_version": 2, "source_commit": SOURCE_COMMIT,
        "evidence_kind": "static-source-registration-census; synthetic source-derived native tests only",
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
            "pub const PERMISSION_DOOR_PLUGIN: PluginId = PluginId(0x0A69F29F);", "",
            "#[rustfmt::skip]", "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
            "pub enum RuntimeStatus { SourceTranslatedTimer, SourceTranslatedNative, UnsupportedUnverified }", "",
            "#[rustfmt::skip]", "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
            "pub enum RecoveryPolicy { RestorePrivateSchema1, RestorePrivateFormat2, ReconcilePrivateFormat2, AbortAndReconcileThroughProvider }", "",
            "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
            "pub struct Registration {", "    pub id: PluginId,", "    pub server_type: &'static str,",
            "    pub ui_type: Option<&'static str>,", "    pub server_anchor: &'static str,",
            "    pub ui_anchor: Option<&'static str>,", "    pub runtime: RuntimeStatus,",
            "    pub recovery: RecoveryPolicy,", "    pub original_runtime_verified: bool,", "}", "",
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
                     "        recovery: RecoveryPolicy::" + ("RestorePrivateSchema1," if entry["id"] == TIMER else "RestorePrivateFormat2," if entry["id"] == DANCE else "ReconcilePrivateFormat2," if entry["id"] in PERSISTED else "AbortAndReconcileThroughProvider,"),
                     "        original_runtime_verified: false,", "    },"])
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

The table derives from the linked server handlers and their corresponding source clients. Signs data is `u16le flags`, a .NET seven-bit UTF-8 byte length, then UTF-8 text. Scoreboard data is `u8 lhs_color, u8 rhs_color, i16le lhs_score, i16le rhs_score`. Door temp5/6 code registers are intentionally ignored, matching its source handler. There are no public door-code events.

Source quirks are explicit. Signs defaults flags to 15, computes ordinary read/write modes from roommate or visitor bits, does not infer friendship, preserves existing flags for ordinary writers, and retains the source write-mode override after read denial. Its limits count UTF-16 units: `A😀z` at three units becomes `A😀`. Splitting a surrogate pair causes the original strict BinaryWriter to throw; native writes reject that case atomically. Source-invalid sign payloads and numeric commands remain no-ops; malformed UTF-8 is rejected at the native parser boundary. Harmless trailing bytes accepted by source Signs/Scoreboard readers are accepted for loads/UI data and discarded on serialization; checkpointed write intents require canonical bytes.

Scoreboard enum names are case sensitive, but numeric byte enum values are retained. Unknown numeric teams persist unchanged state. Score changes wrap in i16 before clamping to 0..999; color changes do not echo `scoreboard_state`. Door codes accept up to nine digits; stored values use the source UInt32 parse and zero fallback. Setting a code saves it without changing the source handler's cached code. Consequently an Edit rebind can show its original cached code until a new invocation reloads storage. CodeInput validates once and disconnects; malformed or premature attempts disconnect without a validation event. Edit close emits Save before disconnect. Plain transport/VM disconnect does not invent Save events.

## Host behavior and boundaries

`wonderland-eod-runtime` is a dependency-free, native-only Rust library. It has no simulation root, browser, renderer, database, authentication service or network dependency. `connect` retains the timer adapter. `connect_plugin` supplies typed authoritative inputs for the four other handlers; `connect_dance_controller` binds the no-avatar controller separately. Modes, owner authorization, roommate status, avatar ObjectID and persistent-object IDs must come from a trusted VM invocation. Neither UI payloads nor actor-looking numbers in text can set them. The source's pre-initialization Signs permission race is closed by rejecting writes before initialization.

The adapter calls one deterministic tick per authoritative 30 Hz simulation tick. Only Timer requests current VM registers during ticks. Signs and Door initialize after their bounded provider load completes; Scoreboard sends its show event on connection and state on load completion. Native DanceFloor supports multiple authenticated UI participants sharing one controller per floor object. Persistent handlers deliberately serialize one session/write stream per `(scope, plugin, persistent object)`; competing invocations are rejected rather than racing asynchronous C# patches. This exclusivity is a native integration rule, not original-runtime multiplayer parity evidence.

Incoming unreleased protocol version 1 retains the 57-byte scoped header. It is a native envelope, not legacy C# wire compatibility. It has no sender, recipient or Verified field. The host derives the actor through `ConnectionAuthority` and checks connection incarnation, actor, plugin, host scope, instance, host epoch, session generation, sequence and per-tick rate. Event names and payload kinds are allowlisted per plugin. A still-authenticated connection moving between scopes cannot reuse an old scope's ticket even if local counters match.

Only `PublicVmEvent` carries synchronized object events. Source UI and provider/checkpoint payloads have separate private types and redacted Debug. The public projection exposes tick metadata only. None of the five translated handlers needs private RNG; RNG-dependent registrations remain unsupported. The original source's broadcast filtering is not reused to carry private door codes.

Limits bound instances and participants (including native controllers), timers, messages, rates, output queues, idle deadlines, checkpoint size, persistence records, individual plugin data and aggregate retained provider bytes. Pending, conflicted and orphaned writes count toward persistence limits. Queue and receive/tick admission are atomic: a rejected operation does not mutate handler state or consume its sequence. Provider work commits one acknowledged operation at a time because acknowledged external writes cannot be rolled back by a later provider error. Controller lifetime belongs to explicit authoritative VM teardown; restored controllers are detached and emit no button events until native rebind.

## Checkpoints and provider reconciliation

Timer-only snapshots retain private format/schema 1, including its existing byte vectors. Generalized snapshots use private format 2 and explicit per-handler schema 1. Both require a quiescent VM/event barrier with public and private delivery queues drained. The adapter must apply VM events and commit the VM checkpoint at the same barrier; merely removing an event from a queue is not proof the VM applied it. `PrivateCheckpointStore` must authenticate/integrity-protect the private bytes, atomically write them and retain a trusted latest stamp. No production checkpoint store or encryption implementation is supplied.

Format 2 retains source state, recorded participants, native controller identities, bounded persistence bindings and immutable plugin-data write intents. It validates field/count/size bounds, schemas, unique identities, idle deadlines, canonical intent bytes and consistency between live source state and effective persisted/pending state. Signs read redaction and Door Edit's intentional stale code cache are handled as source-specific cases. A mismatched format/schema/stamp or unsupported registration fails closed.

Writes are not sent to a provider until `checkpoint_to` successfully records their exact intent. Each `PluginWriteId` includes scope, plugin, persistent object, origin epoch, instance and operation sequence. Its actor, expected storage revision and bytes are immutable. After restore, the same original ID and bytes are retried under the new host's separately fenced identity. The provider must durably deduplicate the complete request, authorize actor/object/plugin access and atomically compare-and-set the record revision. A commit followed by a lost response is retried under the same ID, never under a new request key.

`restore_from` requires the exact trusted checkpoint stamp and a strictly greater host epoch. No old transport connection is restored. Persisted participants cannot rebind while pending writes or provider reconciliation remain. `drive_persistence` replays checkpointed intents and compares the provider's actual revision/existence/bytes with the checkpoint's expected binding before enabling UI rebind. Divergence returns `PersistenceDiverged`; it never overwrites the provider's newer state. A definitive provider conflict remains bounded and blocks the session until the trusted adapter chooses `abort_conflicted_writes`, which discards only confirmed-not-applied work and disconnects the affected session. Ambiguous retry work cannot be discarded by that API. `abort_unreconciled` permits a detached divergent participant to end only after pending intents have been resolved; a fresh invocation can then reload.

Rebinding uses a scoped `InstanceAddress`, fresh transport authentication for the recorded actor and a rotated session generation. It does not emit another VM connect event. Source initialization/progress and timer state are preserved. Detached participants keep their bounded idle deadline; a timeout may close the UI participant while its checkpointed provider work remains queued for explicit reconciliation. Controller rebind requires its recorded scoped address and invoker.

## Remaining production gates

All five native translations still need original C# application/UI traces, VM-adapter integration and restart rehearsal. The three persistent handlers additionally need a real private provider satisfying fencing, access control, atomic CAS, durable idempotency and failure recovery. Tests use an in-memory provider model; this is evidence about the host boundary, not a durable service implementation. The emitted native record/handler status is distinct from original-runtime, UI-runtime and production-provider verification in the JSON census.

The other 25 registrations retain the explicit policy **abort and reconcile any reservations through an authoritative provider before enabling recovery**. The host rejects their creation and checkpoint state. It does not simulate casino results, inventory, funds, escrow, settlement or refunds. `effects::EffectOutbox` remains a separate general-purpose foundation for those future plugins; its production durable integration remains open. The new plugin-data journal does not turn those unimplemented effect types into supported behavior.

## Evidence and validation

The crate's source-unit tables cover source event/register mappings, numeric parsing, UTF-16 text limits, source persistence formats, malformed data, permission modes, transitions, close ordering and private state reconstruction. Actual-host tests cover controller routing, all four dispatch paths, malformed/unauthorized messages, checkpoint-before-write ordering, lost-response replay, provider divergence/conflict and private rebind. Existing timer, scoped wire, replay, recipient, rate, queue, checkpoint and effect-outbox regressions remain present. `fixtures/eod/timer-source-traces.md` continues to distinguish source-derived expectations from original-runtime execution. A local Mono probe verifies selected .NET parsing/encoding boundary behavior; it does not execute FreeSO.
"""


def markdown(census):
    lines = ["# EOD source registration and native coverage", "",
        f'Source baseline: `{SOURCE_COMMIT}`. Generated by `python3 tools/swarm-b/eod-census.py`; verify with `--check`.', "",
        f'The source has **{census["server_registrations"]} server registrations and {census["ui_registrations"]} UI registrations**. The census takes the union of the two actual `IDToHandler` dictionaries. It preserves missing UI entries and unusual server/UI pairings. `VMEODStubPlugin`, `UIDebugEOD`, and unregistered handler files are not added as registrations.', "",
        "**No plugin has original-runtime conformance evidence.** The native host now dispatches five source-translated server handlers: Timer, DanceFloor, Signs, Scoreboard and PermissionDoor. Their tests use synthetic inputs and source-derived expectations; the original FreeSO application and its UI runtime were not executed. The other 25 registrations remain rejected. The three plugin-data persistence integrations require a production provider, which is not implemented or verified here. W06.3, W06.4 and W16 remain incomplete.", "",
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
        policy = "Private format 1 or 2" if entry["id"] == TIMER else "Private format 2; detached controller" if entry["id"] == DANCE else "Private format 2; provider reconciliation" if entry["id"] in PERSISTED else "Abort; provider reconciliation gate"
        lines.append(f'| `{entry["id_hex"]}` | {linked(entry["server"])} | {linked(entry["ui"])} | {status} | **Unverified** | {policy} |')
    lines.extend(["", "## Explicit remaining leaves", "",
        "Each row below is a concrete open work item set; listing a prerequisite is not evidence that it exists. The `-PROVIDER` gate includes confirming whether that plugin has durable effects, and implementing/validating any provider it needs. Purely local plugins can satisfy that gate by source-backed evidence of no durable effects.", "",
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
