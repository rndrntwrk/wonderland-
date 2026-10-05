"""Regressions for false-positive oracle comparisons; no runtimes required."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "eod_source_oracle", ROOT / "tools/swarm-b/eod-source-oracle.py")
ORACLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ORACLE)


def sample():
    return {
        ("door", "1"): {
            "vm": [["101", "1", "-"], ["101", "-1", "-"]],
            "ui": {"1001": [["0a69f29f", "text", "door_init", "30"],
                              ["0a69f29f", "text", "door_code", "31323334"]]},
            "provider": [["save", "77", "0a69f29f", "31323334"]],
            "errors": [],
        },
        ("door", "2"): {"vm": [], "ui": {}, "provider": [], "errors": []},
    }


class ComparatorTests(unittest.TestCase):
    def test_equal_complete_traces_pass(self):
        ORACLE.compare_traces(sample(), copy.deepcopy(sample()))

    def assert_rejected(self, changed):
        with self.assertRaises(ORACLE.TraceMismatch):
            ORACLE.compare_traces(sample(), changed)

    def test_changed_vm_argument_is_rejected(self):
        changed = sample()
        changed[("door", "1")]["vm"][0][2] = "7"
        self.assert_rejected(changed)

    def test_reordered_vm_close_events_are_rejected(self):
        changed = sample()
        changed[("door", "1")]["vm"].reverse()
        self.assert_rejected(changed)

    def test_changed_private_byte_is_rejected(self):
        changed = sample()
        changed[("door", "1")]["ui"]["1001"][1][3] = "31323335"
        self.assert_rejected(changed)

    def test_reordered_private_events_are_rejected(self):
        changed = sample()
        changed[("door", "1")]["ui"]["1001"].reverse()
        self.assert_rejected(changed)

    def test_private_recipient_change_is_rejected(self):
        changed = sample()
        changed[("door", "1")]["ui"]["1002"] = changed[("door", "1")]["ui"].pop("1001")
        self.assert_rejected(changed)

    def test_private_payload_kind_change_is_rejected(self):
        changed = sample()
        changed[("door", "1")]["ui"]["1001"][0][1] = "binary"
        self.assert_rejected(changed)

    def test_changed_persistence_byte_is_rejected(self):
        changed = sample()
        changed[("door", "1")]["provider"][0][3] = "31323335"
        self.assert_rejected(changed)

    def test_dropped_empty_stage_is_rejected(self):
        changed = sample()
        del changed[("door", "2")]
        self.assert_rejected(changed)

    def test_dropped_provider_channel_is_rejected(self):
        changed = sample()
        changed[("door", "1")]["provider"].clear()
        self.assert_rejected(changed)


class ProtocolTests(unittest.TestCase):
    STEPS = [("timer", "0")]
    VALID = "BEGIN\ttimer\t0\nVM\t101\t-2\t-\nUI\t1001\taa65fe9e\tbinary\tTimer_Show\t00000102\nEND\ttimer\t0\n"

    def test_complete_trace_keeps_event_kind_and_exact_bytes(self):
        trace = ORACLE.parse_trace(self.VALID, self.STEPS)
        self.assertEqual(trace, {
            ("timer", "0"): {
                "vm": [["101", "-2", "-"]], "provider": [], "errors": [],
                "ui": {"1001": [["aa65fe9e", "binary", "Timer_Show", "00000102"]]},
            }})

    def test_missing_end_marker_is_rejected(self):
        with self.assertRaises(ValueError):
            ORACLE.parse_trace(self.VALID.rsplit("END", 1)[0], self.STEPS)

    def test_missing_empty_stage_is_rejected(self):
        with self.assertRaises(ValueError):
            ORACLE.parse_trace(self.VALID, self.STEPS + [("timer", "1")])

    def test_unknown_output_channel_is_rejected(self):
        with self.assertRaises(ValueError):
            ORACLE.parse_trace(self.VALID.replace("VM\t", "SECRET\t"), self.STEPS)

    def test_noncanonical_hex_is_rejected(self):
        with self.assertRaises(ValueError):
            ORACLE.parse_trace(self.VALID.replace("00000102", "f"), self.STEPS)

    def test_duplicate_stage_is_rejected(self):
        with self.assertRaises(ValueError):
            ORACLE.parse_trace(self.VALID * 2, self.STEPS)

    def test_record_outside_stage_is_rejected(self):
        with self.assertRaises(ValueError):
            ORACLE.parse_trace("VM\t101\t-2\t-\n" + self.VALID, self.STEPS)

    def test_unadvertised_private_extension_column_is_rejected(self):
        with self.assertRaises(ValueError):
            ORACLE.parse_trace(self.VALID.replace("00000102\n", "00000102\t1\n"), self.STEPS)


class ScenarioPolicyTests(unittest.TestCase):
    def load(self, document):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "scenarios.json"
            path.write_text(json.dumps(document))
            return ORACLE.load_scenarios(path)

    def test_policy_operation_requires_exact_difference_declaration(self):
        document = {"version": 1, "cases": [{
            "name": "unaccounted_error", "plugin": "scoreboard",
            "steps": [["reset"], ["policy_release"]],
        }]}
        with self.assertRaisesRegex(ValueError, "policy"):
            self.load(document)

    def test_difference_declaration_requires_policy_operation(self):
        document = {"version": 1, "cases": [{
            "name": "unmarked_exception", "plugin": "scoreboard",
            "policy": "malformed_provider_record_fails_closed",
            "steps": [["reset"], ["release"]],
            "differences": {"1": {"source": {}, "native": {}}},
        }]}
        with self.assertRaisesRegex(ValueError, "policy"):
            self.load(document)

    def test_exact_policy_error_rejects_residual_outputs_and_secondary_errors(self):
        case = "error_boundary"
        document = {"cases": [{
            "name": case, "steps": [["reset"], ["policy_release"]],
            "policy": "injected_error",
            "differences": {"1": {"source": {}, "native": {"errors": ["PluginNotReady"]}}},
        }]}
        source = {(case, "0"): ORACLE.empty_stage(), (case, "1"): ORACLE.empty_stage()}
        native = copy.deepcopy(source)
        native[(case, "1")]["errors"] = ["PluginNotReady"]
        ORACLE.compare_scenarios(document, source, native)
        residuals = {
            "vm": [["101", "-1", "-"]],
            "ui": {"1001": [["0949e698", "text", "eod_leave", "-"]]},
            "provider": [["save", "77", "0949e698", "000500000000"]],
            "errors": ["PluginNotReady", "InvalidPluginData"],
        }
        for channel, records in residuals.items():
            with self.subTest(channel=channel):
                changed = copy.deepcopy(native)
                changed[(case, "1")][channel] = records
                with self.assertRaises(ORACLE.TraceMismatch):
                    ORACLE.compare_scenarios(document, source, changed)


if __name__ == "__main__":
    unittest.main()
