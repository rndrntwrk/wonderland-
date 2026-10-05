#!/usr/bin/env python3
"""Compare five unchanged original C# EOD handlers with actual NativeHost execution.

Only authored synthetic scenarios enter this controlled component oracle. See
docs/swarm-b/eod-source-oracle.md for the explicit VM/provider/scheduler boundary.
Requires Mono and Rust/Cargo 1.90. No installation, application UI or durable
service is started. Native authentication/checkpoint/provider policies are not
claims about the original application.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "fixtures/eod/source-oracle"
BASE = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
SOURCE_MANIFEST = FIXTURES / "sources.json"
MAX_INPUT_BYTES = 2 * 1024 * 1024
MAX_TRACE_BYTES = 16 * 1024 * 1024
MAX_LINE_BYTES = 300 * 1024
MAX_STEPS = 4096
MAX_RECORDS = 100_000
PLUGIN_IDS = {
    "timer": "aa65fe9e", "dance": "4a5be8ab", "signs": "2a6356a0",
    "scoreboard": "0949e698", "door": "0a69f29f",
}
OP_ARITY = {
    "reset": 0, "seed": 3, "connect": 10, "controller": 3,
    "release": 0, "policy_release": 0, "tick": 1, "registers": 2,
    "text": 3, "binary": 3, "policy_binary": 3, "disconnect": 1,
}
NATIVE_BOUNDARY_TESTS = {
    "boundary_tests::operation_error_still_observes_real_native_disconnect_outputs",
    "boundary_tests::operation_error_still_checkpoints_and_observes_a_no_ui_color_save",
    "boundary_tests::later_provider_error_preserves_earlier_load_ui_and_primary_error",
}


class TraceMismatch(ValueError):
    """A complete ordered source/native trace differs."""


def empty_stage():
    return {"vm": [], "ui": {}, "provider": [], "errors": []}


def compare_traces(reference, candidate):
    """Compare complete stages; only independent private recipient order is free."""
    if list(reference) != list(candidate):
        raise TraceMismatch("complete ordered stage identities/count differ")
    for stage, expected in reference.items():
        actual = candidate[stage]
        if set(expected) != set(actual):
            raise TraceMismatch(f"{stage}: output channel set differs")
        for channel in expected:
            if expected[channel] != actual[channel]:
                raise TraceMismatch(
                    f"{stage}: {channel} differs\n"
                    f"  source={expected[channel]!r}\n  native={actual[channel]!r}")


def _uint(text, maximum):
    if not re.fullmatch(r"0|[1-9][0-9]*", text) or int(text) > maximum:
        raise ValueError(f"noncanonical bounded integer: {text!r}")


def _short(text):
    if not re.fullmatch(r"0|-?[1-9][0-9]*", text) or not -32768 <= int(text) <= 32767:
        raise ValueError(f"noncanonical Int16: {text!r}")


def _hex(text):
    if text != "-" and (len(text) > 256 * 1024 or not re.fullmatch(r"(?:[0-9a-f]{2})+", text)):
        raise ValueError("noncanonical or oversized byte payload")


def parse_trace(text, expected_steps):
    """Reject incomplete, unknown or malformed outputs before comparing any bytes."""
    if len(text.encode("utf-8")) > MAX_TRACE_BYTES:
        raise ValueError("trace byte limit")
    result = {}
    current = None
    count = 0
    for raw in text.splitlines():
        if len(raw) > MAX_LINE_BYTES:
            raise ValueError("trace line limit")
        parts = raw.split("\t")
        tag = parts[0]
        if tag == "BEGIN":
            key = tuple(parts[1:])
            if len(parts) != 3 or current is not None or key in result:
                raise ValueError("duplicate, nested or malformed stage")
            if len(result) >= len(expected_steps) or key != expected_steps[len(result)]:
                raise ValueError("unexpected or reordered stage")
            result[key] = empty_stage()
            current = result[key]
        elif tag == "END":
            if len(parts) != 3 or current is None or tuple(parts[1:]) != next(reversed(result)):
                raise ValueError("unmatched stage end")
            current = None
        else:
            if current is None:
                raise ValueError("record outside stage")
            count += 1
            if count > MAX_RECORDS:
                raise ValueError("trace record limit")
            if tag == "VM" and len(parts) == 4:
                _uint(parts[1], 32767)
                _short(parts[2])
                if parts[3] != "-":
                    temps = parts[3].split(",")
                    if len(temps) > 4:
                        raise ValueError("object event temporary-register limit")
                    for value in temps:
                        _short(value)
                current["vm"].append(parts[1:])
            elif tag == "UI" and len(parts) == 6:
                _uint(parts[1], 2**32 - 1)
                if parts[2] not in PLUGIN_IDS.values() or parts[3] not in ("text", "binary"):
                    raise ValueError("invalid private plugin or payload kind")
                if not re.fullmatch(r"[A-Za-z0-9_]{1,96}", parts[4]):
                    raise ValueError("invalid private event name")
                _hex(parts[5])
                current["ui"].setdefault(parts[1], []).append(parts[2:])
            elif tag == "PROVIDER" and len(parts) in (4, 5, 6):
                kind = parts[1]
                if (kind, len(parts)) not in (("request", 4), ("result", 6), ("save", 5)):
                    raise ValueError("invalid provider record shape")
                _uint(parts[2], 2**32 - 1)
                if parts[3] not in PLUGIN_IDS.values():
                    raise ValueError("invalid provider plugin")
                if kind == "result":
                    if parts[4] not in ("0", "1") or (parts[4] == "0" and parts[5] != "-"):
                        raise ValueError("invalid missing-record result")
                    _hex(parts[5])
                elif kind == "save":
                    _hex(parts[4])
                current["provider"].append(parts[1:])
            elif tag == "ERROR" and len(parts) == 2 and re.fullmatch(r"[A-Za-z0-9]{1,80}", parts[1]):
                current["errors"].append(parts[1])
            else:
                raise ValueError(f"unknown or malformed trace record {tag!r}")
    if current is not None or list(result) != expected_steps:
        raise ValueError("incomplete trace or missing empty stage")
    return result


def bounded_read(path, limit=MAX_INPUT_BYTES):
    with Path(path).open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError(f"input byte limit: {path}")
    return data


def load_scenarios(path):
    document = json.loads(bounded_read(path))
    if document.get("version") != 1 or not isinstance(document.get("cases"), list):
        raise ValueError("unsupported scenario document")
    lines, stages, names = [], [], set()
    for case in document["cases"]:
        name = case["name"]
        if not re.fullmatch(r"[a-z][a-z0-9_]{0,63}", name) or name in names or case.get("plugin") not in PLUGIN_IDS:
            raise ValueError("invalid or duplicate scenario name")
        names.add(name)
        if not case["steps"] or case["steps"][0] != ["reset"]:
            raise ValueError("each scenario must reset both runtimes first")
        for index, step in enumerate(case["steps"]):
            if not isinstance(step, list) or not step or OP_ARITY.get(step[0]) != len(step) - 1:
                raise ValueError(f"{name}:{index}: invalid operation or arity")
            if any(not isinstance(value, str) or not value or any(c in value for c in "\t\n\r") for value in step):
                raise ValueError("protocol fields must be nonempty single-line strings")
            line = "\t".join([name, str(index), *step])
            if not line.isascii() or len(line) > MAX_LINE_BYTES:
                raise ValueError("scenario line/ASCII limit")
            lines.append(line)
            stages.append((name, str(index)))
            if len(stages) > MAX_STEPS:
                raise ValueError("scenario step limit")
        differences = case.get("differences", {})
        policy_steps = {str(index) for index, step in enumerate(case["steps"])
                        if step[0].startswith("policy_")}
        if not isinstance(differences, dict) or set(differences) != policy_steps:
            raise ValueError("policy operations and exact difference declarations must pair")
        if differences and (not isinstance(case.get("policy"), str) or not case["policy"]):
            raise ValueError("policy difference requires a named policy")
        for policy in differences.values():
            if not isinstance(policy, dict) or set(policy) != {"source", "native"}:
                raise ValueError("policy difference requires both exact source/native outcomes")
    wire = ("\n".join(lines) + "\n").encode("ascii")
    if len(wire) > MAX_INPUT_BYTES:
        raise ValueError("rendered scenario byte limit")
    return document, wire, stages


def compare_scenarios(document, source, native):
    common_source, common_native = {}, {}
    differences = []
    for case in document["cases"]:
        name = case["name"]
        for index in range(len(case["steps"])):
            key = (name, str(index))
            policy = case.get("differences", {}).get(str(index))
            if policy is None:
                common_source[key] = source[key]
                common_native[key] = native[key]
            else:
                for side, actual in (("source", source[key]), ("native", native[key])):
                    expected = empty_stage()
                    expected.update(policy[side])
                    compare_traces({key: expected}, {key: actual})
                if source[key] == native[key]:
                    raise TraceMismatch(f"{key}: named policy difference disappeared")
                differences.append({"case": name, "step": index, "policy": case["policy"]})
        for assertion in case.get("assertions", []):
            key = (name, str(assertion["step"]))
            for actual in (source[key], native[key]):
                channel = actual[assertion["channel"]]
                if "recipient" in assertion:
                    channel = channel.get(assertion["recipient"], [])
                if channel != assertion["equals"]:
                    raise TraceMismatch(f"{key}: literal {assertion['channel']} assertion failed: {channel!r}")
    compare_traces(common_source, common_native)
    return common_source, differences


def negative_controls(reference):
    """Mutate actual compared traces; every mutation must fail the comparator."""
    controls = []

    def reject(label, change):
        altered = copy.deepcopy(reference)
        change(altered)
        try:
            compare_traces(reference, altered)
        except TraceMismatch:
            controls.append(label)
        else:
            raise AssertionError(f"negative control escaped comparison: {label}")

    vm = next(key for key, value in reference.items() if value["vm"])
    ui = next((key, actor) for key, value in reference.items() for actor, records in value["ui"].items() if records)
    saved = next((key, index) for key, value in reference.items() for index, record in enumerate(value["provider"]) if record[0] == "save")
    vm_order = next(key for key, value in reference.items() if len(value["vm"]) > 1 and value["vm"] != list(reversed(value["vm"])))
    ui_order = next((key, actor) for key, value in reference.items() for actor, records in value["ui"].items() if len(records) > 1 and records != list(reversed(records)))
    reject("changed_vm_code", lambda trace: trace[vm]["vm"][0].__setitem__(1, "12345"))
    reject("changed_private_bytes", lambda trace: trace[ui[0]]["ui"][ui[1]][0].__setitem__(3, "feed"))
    reject("changed_private_kind", lambda trace: trace[ui[0]]["ui"][ui[1]][0].__setitem__(1, "invalid"))
    reject("changed_save_bytes", lambda trace: trace[saved[0]]["provider"][saved[1]].__setitem__(3, "feed"))
    reject("reordered_vm_effects", lambda trace: trace[vm_order]["vm"].reverse())
    reject("reordered_private_effects", lambda trace: trace[ui_order[0]]["ui"][ui_order[1]].reverse())
    reject("dropped_stage", lambda trace: trace.pop(next(iter(trace))))
    reject("dropped_provider_effect", lambda trace: trace[saved[0]]["provider"].pop(saved[1]))

    def reroute(trace):
        messages = trace[ui[0]]["ui"].pop(ui[1])
        trace[ui[0]]["ui"]["4294967295"] = messages
    reject("misrouted_private_recipient", reroute)
    return controls


def source_manifest():
    """Public source-input inventory for aggregate verification digest binding."""
    manifest = json.loads(bounded_read(SOURCE_MANIFEST))
    if manifest["source_commit"] != BASE or len(manifest["files"]) != 18:
        raise ValueError("unexpected pinned source set")
    seen = set()
    for entry in manifest["files"]:
        name = entry["path"]
        if name in seen or not name.startswith("TSOClient/") or ".." in Path(name).parts:
            raise ValueError("invalid source input path")
        seen.add(name)
    return manifest


def verify_original_sources(manifest):
    for entry in manifest["files"]:
        name = entry["path"]
        actual = bounded_read(ROOT / name)
        original = subprocess.check_output(["git", "show", f"{BASE}:{name}"], cwd=ROOT, timeout=20)
        if actual != original or hashlib.sha256(actual).hexdigest() != entry["sha256"]:
            raise ValueError(f"original source differs from pinned baseline: {name}")


def input_digest(manifest, scenario_path):
    paths = {ROOT / entry["path"] for entry in manifest["files"]}
    paths.update(path for path in FIXTURES.rglob("*") if path.is_file() and "__pycache__" not in path.parts)
    paths.update((ROOT / "crates/eod-runtime").rglob("*.rs"))
    paths.update([Path(__file__), Path(scenario_path), ROOT / "crates/eod-runtime/Cargo.toml", ROOT / "crates/eod-runtime/Cargo.lock"])
    digest = hashlib.sha256()
    for path in sorted(paths):
        data = bounded_read(path)
        name = str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path)
        digest.update(name.encode())
        digest.update(b"\0")
        digest.update(len(data).to_bytes(8, "little"))
        digest.update(data)
    return digest.hexdigest()


def run_logged(label, command, logs, env, input_bytes=None, timeout=90):
    stdout_path, stderr_path = logs / f"{label}.stdout", logs / f"{label}.stderr"
    with stdout_path.open("wb") as out, stderr_path.open("wb") as err:
        result = subprocess.run(command, cwd=ROOT, env=env, input=input_bytes,
                                stdout=out, stderr=err, timeout=timeout)
    if result.returncode:
        stderr = bounded_read(stderr_path, MAX_TRACE_BYTES).decode("utf-8", "replace")
        stdout = bounded_read(stdout_path, MAX_TRACE_BYTES).decode("utf-8", "replace")
        raise RuntimeError(f"{label} exited {result.returncode}\n{stdout[-6000:]}\n{stderr[-10000:]}")
    return bounded_read(stdout_path, MAX_TRACE_BYTES).decode("utf-8", "strict")


def trace_json(trace):
    return [{"case": key[0], "step": int(key[1]), **value} for key, value in trace.items()]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default=os.environ.get("B_CARGO_BIN") or str(Path.home() / ".cargo/bin/cargo"))
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--logs-dir", type=Path)
    parser.add_argument("--scenarios", type=Path, default=FIXTURES / "scenarios.json")
    args = parser.parse_args()
    logs = (args.logs_dir or Path(tempfile.mkdtemp(prefix="wonderland-eod-oracle-logs-"))).resolve()
    logs.mkdir(parents=True, exist_ok=True)
    report = {"schema": 1, "result": "fail", "scope": "unchanged_handler_component_with_controlled_adapters",
              "original_application_runtime": "unverified", "ui_runtime": "unverified",
              "vm_adapter_integration": "unverified", "production_provider": "unverified"}
    try:
        for program in ("mcs", "mono"):
            if not shutil.which(program):
                raise RuntimeError(f"{program} is required")
        env = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0",
                   CARGO_PROFILE_TEST_DEBUG="0", PYTHONDONTWRITEBYTECODE="1", RUSTUP_TOOLCHAIN="1.90.0")
        if os.path.isabs(args.cargo):
            env["PATH"] = str(Path(args.cargo).parent) + os.pathsep + env.get("PATH", "")
        cargo_version = subprocess.check_output([args.cargo, "--version"], env=env, text=True, timeout=20).strip()
        if not cargo_version.startswith("cargo 1.90.0 "):
            raise RuntimeError(f"Rust/Cargo 1.90.0 required; found {cargo_version}")
        manifest = source_manifest()
        verify_original_sources(manifest)
        initial = input_digest(manifest, args.scenarios)
        document, wire, steps = load_scenarios(args.scenarios)
        (logs / "scenario-input.tsv").write_bytes(wire)
        run_logged("comparator-tests", [sys.executable, "-m", "unittest", "discover", "-s", str(FIXTURES), "-p", "test_*.py"], logs, env)
        comparator_output = bounded_read(logs / "comparator-tests.stderr", MAX_TRACE_BYTES).decode("utf-8")
        comparator_summary = re.search(r"(?m)^Ran ([1-9][0-9]*) tests in ", comparator_output)
        if comparator_summary is None or not comparator_output.rstrip().endswith("OK"):
            raise RuntimeError("comparator test run did not report a positive complete success")
        comparator_count = int(comparator_summary.group(1))
        with tempfile.TemporaryDirectory(prefix="wonderland-eod-source-") as temporary:
            work = Path(temporary)
            exe = work / "source-oracle.exe"
            target = (args.target_dir or work / "target").resolve()
            sources = [str(ROOT / entry["path"]) for entry in manifest["files"]]
            adapters = [str(FIXTURES / name) for name in ("CSharpOracle.cs", "RuntimeAdapters.cs", "QueuedScheduler.cs")]
            run_logged("csharp-build", ["mcs", "-warn:4", f"-out:{exe}", *adapters, *sources], logs, env)
            run_logged("native-build", [args.cargo, "build", "--locked", "--offline", "--manifest-path", "crates/eod-runtime/Cargo.toml", "--example", "source-oracle", "--target-dir", str(target)], logs, env)
            boundary_output = run_logged("native-boundary-tests", [args.cargo, "test", "--locked", "--offline", "--manifest-path", "crates/eod-runtime/Cargo.toml", "--example", "source-oracle", "--target-dir", str(target)], logs, env)
            boundary_names = re.findall(r"(?m)^test (boundary_tests::[a-z0-9_]+) \.\.\. ok$", boundary_output)
            boundary_summary = re.search(r"(?m)^test result: ok\. ([1-9][0-9]*) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;", boundary_output)
            if (boundary_summary is None or set(boundary_names) != NATIVE_BOUNDARY_TESTS
                    or len(boundary_names) != len(NATIVE_BOUNDARY_TESTS)
                    or int(boundary_summary.group(1)) != len(boundary_names)):
                raise RuntimeError("native boundary regressions did not all execute and pass")
            source = parse_trace(run_logged("source-trace", ["mono", str(exe)], logs, env, wire), steps)
            native_binary = target / "debug/examples/source-oracle"
            native = parse_trace(run_logged("native-trace", [str(native_binary)], logs, env, wire), steps)
            compare_traces(source, parse_trace(run_logged("source-repeat", ["mono", str(exe)], logs, env, wire), steps))
            compare_traces(native, parse_trace(run_logged("native-repeat", [str(native_binary)], logs, env, wire), steps))
        common, differences = compare_scenarios(document, source, native)
        controls = negative_controls(common)
        verify_original_sources(manifest)
        final = input_digest(manifest, args.scenarios)
        if final != initial:
            raise RuntimeError("verification inputs changed while oracle was running")
        source_json, native_json = trace_json(source), trace_json(native)
        (logs / "source-trace.json").write_text(json.dumps(source_json, indent=2) + "\n")
        (logs / "native-trace.json").write_text(json.dumps(native_json, indent=2) + "\n")
        canonical = json.dumps(trace_json(common), sort_keys=True, separators=(",", ":")).encode()
        by_plugin = []
        for name, plugin_id in PLUGIN_IDS.items():
            cases = [case for case in document["cases"] if case["plugin"] == name]
            selected = {case["name"] for case in cases}
            stages = [value for key, value in common.items() if key[0] in selected]
            by_plugin.append({"plugin": name, "plugin_id": plugin_id, "scenarios": len(cases),
                              "compared_stages": len(stages), "vm_events": sum(len(x["vm"]) for x in stages),
                              "private_events": sum(sum(map(len, x["ui"].values())) for x in stages),
                              "provider_events": sum(len(x["provider"]) for x in stages)})
        report.update(result="pass", source_commit=BASE, source_files=manifest["files"],
                      input_sha256=initial, comparison_sha256=hashlib.sha256(canonical).hexdigest(),
                      cargo=cargo_version, mono=subprocess.check_output(["mono", "--version"], text=True).splitlines()[0],
                      scenarios=len(document["cases"]), stages=len(steps), compared_stages=len(common),
                      compared_vm_events=sum(len(x["vm"]) for x in common.values()),
                      compared_private_events=sum(sum(map(len, x["ui"].values())) for x in common.values()),
                      compared_provider_events=sum(len(x["provider"]) for x in common.values()),
                      literal_channel_assertions=sum(len(case.get("assertions", [])) for case in document["cases"]),
                      comparator_tests={"passed": comparator_count, "stdout_log": "comparator-tests.stdout", "stderr_log": "comparator-tests.stderr"},
                      native_boundary_tests={"passed": len(boundary_names), "names": sorted(boundary_names),
                                             "stdout_log": "native-boundary-tests.stdout", "stderr_log": "native-boundary-tests.stderr"},
                      by_plugin=by_plugin,
                      deterministic_repetitions=2, negative_controls=controls,
                      asserted_policy_differences=differences, policy_exclusions=document["policy_exclusions"])
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        (logs / "eod-source-oracle.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report[key] for key in ("result", "scenarios", "stages", "compared_stages", "compared_vm_events", "compared_private_events", "compared_provider_events", "comparison_sha256")}, sort_keys=True))
    print(f"Original EOD handler component oracle: PASS; evidence: {logs}")


if __name__ == "__main__":
    main()
