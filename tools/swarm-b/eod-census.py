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
            "runtime_status": "source-translated-timer-only" if plugin_id == TIMER else "unsupported-unverified",
            "original_runtime_verification": "unverified",
            "ui_runtime_verification": "unverified" if presentation else "not-registered",
            "recovery_policy": "restore-private-schema-1-at-VM-barrier" if plugin_id == TIMER else "abort-and-reconcile-reservations-through-provider-before-enabling",
            "remaining_leaf_ids": [f"{leaf}-SOURCE-TRACE", f"{leaf}-RECOVERY"] + ([] if plugin_id == TIMER else [f"{leaf}-NATIVE", f"{leaf}-PROVIDER"]),
            "prerequisites": PREREQUISITES[name],
        })
    return {
        "schema_version": 1, "source_commit": SOURCE_COMMIT,
        "evidence_kind": "static-source-registration-census; synthetic source-derived native tests only",
        "server_registrations": len(server), "ui_registrations": len(ui), "entries": entries,
    }


def rust(census):
    rows = ["// This Source Code Form is subject to the terms of the Mozilla Public",
            "// License, v. 2.0. If a copy of the MPL was not distributed with this",
            "// file, You can obtain one at http://mozilla.org/MPL/2.0/.",
            "// Source registrations: FreeSO contributors; see root LICENSE.md.",
            "// Generated by tools/swarm-b/eod-census.py; do not hand-edit.",
            "use crate::PluginId;", "", "pub const TIMER_PLUGIN: PluginId = PluginId(0xAA65FE9E);", "",
            "#[rustfmt::skip]", "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
            "pub enum RuntimeStatus { SourceTranslatedTimer, UnsupportedUnverified }", "",
            "#[rustfmt::skip]", "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
            "pub enum RecoveryPolicy { RestorePrivateSchema1, AbortAndReconcileThroughProvider }", "",
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
                     "        runtime: RuntimeStatus::" + ("SourceTranslatedTimer," if entry["id"] == TIMER else "UnsupportedUnverified,"),
                     "        recovery: RecoveryPolicy::" + ("RestorePrivateSchema1," if entry["id"] == TIMER else "AbortAndReconcileThroughProvider,"),
                     "        original_runtime_verified: false,", "    },"])
    rows.extend(["];", "", "pub fn lookup(id: PluginId) -> Option<&'static Registration> {",
                 "    REGISTRATIONS.iter().find(|entry| entry.id == id)", "}", ""])
    return "\n".join(rows)


