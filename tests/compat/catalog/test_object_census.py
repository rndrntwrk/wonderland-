"""Metadata inventory tests; these do not execute catalog gameplay."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("object_census", ROOT / "tools/swarm-b/object-census.py")
census = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(census)


def corpus():
    digest = hashlib.sha256(b"metadata-only example").hexdigest()
    return {"schema_version": 1, "source_baseline": census.BASELINE,
            "files": [{"path": "example.iff", "source_sha256": digest, "byte_len": 21,
                "format": "iff", "envelope_status": "accepted", "errors": [],
                "objects": [{"chunk_id": 4096, "guid": 1234, "label": "example",
                    "version": 142, "raw_fields": {"tree_table_id": 4096},
                    "bhav_refs": [{"field": "BHAV_MainID", "id": 4097}]}],
                "behaviors": [{"chunk_id": 4097, "label": "main", "version": 32771,
                    "opcode_counts": [{"opcode": 45, "count": 2}], "direct_calls": [8192]}],
                "interactions": [{"chunk_id": 4096, "entries": [
                    {"action_function": 4097, "test_function": 8193}]}]}]}


class InventoryTests(unittest.TestCase):
    def test_objf_lifecycle_replaces_objd_fields_and_selects_matching_table(self):
        source = corpus()["files"][0]
        obj = source["objects"][0]
        obj["raw_fields"]["uses_fn_table"] = 1
        source["function_tables"] = [
            {"chunk_id": 4096, "resource_ordinal": 1, "entries": [
                {"condition_function": 8194, "action_function": 4100},
                {"condition_function": 0, "action_function": 0}]},
            {"chunk_id": 4097, "resource_ordinal": 2, "entries": [
                {"condition_function": 8195, "action_function": 4101}]},
        ]
        source["resources"] = [{"kind": "OBJf", "id": 4096, "resource_ordinal": 1},
            {"kind": "OBJf", "id": 4097, "resource_ordinal": 2}]
        source["behaviors"].append({"chunk_id": 4100,
            "opcode_counts": [{"opcode": 27, "count": 1}], "direct_calls": []})
        refs, primitives, reachable, _ = census.behavior_dependencies(source, obj, {})
        lifecycle = [ref for ref in refs if ref["origin"] in ("OBJD", "OBJf")]
        self.assertEqual({(ref["id"], ref["field"]) for ref in lifecycle},
            {(8194, "condition_function"), (4100, "action_function")})
        self.assertTrue(all(ref["origin"] == "OBJf" and ref["chunk_id"] == 4096 for ref in lifecycle))
        self.assertEqual(reachable, [4097, 4100])
        self.assertEqual([entry["opcode"] for entry in primitives], [27, 45])

    def test_missing_or_ambiguous_objf_never_falls_back_to_objd_entrypoints(self):
        source = corpus()["files"][0]
        obj = source["objects"][0]
        obj["raw_fields"]["uses_fn_table"] = 1
        for tables in ([], [
            {"chunk_id": 4096, "entries": [{"condition_function": 0, "action_function": 4100}]},
            {"chunk_id": 4096, "entries": [{"condition_function": 0, "action_function": 4101}]},
        ]):
            source["function_tables"] = tables
            refs, _, _, _ = census.behavior_dependencies(source, obj, {})
            self.assertFalse(any(ref["origin"] in ("OBJD", "OBJf") for ref in refs))
        obj["raw_fields"]["uses_fn_table"] = 0
        refs, _, _, _ = census.behavior_dependencies(source, obj, {})
        self.assertEqual([(ref["field"], ref["id"]) for ref in refs if ref["origin"] == "OBJD"],
            [("BHAV_MainID", 4097)])

    def test_objf_inventory_bounds_and_alias_consistency(self):
        data = corpus()
        data["files"][0]["function_tables"] = [
            {"chunk_id": 4096, "entries": [{"condition_function": 0, "action_function": 4097}]}]
        alias = copy.deepcopy(data["files"][0])
        alias["path"] = "alias.iff"
        alias["function_tables"][0]["entries"][0]["action_function"] = 4100
        data["files"].append(alias)
        with self.assertRaisesRegex(ValueError, "inconsistent census"):
            census.build_inventory(data, {})
        data["files"].pop()
        with patch.object(census, "MAX_RESOURCES", 2):
            data["files"][0]["function_tables"] *= 3
            with self.assertRaisesRegex(ValueError, "function_tables count"):
                census.build_inventory(data, {})
            data["files"][0]["function_tables"] = data["files"][0]["function_tables"][:1]
            data["files"][0]["function_tables"][0]["entries"] *= 3
            with self.assertRaisesRegex(ValueError, "OBJf entry count"):
                census.build_inventory(data, {})

    def test_malformed_duplicate_objf_stays_unresolved_in_either_source_order(self):
        source = corpus()["files"][0]
        obj = source["objects"][0]
        obj["raw_fields"]["uses_fn_table"] = 1
        source["resources"] = [{"kind": "OBJf", "id": 4096, "resource_ordinal": 1},
            {"kind": "OBJf", "id": 4096, "resource_ordinal": 2}]
        source["duplicate_keys"] = [{"kind": "OBJf", "id": 4096, "count": 2}]
        source["behaviors"].append({"chunk_id": 4100,
            "opcode_counts": [{"opcode": 27, "count": 1}], "direct_calls": []})
        for valid_ordinal, bad_ordinal in ((1, 2), (2, 1)):
            source["function_tables"] = [{"chunk_id": 4096, "resource_ordinal": valid_ordinal,
                "entries": [{"condition_function": 0, "action_function": 4100}]}]
            source["errors"] = [{"kind": "OBJf", "chunk_id": 4096, "resource_ordinal": bad_ordinal}]
            refs, primitives, _, _ = census.behavior_dependencies(source, obj, {})
            self.assertFalse(any(ref["origin"] in ("OBJf", "OBJD") for ref in refs))
            self.assertNotIn(27, [item["opcode"] for item in primitives])

    def test_source_only_objf_references_keep_each_duplicate_resource_ordinal(self):
        source = corpus()["files"][0]
        source["function_tables"] = [{"chunk_id": 4096, "resource_ordinal": ordinal,
            "entries": [{"condition_function": 0, "action_function": 4100}]}
            for ordinal in (1, 2)]
        refs, _, _, _ = census.behavior_dependencies(source, None, {})
        origins = [ref.get("resource_ordinal") for ref in refs if ref["origin"] == "OBJf"]
        self.assertEqual(origins, [1, 2])

    def test_aliases_cannot_disagree_on_raw_objf_ambiguity(self):
        data = corpus()
        source = data["files"][0]
        source["objects"][0]["raw_fields"]["uses_fn_table"] = 1
        source["resources"] = [{"kind": "OBJf", "id": 4096, "resource_ordinal": 1}]
        source["function_tables"] = [{"chunk_id": 4096, "resource_ordinal": 1,
            "entries": [{"condition_function": 0, "action_function": 4100}]}]
        alias = copy.deepcopy(source)
        alias["path"] = "alias.iff"
        alias["resources"].append({"kind": "OBJf", "id": 4096, "resource_ordinal": 2})
        data["files"].append(alias)
        with self.assertRaisesRegex(ValueError, "inconsistent census"):
            census.build_inventory(data, {})

    def test_deterministic_complete_inventory_and_aliases(self):
        data = corpus()
        duplicate = copy.deepcopy(data["files"][0])
        duplicate["path"] = "duplicate.iff"
        empty = copy.deepcopy(duplicate)
        empty.update(path="patch.piff", source_sha256="f" * 64, objects=[])
        data["files"] += [duplicate, empty]
        first = census.build_inventory(data, {})
        reverse = copy.deepcopy(data)
        reverse["files"].reverse()
        self.assertEqual(census.canonical(first), census.canonical(census.build_inventory(reverse, {})))
        self.assertEqual(len(first["rows"]), 2)
        self.assertEqual(first["denominator"]["checked_in_source_paths"], 3)
        self.assertEqual(first["cohorts"][0]["source_paths"], ["duplicate.iff", "example.iff"])
        self.assertEqual({row["row_kind"] for row in first["rows"]}, {"object", "source_cohort"})

    def test_exact_references_and_gameplay_not_verified(self):
        matrix = census.build_inventory(corpus(), {})
        row = matrix["rows"][0]
        record = matrix["_cohort_records"][row["cohort_id"][7:]]
        dependency = record["object_dependency_index"][0]
        self.assertIsNone(row["effective_content_id"])
        self.assertEqual(dependency["primitive_opcodes"], [45])
        self.assertEqual(record["behaviors"][0]["opcode_counts"][0]["count"], 2)
        self.assertEqual(dependency["behavior_references"][0]["scope"], "private")
        self.assertTrue(any(ref["id"] == 8193 and ref["resolution"] == "requires_semiglobal_provider"
                            for ref in dependency["behavior_references"]))
        evidence = matrix["scenario_status_templates"][row["scenario_status_ref"]]
        self.assertEqual(set(evidence), set(census.SCENARIOS))
        self.assertTrue(all(v["status"] == "unverified" for v in evidence.values()))

    def test_stale_hash_and_path_escape_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "example.iff").write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "source hash"):
                census.verify_source_files(corpus(), root, {"example.iff"})
            data = corpus()
            data["files"][0]["path"] = "../example.iff"
            with self.assertRaisesRegex(ValueError, "unsafe source path"):
                census.verify_source_files(data, root, {"../example.iff"})

    def test_baseline_count_duplicate_and_limits_rejected(self):
        data = corpus()
        data["source_baseline"] = "stale"
        with self.assertRaisesRegex(ValueError, "baseline"):
            census.build_inventory(data, {})
        data = corpus()
        data["files"].append(copy.deepcopy(data["files"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate source path"):
            census.build_inventory(data, {})
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with self.assertRaisesRegex(ValueError, "source path denominator"):
                census.verify_source_files(corpus(), root, {"missing.iff"})
            large = root / "large.json"
            large.write_text("x" * 12)
            with self.assertRaisesRegex(ValueError, "JSON input limit"):
                census.load_json(large, 10)

    def test_generated_files_staleness_detected(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "descriptor.json"
            census.write_or_check(path, {"status": "unverified"}, False)
            census.write_or_check(path, {"status": "unverified"}, True)
            path.write_text(json.dumps({"status": "verified"}))
            with self.assertRaisesRegex(ValueError, "stale generated artifact"):
                census.write_or_check(path, {"status": "unverified"}, True)

    def test_checked_in_corpus_complete_leaf_rows(self):
        path = ROOT / "docs/compat/content-corpus.json"
        if not path.exists():
            self.skipTest("Rust corpus has not been generated")
        data = census.load_json(path)
        data["files"] = [source for source in data["files"] if source["path"].lower().endswith((".iff", ".piff"))]
        census.verify_source_files(data, ROOT, census.git_source_paths(ROOT))
        matrix = census.load_json(ROOT / "docs/compat/object-matrix.json")
        by_hash = {source["source_sha256"]: source for source in data["files"]}
        expected_rows = sum(len(source["objects"]) or 1 for source in by_hash.values())
        self.assertEqual(len(matrix["rows"]), expected_rows)
        self.assertEqual(len({row["leaf_id"] for row in matrix["rows"]}), expected_rows)
        self.assertEqual({path for cohort in matrix["cohorts"] for path in cohort["source_paths"]}, {source["path"] for source in data["files"]})
        self.assertTrue(all(row["cohort_id"] in {cohort["cohort_id"] for cohort in matrix["cohorts"]} for row in matrix["rows"]))
        self.assertEqual(matrix["content_census_sha256"], census.digest_file(path))
        self.assertTrue(all(row["status"] == "unverified" and row["effective_content_id"] is None for row in matrix["rows"]))

    def test_ticket_expansion_has_exact_allowlist_and_six_scenarios(self):
        matrix = census.build_inventory(corpus(), {})
        row = matrix["rows"][0]
        ticket = census.expand_ticket(matrix, row)
        self.assertEqual(ticket["named_test"], "catalog_parity::" + row["leaf_id"])
        self.assertEqual(ticket["file_allowlist"][1], "tests/compat/catalog/" + row["leaf_id"] + ".rs")
        self.assertEqual(set(ticket["scenario_evidence"]), set(census.SCENARIOS))
        self.assertEqual(ticket["source_anchors"], ["example.iff"])

    def test_pinned_denominator_survives_new_head_and_authored_fixtures(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()
            git("init")
            git("config", "user.email", "fixture@example.invalid")
            git("config", "user.name", "Inventory fixture")
            (root / "original.iff").write_text("source metadata fixture")
            git("add", "original.iff")
            git("commit", "-m", "pinned original fixture")
            baseline = git("rev-parse", "HEAD")
            (root / "authored.iff").write_text("newly authored fixture")
            git("add", "authored.iff")
            git("commit", "-m", "new authored fixture")
            self.assertNotEqual(git("rev-parse", "HEAD"), baseline)
            with patch.object(census, "BASELINE", baseline):
                self.assertEqual(census.git_source_paths(root), {"original.iff"})


if __name__ == "__main__":
    unittest.main()
