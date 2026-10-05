#!/usr/bin/env python3
"""Build source-backed object tickets from the Rust content census (no asset codec)."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

BASELINE = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
SCENARIOS = ("enter", "use", "cancel", "leave", "save", "reconnect")
MAX_JSON_BYTES = 128 * 1024 * 1024
MAX_SOURCE_BYTES = 256 * 1024 * 1024
MAX_FILES = 10000
MAX_RESOURCES = 100000
ANCHORS = {
    "primitive_registry": "TSOClient/tso.simantics/VMContext.cs",
    "object_definition": "TSOClient/tso.files/Formats/IFF/Chunks/OBJD.cs",
    "function_table": "TSOClient/tso.files/Formats/IFF/Chunks/OBJf.cs",
    "lifecycle_selection": "TSOClient/tso.simantics/Entities/VMEntity.cs",
    "behavior": "TSOClient/tso.files/Formats/IFF/Chunks/BHAV.cs",
    "interaction_table": "TSOClient/tso.files/Formats/IFF/Chunks/TTAB.cs",
    "routine_scope": "TSOClient/tso.simantics/Engine/VMStackFrame.cs",
    "routing": "TSOClient/tso.simantics/Engine/VMRoutingFrame.cs",
    "slots": "TSOClient/tso.simantics/Engine/VMSlotParser.cs",
    "instruction_dispatch": "TSOClient/tso.simantics/Engine/VMThread.cs",
}
INTEGRATION_CHECKLIST = [
    {"provider": "content", "requires": "Freeze W00 installation manifest, localization, tuning, variants and explicit ordered PIFF list; resolve through W01.2; bind effective_content_id and critical readiness closure."},
    {"provider": "vm", "requires": "Load actual OBJD/OBJf/TTAB/BHAV resources; supply global/private/semiglobal lookup, frame execution, scheduler and source opcode handlers. Resolve listed resource references; do not substitute per-object gameplay."},
    {"provider": "interaction", "requires": "Use real W06.1 offer/query/intent/queue adapters with authoritative identity, revision and cancellation checks; isolate UI queries from VM RNG/state."},
    {"provider": "routing", "requires": "W04.3 reservation/slot/portal provider must execute real route requests and interruption/failure continuations. Opcode presence does not establish a runtime route or selected slot."},
    {"provider": "animation_renderer", "requires": "W05.2/W01.3/W07-W09 providers must load actual sprites/rigs/animations, preserve xevt timing and expose all declared view modes; record visual evidence separately."},
    {"provider": "snapshot_reconnect", "requires": "W02.4/SL.9 and W13.4/W15.3 providers must checkpoint and restore actual suspended route/animation/effect continuations using matching content hashes and accepted tick tail."},
    {"provider": "original_oracle", "requires": "W00.3/W00.4 must capture controlled original-runtime traces for all six scenarios, concurrency and interrupted/failed use; compare actual native/WASM execution, not descriptor generation."},
]


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n"


def load_json(path, maximum=MAX_JSON_BYTES):
    if path.stat().st_size > maximum:
        raise ValueError("JSON input limit exceeded")
    return json.loads(path.read_bytes())


def digest_file(path, maximum=MAX_SOURCE_BYTES):
    if path.stat().st_size > maximum:
        raise ValueError(f"source input limit exceeded: {path}")
    sha = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


def validate_corpus(corpus):
    if corpus.get("schema_version") != 1 or corpus.get("source_baseline") != BASELINE:
        raise ValueError("unsupported census schema or stale source baseline")
    files = corpus.get("files")
    if not isinstance(files, list) or len(files) > MAX_FILES:
        raise ValueError("census file count limit")
    paths = set()
    for source in files:
        path = source["path"]
        if path in paths:
            raise ValueError(f"duplicate source path: {path}")
        paths.add(path)
        if not re.fullmatch(r"[0-9a-f]{64}", source["source_sha256"]):
            raise ValueError(f"invalid source hash: {path}")
        for field in ("objects", "behaviors", "interactions", "function_tables", "resources"):
            if len(source.get(field, [])) > MAX_RESOURCES:
                raise ValueError(f"census {field} count limit: {path}")
        function_count = sum(len(table.get("entries", [])) for table in source.get("function_tables", []))
        if function_count > MAX_RESOURCES:
            raise ValueError(f"census OBJf entry count limit: {path}")
    return paths


def verify_source_files(corpus, root, expected_paths):
    paths = validate_corpus(corpus)
    if paths != expected_paths:
        raise ValueError(f"source path denominator differs: missing={sorted(expected_paths-paths)}, extra={sorted(paths-expected_paths)}")
    for source in corpus["files"]:
        relative = PurePosixPath(source["path"])
        path = root / relative
        if relative.is_absolute() or ".." in relative.parts or path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
            raise ValueError(f"unsafe source path: {relative}")
        if digest_file(path) != source["source_sha256"] or path.stat().st_size != source["byte_len"]:
            raise ValueError(f"stale source hash or byte count: {relative}")


def git_source_paths(root):
    result = subprocess.run(["git", "ls-tree", "-r", "--name-only", BASELINE], cwd=root, check=True, capture_output=True, text=True)
    paths = {p for p in result.stdout.splitlines() if p.lower().endswith((".iff", ".piff"))}
    # A matching working-tree hash alone would accept a regenerated census from
    # modified assets. Verify that the actual checked-in source baseline matches.
    changed = subprocess.run(["git", "diff", "--name-only", BASELINE, "--", *sorted(paths), *sorted(ANCHORS.values())], cwd=root, check=True, capture_output=True, text=True)
    if changed.stdout.strip():
        raise ValueError("checked-in asset baseline modified: " + changed.stdout.strip())
    return paths


def primitive_registry(root):
    path = root / ANCHORS["primitive_registry"]
    source = path.read_text(encoding="utf-8-sig")
    registrations = {}
    pattern = re.compile(r"AddPrimitive\(new VMPrimitiveRegistration\(new (\w+)\(\)\)\s*\{\s*Opcode\s*=\s*(\d+),\s*Name\s*=\s*\"([^\"]+)\"", re.MULTILINE)
    ts1_start = source.index("            if (ts1)\n", source.index("public static void InitVMConfig"))
    tso_start = source.index("            else\n", ts1_start)
    for match in pattern.finditer(source):
        # TSO is the declared corpus; TS1-only handlers are retained as explicit
        # variant data and do not override the TSO opcode mapping.
        variant = "ts1" if ts1_start <= match.start() < tso_start else "tso" if match.start() >= tso_start else "common"
        registration = {"handler": match[1], "name": match[3], "variant": variant,
                        "source_path": ANCHORS["primitive_registry"],
                        "source_line": source.count("\n", 0, match.start()) + 1,
                        "handler_source_path": f"TSOClient/tso.simantics/Primitives/{match[1]}.cs"}
        registrations.setdefault(int(match[2]), []).append(registration)
    if not registrations or not any(item["handler"] == "VMGotoRoutingSlot" for item in registrations.get(45, [])):
        raise ValueError("primitive registration source shape changed")
    return registrations


def scope_for(identifier):
    return "global" if identifier < 4096 else "private" if identifier < 8192 else "semiglobal"


def lifecycle_references(source, obj):
    """Select the source function table without substituting unused OBJD fields."""
    refs = []
    uses_table = bool((obj or {}).get("raw_fields", {}).get("uses_fn_table"))
    if obj is None or uses_table:
        tables = [table for table in source.get("function_tables", [])
                  if obj is None or table["chunk_id"] == obj["chunk_id"]]
        if obj is not None:
            # A malformed duplicate is absent from decoded tables but still
            # makes the physical source key ambiguous. Select by raw identity
            # cardinality as well as the successful decode and exact ordinal.
            raw = [resource for resource in source.get("resources", [])
                   if resource["kind"] == "OBJf" and resource["id"] == obj["chunk_id"]]
            if (len(raw) != 1 or len(tables) != 1
                    or raw[0].get("resource_ordinal") is None
                    or raw[0].get("resource_ordinal") != tables[0].get("resource_ordinal")):
                return refs, "Required OBJf is missing/unsupported or ambiguous; OBJD lifecycle fields are not substituted"
        for table in tables:
            for index, entry in enumerate(table.get("entries", [])):
                for field in ("condition_function", "action_function"):
                    if entry.get(field):
                        refs.append({"origin": "OBJf", "chunk_id": table["chunk_id"],
                            "resource_ordinal": table.get("resource_ordinal"),
                            "function_index": index, "field": field, "id": entry[field]})
        return refs, ("Decoded OBJf condition/action references are structural evidence; execution and effective patch resolution remain gates"
                      if obj is not None else "Source-only OBJf tables are inventoried without applying them to an object")
    for reference in (obj or {}).get("bhav_refs", []):
        if reference["id"]:
            refs.append({"origin": "OBJD", "field": reference["field"], "id": reference["id"]})
    return refs, "OBJD lifecycle fields selected because UsesFnTable is zero; execution remains unverified"


def behavior_dependencies(source, obj, registry):
    behaviors = {b["chunk_id"]: b for b in source.get("behaviors", [])}
    refs, _ = lifecycle_references(source, obj)
    table_id = (obj or {}).get("raw_fields", {}).get("tree_table_id", 0)
    tables = [table for table in source.get("interactions", []) if obj is None or table["chunk_id"] == table_id]
    for table in tables:
        for index, entry in enumerate(table.get("entries", [])):
            for field in ("action_function", "test_function"):
                if entry.get(field):
                    refs.append({"origin": "TTAB", "chunk_id": table["chunk_id"], "interaction_index": index, "field": field, "id": entry[field]})
    # Closure follows only actual statically encoded private routine calls. No
    # semantic inference from names, indirect calls or operand bytes is made.
    if obj is None:
        refs.extend({"origin": "source_resource_index", "chunk_id": identifier, "field": "BHAV", "id": identifier} for identifier in sorted(behaviors))
    pending = sorted(behaviors) if obj is None else sorted({ref["id"] for ref in refs if scope_for(ref["id"]) == "private"})
    reachable = set()
    while pending:
        identifier = pending.pop(0)
        if identifier in reachable or identifier not in behaviors:
            continue
        reachable.add(identifier)
        for call in sorted(set(behaviors[identifier].get("direct_calls", []))):
            refs.append({"origin": "BHAV", "chunk_id": identifier, "field": "instruction_opcode", "id": call})
            if scope_for(call) == "private" and call not in reachable:
                pending.append(call)
    for ref in refs:
        scope = scope_for(ref["id"])
        ref["scope"] = scope
        local_evidence = ref["id"] in behaviors and (scope == "private" or obj is None)
        ref["resolution"] = "decoded_source_resource_not_applied_to_target" if obj is None and local_evidence else "decoded_source_resource" if local_evidence else "missing_or_unsupported_private_resource" if scope == "private" else f"requires_{scope}_provider"
    primitives = {}
    for identifier in sorted(reachable):
        for histogram in behaviors[identifier].get("opcode_counts", []):
            opcode = histogram["opcode"]
            if opcode >= 256:
                continue
            registrations = registry.get(opcode, [])
            dep = primitives.setdefault(opcode, {"opcode": opcode, "instruction_occurrences": 0, "bhav_ids": [], "provider_status": "unverified", "registrations": registrations,
                "tso_registration_status": "registered" if any(reg["variant"] in ("common", "tso") for reg in registrations) else "not_registered_in_tso_source"})
            dep["instruction_occurrences"] += histogram["count"]
            dep["bhav_ids"].append(identifier)
    return (sorted(refs, key=canonical), [primitives[key] for key in sorted(primitives)], sorted(reachable), tables)


def build_inventory(corpus, registry):
    validate_corpus(corpus)
    grouped = {}
    for source in sorted(corpus["files"], key=lambda f: f["path"]):
        grouped.setdefault(source["source_sha256"], []).append(source)
    rows = []
    object_count = 0
    for sha, aliases in sorted(grouped.items()):
        source = aliases[0]
        objects = source.get("objects", [])
        # Same bytes must yield identical metadata irrespective of alias path.
        for alias in aliases[1:]:
            for field in ("objects", "behaviors", "interactions", "function_tables"):
                if alias.get(field, []) != source.get(field, []):
                    raise ValueError("inconsistent census for identical source hash")
            def objf_identities(record):
                return [(resource["id"], resource.get("resource_ordinal"))
                        for resource in record.get("resources", []) if resource["kind"] == "OBJf"]
            if objf_identities(alias) != objf_identities(source):
                raise ValueError("inconsistent census OBJf identities for identical source hash")
        resource_ids = set()
        for obj in objects or [None]:
            identifier = obj["chunk_id"] if obj else None
            if identifier in resource_ids:
                raise ValueError(f"ambiguous duplicate OBJD ID {identifier}: {source['path']}")
            resource_ids.add(identifier)
            suffix = f"OBJD-{identifier:05d}" if obj else "SOURCE"
            row_id = f"sha256:{sha}:{suffix}"
            leaf_id = f"OBJ-{sha}-{suffix}"
            refs, primitives, reachable, tables = behavior_dependencies(source, obj, registry)
            row = {"row_id": row_id, "leaf_id": leaf_id, "row_kind": "object" if obj else "source_cohort",
                   "source_sha256": sha, "source_paths": sorted(a["path"] for a in aliases),
                   "source_format": source.get("format"), "envelope_status": source.get("envelope_status"),
                   "effective_content_id": None, "effective_identity_gate": "W00 installation manifest and W01.2 ordered patch/tuning/variant resolution absent; source identity is not effective identity",
                   "object_definition": obj, "behavior_references": refs,
                   "global_resource_names": source.get("globals", []),
                   "patch_metadata": source.get("piffs", []),
                   "source_resource_index": [resource for resource in source.get("resources", []) if resource["kind"] in ("GLOB", "TTAB", "BHAV", "OBJf", "SLOT", "DGRP", "FSOM", "FSOR", "FWAV")],
                   "function_table_gate": lifecycle_references(source, obj)[1],
                   "decoded_private_bhav_closure": [identifier for identifier in reachable if scope_for(identifier) == "private"],
                   "decoded_source_bhav_dependency_ids": reachable, "primitive_dependencies": primitives,
                   "interaction_tables": tables,
                   "routing_dependencies": [{"opcode": dep["opcode"], "bhav_ids": dep["bhav_ids"], "provider_status": "unverified", "evidence": "registered VMGotoRelativePosition or VMGotoRoutingSlot handler opcode occurs in decoded private BHAV closure"} for dep in primitives if any(reg["handler"] in ("VMGotoRelativePosition", "VMGotoRoutingSlot") for reg in dep["registrations"])],
                   "eod_registry_id": None, "eod_identity_gate": "VMInvokePlugin operands and dynamic plugin selection are not resolved by opcode histograms",
                   "source_parse_errors": source.get("errors", []), "intentional_deviations": [],
                   "status": "unverified", "evidence": {case: {"status": "unverified", "artifact": None, "original_trace": None} for case in SCENARIOS},
                   "ticket": {"packages": ["W06.2", "W06.4"], "source_anchors": sorted(a["path"] for a in aliases),
                       "accepted_contract_versions": None, "predecessors": ["W00.1", "W00.2", "W00.3", "W00.4", "W01.2", "W02.4", "W03.4", "W04.3", "W05.2", "W06.1", "SL.9"],
                       "file_allowlist": [f"fixtures/objects/cohorts/{sha}.json", f"tests/compat/catalog/{leaf_id}.rs"],
                       "checkable_outcome": "All six scenarios execute actual resolved content through real providers, compare original/native/WASM traces, and preserve concurrent/interrupted continuation semantics",
                       "fixture": f"fixtures/objects/cohorts/{sha}.json", "named_test": f"catalog_parity::{leaf_id}",
                       "test_status": "not_implemented", "expected_result": "enter/use/cancel/leave/save/reconnect traces agree and failing/stale/content-mismatch scenarios are rejected",
                       "commit_or_pr": None, "reviewer": None, "remaining_exceptions": ["Unfrozen installation denominator", "Unresolved effective identity", "Real provider integration and original traces unavailable"]}}
            rows.append(row)
            object_count += obj is not None
    rows.sort(key=lambda row: (row["row_kind"] != "object", row["row_id"]))
    normalized_rows = []
    cohort_records = {}
    for sha, aliases in sorted(grouped.items()):
        source = aliases[0]
        source_rows = [row for row in rows if row["source_sha256"] == sha]
        all_opcodes = sorted({histogram["opcode"] for behavior in source.get("behaviors", [])
                              for histogram in behavior.get("opcode_counts", []) if histogram["opcode"] < 256})
        cohort_records[sha] = {
            "schema_version": 1, "source_baseline": BASELINE, "cohort_id": f"sha256:{sha}",
            "source_sha256": sha, "source_paths": source_rows[0]["source_paths"],
            "source_git_blob": source.get("source_git_blob"),
            "source_format": source.get("format"), "envelope_status": source.get("envelope_status"),
            "source_parse_errors": source.get("errors", []),
            "objects": source.get("objects", []), "behaviors": source.get("behaviors", []),
            "interaction_tables": source.get("interactions", []),
            "function_tables": source.get("function_tables", []),
            "global_resource_names": source.get("globals", []), "patch_metadata": source.get("piffs", []),
            "source_resource_index": source_rows[0]["source_resource_index"],
            "primitive_registrations": [{"opcode": opcode, "registrations": registry.get(opcode, []),
                "provider_status": "unverified", "tso_registration_status": "registered" if any(reg["variant"] in ("common", "tso") for reg in registry.get(opcode, [])) else "not_registered_in_tso_source"} for opcode in all_opcodes],
            "object_dependency_index": [{"leaf_id": row["leaf_id"], "objd_chunk_id": row["object_definition"]["chunk_id"] if row["object_definition"] else None,
                "behavior_references": row["behavior_references"],
                "function_table_gate": row["function_table_gate"],
                "source_bhav_dependency_ids": row["decoded_source_bhav_dependency_ids"],
                "primitive_opcodes": [dependency["opcode"] for dependency in row["primitive_dependencies"]],
                "routing_opcodes": [dependency["opcode"] for dependency in row["routing_dependencies"]]}
                for row in source_rows],
            "leaf_ids": [row["leaf_id"] for row in source_rows],
            "shared_scenario_status_ref": "docs/compat/object-matrix.json#scenario_status_templates/all_unverified",
            "ticket_template_ref": "docs/compat/object-matrix.json#ticket_templates/catalog_parity",
            "function_table_gate": "Per-object OBJf selection and unresolved tables are recorded in object_dependency_index; source references do not establish execution",
            "effective_identity_gate": source_rows[0]["effective_identity_gate"],
            "provider_integration_checklist": INTEGRATION_CHECKLIST,
            "original_asset_payload_included": False,
        }
    for row in rows:
        obj = row["object_definition"]
        normalized_rows.append({"row_id": row["row_id"], "leaf_id": row["leaf_id"], "row_kind": row["row_kind"],
            "cohort_id": f"sha256:{row['source_sha256']}", "effective_content_id": None,
            "eod_registry_id": None, "intentional_deviations": [],
            "objd_chunk_id": obj["chunk_id"] if obj else None, "objd_resource_ordinal": obj.get("resource_ordinal") if obj else None,
            "guid_hex": obj.get("guid_hex", f"{obj['guid']:08x}") if obj else None,
            "label": obj.get("label") if obj else None,
            "entrypoints": lifecycle_references(grouped[row["source_sha256"]][0], obj)[0] if obj else [],
            "tree_table_id": obj.get("raw_fields", {}).get("tree_table_id") if obj else None,
            "slot_id": obj.get("raw_fields", {}).get("slot_id") if obj else None,
            "status": "unverified", "scenario_status_ref": "all_unverified", "ticket_template_ref": "catalog_parity"})
    return {"schema_version": 1, "source_baseline": BASELINE,
            "denominator": {"scope": "all IFF/PIFF source paths checked in at pinned git baseline; not the W00 full installation baseline",
                "checked_in_source_paths": len(corpus["files"]), "distinct_source_hashes": len(grouped),
                "decoded_object_rows": object_count, "source_cohort_rows_without_decoded_object": len(rows) - object_count,
                "total_leaf_tickets": len(rows), "full_installation_baseline_available": False},
            "gameplay_acceptance": {"verified_rows": 0, "completion_percentage": None, "reason": "Inventory generation proves metadata coverage, not gameplay parity"},
            "eod_identity_gate": "VMInvokePlugin operands and dynamic plugin selection are not resolved by opcode histograms; no EOD ID is inferred",
            "primitive_dependency_policy": "Object rows: exact opcode histograms in statically referenced private routines from selected OBJf or OBJD lifecycle fields and selected TTAB, following encoded private calls. Source-only cohorts: all decoded source BHAVs/TTABs/OBJf tables are inventoried without applying them to a target. All instructions include potentially dead branches; this is a dependency upper bound, not execution reachability. Missing/ambiguous OBJf, global/semiglobal, indirect and unresolved private calls remain gates; no operand semantic interpretation.",
            "provider_integration_checklist": INTEGRATION_CHECKLIST,
            "scenario_status_templates": {"all_unverified": {case: {"status": "unverified", "artifact": None, "original_trace": None} for case in SCENARIOS}},
            "ticket_templates": {"catalog_parity": {"packages": ["W06.2", "W06.4"], "accepted_contract_versions": None,
                "predecessors": rows[0]["ticket"]["predecessors"] if rows else [],
                "file_allowlist_rule": ["The cohort descriptor path in the cohorts index", "tests/compat/catalog/<leaf_id>.rs"],
                "source_anchor_rule": "Cohort source_paths plus source_implementation_anchors; the row identifies its exact OBJD",
                "fixture_rule": "The cohort descriptor path in the cohorts index",
                "named_test_rule": "catalog_parity::<leaf_id>", "test_status": "not_implemented",
                "checkable_outcome": rows[0]["ticket"]["checkable_outcome"] if rows else None,
                "expected_result": rows[0]["ticket"]["expected_result"] if rows else None,
                "commit_or_pr": None, "reviewer": None, "remaining_exceptions": rows[0]["ticket"]["remaining_exceptions"] if rows else []}},
            "cohorts": [{"cohort_id": record["cohort_id"], "descriptor": f"fixtures/objects/cohorts/{sha}.json", "source_paths": record["source_paths"]} for sha, record in sorted(cohort_records.items())],
            "rows": normalized_rows, "_cohort_records": cohort_records}


def write_or_check(path, value, check):
    expected = canonical(value)
    if check:
        if not path.exists() or path.read_text() != expected:
            raise ValueError(f"stale generated artifact: {path}")
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_suffix(path.suffix + ".tmp")
        temporary.write_text(expected)
        temporary.replace(path)


def expand_ticket(matrix, row):
    """Materialize one exact leaf ticket from the normalized shared template."""
    cohort = next(item for item in matrix["cohorts"] if item["cohort_id"] == row["cohort_id"])
    template = matrix["ticket_templates"][row["ticket_template_ref"]]
    ticket = {key: value for key, value in template.items() if not key.endswith("_rule")}
    ticket.update(leaf_id=row["leaf_id"], row_id=row["row_id"], cohort_id=row["cohort_id"],
        effective_content_id=None, source_anchors=cohort["source_paths"] + [anchor["path"] for anchor in matrix.get("source_implementation_anchors", [])],
        file_allowlist=[cohort["descriptor"], f"tests/compat/catalog/{row['leaf_id']}.rs"],
        fixture=cohort["descriptor"], named_test=f"catalog_parity::{row['leaf_id']}",
        scenario_evidence=matrix["scenario_status_templates"][row["scenario_status_ref"]])
    return ticket


def descriptor_candidates(matrix, records):
    # Selection uses source filename plus decoded OBJD label. It establishes a
    # concrete content case, not behavior classification or runtime completion.
    patterns = {"chair": re.compile(r"chair", re.I), "bed": re.compile(r"(?:bed|sleep)", re.I),
                "appliance": re.compile(r"(?:microwave|fridge|refrigerator|stove)", re.I)}
    label_patterns = {"chair": re.compile(r"\bchair\b", re.I), "bed": re.compile(r"\b(?:bed|sleep)\b", re.I),
                      "appliance": re.compile(r"\b(?:microwave|fridge|refrigerator|stove)\b", re.I)}
    result = {}
    for family, pattern in patterns.items():
        candidates = [row for row in matrix["rows"] if row["row_kind"] == "object" and
                      pattern.search(" ".join(records[row["cohort_id"][7:]]["source_paths"])) and
                      label_patterns[family].search(row.get("label", ""))]
        candidates.sort(key=lambda row: (not any(table["chunk_id"] == row["tree_table_id"] for table in records[row["cohort_id"][7:]]["interaction_tables"]), records[row["cohort_id"][7:]]["source_paths"][0], row["row_id"]))
        selected = candidates[0] if candidates else None
        detail = None
        if selected:
            record = records[selected["cohort_id"][7:]]
            definition = next(obj for obj in record["objects"] if obj["chunk_id"] == selected["objd_chunk_id"])
            dependency = next(item for item in record["object_dependency_index"] if item["leaf_id"] == selected["leaf_id"])
            detail = dict(selected, source_paths=record["source_paths"], object_definition=definition,
                behavior_dependencies=dependency, global_resource_names=record["global_resource_names"],
                interaction_tables=[table for table in record["interaction_tables"] if table["chunk_id"] == selected["tree_table_id"]],
                primitive_registrations=[item for item in record["primitive_registrations"] if item["opcode"] in dependency["primitive_opcodes"]],
                scenario_status=matrix["scenario_status_templates"]["all_unverified"],
                function_table_gate=dependency["function_table_gate"], effective_identity_gate=record["effective_identity_gate"])
        result[family] = {"schema_version": 1, "source_baseline": BASELINE, "family_requested": family,
            "selection_status": "source_descriptor_selected" if selected else "no_exact_label_and_path_candidate_available",
            "selection_evidence": "Decoded OBJD label and checked-in source path both match family keyword; does not establish gameplay behavior",
            "candidate_count": len(candidates), "selected_leaf_id": selected["leaf_id"] if selected else None,
            "descriptor": detail, "provider_integration_checklist": INTEGRATION_CHECKLIST,
            "original_asset_payload_included": False, "gameplay_parity_status": "unverified"}
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--corpus", type=Path)
    parser.add_argument("--check", action="store_true", help="Reject stale source or generated descriptors")
    parser.add_argument("--leaf", help="Print one exact expanded leaf ticket without writing generated artifacts")
    args = parser.parse_args(argv)
    root = args.root.resolve()
    corpus_path = args.corpus or root / "docs/compat/content-corpus.json"
    corpus = load_json(corpus_path)
    corpus = dict(corpus, files=[source for source in corpus["files"] if source["path"].lower().endswith((".iff", ".piff"))])
    verify_source_files(corpus, root, git_source_paths(root))
    matrix = build_inventory(corpus, primitive_registry(root))
    cohorts = matrix.pop("_cohort_records")
    matrix["content_census_sha256"] = digest_file(corpus_path)
    matrix["source_implementation_anchors"] = [{"role": role, "path": path, "source_sha256": digest_file(root / path)} for role, path in sorted(ANCHORS.items())]
    if args.leaf:
        matching = [row for row in matrix["rows"] if row["leaf_id"] == args.leaf]
        if not matching:
            raise ValueError("unknown leaf ID")
        print(canonical(expand_ticket(matrix, matching[0])), end="")
        return
    write_or_check(root / "docs/compat/object-matrix.json", matrix, args.check)
    for sha, record in cohorts.items():
        write_or_check(root / f"fixtures/objects/cohorts/{sha}.json", record, args.check)
    index = {"schema_version": 1, "source_baseline": BASELINE, "denominator": matrix["denominator"],
             "cohorts": [{"cohort_id": record["cohort_id"], "source_sha256": sha, "descriptor": f"fixtures/objects/cohorts/{sha}.json",
                          "source_paths": record["source_paths"], "leaf_ids": record["leaf_ids"]}
                         for sha, record in sorted(cohorts.items())]}
    write_or_check(root / "fixtures/objects/cohorts/index.json", index, args.check)
    for family, descriptor in descriptor_candidates(matrix, cohorts).items():
        write_or_check(root / f"fixtures/objects/core/{family}/descriptor.json", descriptor, args.check)
    expected = {f"{sha}.json" for sha in cohorts} | {"index.json"}
    extras = {path.name for path in (root / "fixtures/objects/cohorts").glob("*.json")} - expected
    if extras:
        raise ValueError("obsolete generated cohort descriptors: " + ", ".join(sorted(extras)))
    print(canonical({"check": args.check, "denominator": matrix["denominator"], "verified_gameplay_rows": 0}), end="")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        print(f"object-census: {error}", file=sys.stderr)
        sys.exit(1)