def markdown(census):
    lines = ["# EOD source registration and native coverage", "",
        f'Source baseline: `{SOURCE_COMMIT}`. Generated by `python3 tools/swarm-b/eod-census.py`; verify with `--check`.', "",
        f'The source has **{census["server_registrations"]} server registrations and {census["ui_registrations"]} UI registrations**. The census takes the union of the two actual `IDToHandler` dictionaries. It preserves missing UI entries and unusual server/UI pairings. `VMEODStubPlugin`, `UIDebugEOD`, and unregistered handler files are not added as registrations.', "",
        "**No plugin has original-runtime conformance evidence.** Only the timer server behavior is translated into the native host. Its unit/integration tests use synthetic inputs with expectations derived from C#; neither original FreeSO runtime traces nor the UI runtime were executed. All 29 other registrations are explicitly rejected by the native host. W06.3, W06.4 and W16 remain incomplete.", "",
        "## Exact registrations", "",
        "The JSON companion `fixtures/eod/registration-census.json` contains numeric IDs, registration-line anchors, definition-line anchors, SHA-256 source digests, separate native/UI status, recovery policies and concrete remaining leaf IDs for every row.", "",
        "| ID | Registered server source | Registered UI source | Native status | Original runtime | Recovery |", "| --- | --- | --- | --- | --- | --- |"]
    for entry in census["entries"]:
        def linked(data):
            if not data:
                return "**No UI registration**"
            a = data["source"]
            return f'[`{data["type"]}`](../../{a["path"]}#L{a["line"]})'
        status = "Source-translated timer" if entry["id"] == TIMER else "Unsupported; unverified"
        policy = "Restore private schema 1" if entry["id"] == TIMER else "Abort; provider reconciliation gate"
        lines.append(f'| `{entry["id_hex"]}` | {linked(entry["server"])} | {linked(entry["ui"])} | {status} | **Unverified** | {policy} |')
    lines.extend(["", "## Explicit remaining leaves", "",
        "Each row below is a concrete open work item set; listing a prerequisite is not evidence that it exists. The `-PROVIDER` gate includes confirming whether that plugin has durable effects, and implementing/validating any provider it needs. Purely local plugins can satisfy that gate by source-backed evidence of no durable effects.", "",
        "| Plugin | Open leaf IDs | Prerequisites |", "| --- | --- | --- |"])
    for entry in census["entries"]:
        leaves = ", ".join(f'`{leaf}`' for leaf in entry["remaining_leaf_ids"])
        lines.append(f'| `{entry["id_hex"]}` | {leaves} | {entry["prerequisites"]} |')
    lines.extend(["", "## Host behavior and boundaries", "",
        "`wonderland-eod-runtime` is a dependency-free, native-only standalone Rust library. It has no simulation root, browser, renderer, database, or network dependency. Its adapter must call one deterministic tick per authoritative 30 Hz simulation tick and supply the invoker's first four temporary registers. This is a host boundary and timer translation, not a VM implementation.", "",
        "Incoming unreleased protocol version 1 is a new bounded native envelope with a 57-byte scoped header, not a claim of legacy network wire compatibility. The earlier unscoped draft is unsupported. It has no sender, recipient, or Verified field. The trusted transport implements `ConnectionAuthority`; the host derives the actor from that provider and checks connection, actor, plugin, host scope, instance, host epoch, session generation, sequence and per-tick rate. A still-authenticated connection moving between host scopes cannot reuse a prior scope's ticket, even if their local epochs/counters match. Event names and payload kinds are allowlisted per implemented plugin. Actor IDs in client data never confer authority. Connection IDs must identify a non-reusable authenticated transport incarnation.", "",
        "Only `PublicVmEvent` can carry synchronized object events. Private UI payloads have a separate type and recipient-checked extraction API, with redacted Debug. The public projection exposes only tick metadata. No RNG is implemented or required by the timer; source plugins that use private RNG remain unsupported. Future private RNG implementations must be confined to private plugin/checkpoint state.", "",
        "Host instance, participant, timer, message, rate, queue byte/count, idle lifetime and checkpoint limits are finite. Queue admission and tick/message mutations are atomic within the in-memory host. Timeout order is deterministic by instance ID; authentication revocation disconnects on the next authoritative tick. Source-invalid timer payloads retain the source no-op behavior. Protocol, authentication and host-safety rejects are explicit errors.", "",
        "## Checkpoint and recovery policy", "",
        "Timer checkpoints are private format/schema 1 and may be written only at a quiescent VM/event barrier with public and private output queues drained. They retain private timer progress, participant bindings, counters and deadlines. The provider must store them privately, authenticate/integrity-protect bytes, perform atomic writes, and retain the latest trusted checkpoint stamp. The library does not supply encryption or a database. The authoritative VM checkpoint must use the same barrier; draining an event is not proof that a VM applied it.", "",
        "Restoration verifies the exact expected scope/epoch/revision stamp, format and plugin schema, size/count bounds, fields and uniqueness. It requires a strictly greater host epoch. Old connections are discarded. Rebinding requires a scoped `InstanceAddress` and fresh transport authentication for the recorded actor, rejects scope mismatch, and rotates the session generation; reinitializing the UI does not reset the preserved timer. All private deliveries retain the scoped ticket. Detached restored timers wait for rebind while the bounded idle deadline continues advancing. Generic Debug and public projections do not expose checkpoint bytes or private state.", "",
        "Every unsupported registration has the explicit policy **abort and reconcile any reservations through an authoritative provider before enabling recovery**. This is a fail-closed integration gate: the host rejects its runtime creation and checkpoint restore, and does not pretend to execute its source abort/refund behavior. A refund can only be requested against a provider-confirmed reservation; the crate never invents balances or performs a transfer. There is no database provider in this work package.", "",
        "## Durable-effect handoff", "",
        "`effects::EffectOutbox` is a separate native provider handoff foundation. It retains immutable request identity, rejects conflicting reuse, retries a pending request under the same key, and remembers terminal receipts. The provider is responsible for authorization and atomic durable deduplication by that key, including the case where it commits an effect and loses its response. Tests use a fake provider that models that failure. These tests do not establish a production money, inventory, storage or refund implementation; the timer emits no durable requests. Outbox persistence/restart integration for future effectful plugins is an open gate and must be implemented before enabling them.", "",
        "## Evidence and validation", "",
        "The Task 5 implementation report records the red-to-green timer and independent cross-scope replay regressions and final commands. Native tests cover spoofed sessions/recipient access, cross-scope commands/private deliveries/disconnect/rebind, scoped binary and UTF-8 wire roundtrips, stale connections and epochs, protocol kinds/versions/sizes, event allowlists, bounded queues/rates/timers, deterministic timeout, checkpoint bounds/schema/stamp/restore/rebind, private redaction and effect request idempotency under retry. `fixtures/eod/timer-source-traces.md` distinguishes source-derived expectations from original-runtime evidence.", ""])
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
