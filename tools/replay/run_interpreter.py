"""Execute the original/native/WASM interpreter cohort on the isolated Windows runner."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib
from compare_interpreter import compare, parse_trace, parse_state_hashes

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def require(ok: bool, message: str) -> None:
    if not ok:
        raise ValueError(message)

def run(root: Path, output: Path) -> int:
    root = root.resolve(strict=True)
    output.mkdir(parents=True, exist_ok=False)
    output = output.resolve()
    build = output.parent / "interpreter-build"
    build.mkdir(exist_ok=False)
    record = {
        "schema": 1, "passed": False, "scope": "interpreter-observable-projection-v1",
        "original_content_installation": False, "full_game_parity": False,
        "rendered_browser_test": False, "commands": [], "comparisons": {},
        "fault_controls": [], "executables": {},
    }
    env = dict(os.environ, CARGO_TARGET_DIR=str(build / "cargo"),
               WONDERLAND_INTERPRETER_TEST_ONLY="1")
    def command(label, args, timeout=120, exit_code=0):
        args = list(map(str,args))
        entry = {"label": label, "argv": args, "expected_exit": exit_code}
        record["commands"].append(entry)
        stdout_path, stderr_path = output/(label+".stdout"), output/(label+".stderr")
        with stdout_path.open("xb") as out, stderr_path.open("xb") as err:
            try:
                process = subprocess.run(args, cwd=root, env=env, stdout=out, stderr=err,
                                         timeout=timeout, check=False)
            except subprocess.TimeoutExpired:
                entry["timeout"] = timeout
                raise
        entry["exit"] = process.returncode
        entry["stdout_sha256"], entry["stderr_sha256"] = sha(stdout_path), sha(stderr_path)
        require(stdout_path.stat().st_size <= 16*1024*1024 and stderr_path.stat().st_size <= 16*1024*1024,
                label+": output limit")
        if process.returncode != exit_code:
            print(stdout_path.read_text(errors="replace")[-12000:])
            print(stderr_path.read_text(errors="replace")[-12000:])
            raise ValueError(label+f": exit {process.returncode}, expected {exit_code}")
        print(label+f": exit {process.returncode}", flush=True)
        return stdout_path, stderr_path

    def pair(label, left: Path, right: Path):
        result = compare(left.read_bytes(), right.read_bytes())
        (output/(label+".json")).write_text(json.dumps(result,indent=2)+"\n")
        record["comparisons"][label] = result
        require(result["passed"], label+": "+str(result["first_difference"]))
        print(label+f": {result['rows']} rows matched", flush=True)
        return result

    try:
        require(os.name == "nt", "original .NET Framework execution requires Windows")
        source = root/"TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff"
        require(sha(source) == "20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865",
                "original source resource drift")
        paths = [root/p for p in [
            "tools/replay/interpreter_probe.rs",
            "tools/replay/compare_interpreter.py",
            "tools/replay/run_interpreter.py",
            "tools/replay/run_interpreter_wasm.mjs",
            "tools/reference-csharp/interpreter-differential.cs",
            "tools/reference-csharp/vm-bootstrap.cs",
            "fixtures/reference/interpreter-authored.iff.hex",
            "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
        ]] + [source]
        record["inputs"] = {str(p.relative_to(root)).replace("\\","/"): sha(p) for p in paths}
        command("comparator-tests",[sys.executable,"-S",root/"tests/parity/test_interpreter_compare.py"])
        binary = root/"TSOClient/tso.simantics/bin/Release"
        refs = root.parent/"framework/Microsoft.NETFramework.ReferenceAssemblies.net45.1.0.3/build/.NETFramework/v4.5"
        vswhere = Path(os.environ["ProgramFiles(x86)"])/"Microsoft Visual Studio/Installer/vswhere.exe"
        found, _ = command("find-msbuild",[vswhere,"-latest","-requires","Microsoft.Component.MSBuild",
                                           "-find","MSBuild/**/Bin/MSBuild.exe"])
        csc = Path(found.read_text().strip().splitlines()[0]).parent/"Roslyn/csc.exe"
        executable = binary/"swarm-f-interpreter-differential.exe"
        compile_args = [csc,"/nologo","/target:exe","/platform:anycpu","/nostdlib+",
                        "/warn:4","/warnaserror+","/main:InterpreterDifferential",
                        "/out:"+str(executable)]
        for name in ("mscorlib","System","System.Core"):
            compile_args.append("/reference:"+str(refs/(name+".dll")))
        for name in ("FSO.SimAntics","FSO.LotView","FSO.Content","FSO.Files","FSO.Common",
                     "FSO.Vitaboy","FSO.Vitaboy.Engine","FSO.HIT","MonoGame.Framework"):
            require((binary/(name+".dll")).is_file(), "missing original dependency "+name)
            compile_args.append("/reference:"+str(binary/(name+".dll")))
        compile_args += [root/"tools/reference-csharp/vm-bootstrap.cs",
                         root/"tools/reference-csharp/interpreter-differential.cs"]
        command("compile-original-interpreter",compile_args)
        record["executables"]["original"] = {"sha256": sha(executable)}
        reference_traces = []
        for repeat in (1,2):
            identity = output/f"original-{repeat}.runtime.tsv"
            raw, stderr = command(f"original-{repeat}",[executable,root,identity],timeout=60)
            require(stderr.stat().st_size == 0, "unexpected original diagnostic")
            trace = output/f"original-{repeat}.tsv"
            trace.write_bytes(raw.read_bytes().replace(b"\r\n",b"\n"))
            parse_trace(trace.read_bytes())
            identity_lines = identity.read_text().splitlines()
            values = dict(line.split("\t") for line in identity_lines)
            require(len(values) == len(identity_lines) == 8, "original identity schema")
            require(values["clr"].startswith("4.0.") and values["vm_type"] == "FSO.SimAntics.VM"
                    and values["thread_type"] == "FSO.SimAntics.Engine.VMThread", "wrong CLR/VM")
            require(values["vm_sha256"] == sha(binary/"FSO.SimAntics.dll")
                    and values["witness_sha256"] == sha(executable), "loaded original binary mismatch")
            require(values["source_sha256"] == sha(source)
                    and values["authored_sha256"] == hashlib.sha256(bytes.fromhex(
                        (root/"fixtures/reference/interpreter-authored.iff.hex").read_text())).hexdigest()
                    and values["provider"] == "authored-in-memory", "original input/provider identity")
            record["executables"]["original"][f"runtime_{repeat}"] = values
            reference_traces.append(trace)
        pair("original-repeat",*reference_traces)

        toolchain = tomllib.loads((root/"rust-toolchain.toml").read_text())["toolchain"]["channel"]
        require(toolchain == "1.99.0", "review a changed Rust toolchain before requalification")
        command("rust-toolchain",["rustup","toolchain","install",toolchain,"--profile","minimal",
                                 "--target","wasm32-unknown-unknown"],timeout=300)
        command("rust-version",["rustc","+"+toolchain,"--version","--verbose"])
        for target in ("native","wasm"):
            args = ["cargo","+"+toolchain,"build","--locked","--lib","--no-default-features",
                    "-p","sim-core","-p","wonderland-content-runtime-bridge","-p","wonderland-legacy-formats"]
            if target == "wasm":
                args += ["--target","wasm32-unknown-unknown"]
            command("build-shipping-"+target,args,timeout=480)
        normal, normal_hashes = {}, {}
        for target in ("native","wasm"):
            libraries = build/"cargo"
            if target == "wasm":
                libraries /= "wasm32-unknown-unknown"
            libraries /= "debug"
            for flavor in ("normal","attribute","tick"):
                artifact = build/(target+"-"+flavor+(".wasm" if target=="wasm" else ".exe"))
                args = ["rustc","+"+toolchain,"--edition=2024","--crate-name","swarm_f_interpreter",
                        "-D","warnings","-C","opt-level=2",
                        "-L","dependency="+str(libraries/"deps")]
                for crate in ("sim_core","wonderland_content_runtime_bridge","wonderland_legacy_formats"):
                    args += ["--extern",crate+"="+str(libraries/("lib"+crate+".rlib"))]
                if target == "wasm":
                    args += ["--target","wasm32-unknown-unknown","--crate-type","cdylib",
                             "-L","dependency="+str(build/"cargo/debug/deps"),
                             "-C","link-arg=--max-memory=268435456"]
                if flavor != "normal":
                    args += ["--cfg","interpreter_fault_"+flavor]
                args += [root/"tools/replay/interpreter_probe.rs","-o",artifact]
                command("compile-"+target+"-"+flavor,args,timeout=180)
                record["executables"][target+"-"+flavor] = {
                    "sha256": sha(artifact), "bytes": artifact.stat().st_size}
                runs = []
                for repeat in ((1,2) if flavor=="normal" else (1,)):
                    label = target+"-"+flavor+f"-{repeat}"
                    trace, hashes = output/(label+".tsv"), output/(label+".hashes.tsv")
                    if target == "native":
                        raw, stderr = command(label,[artifact,hashes],timeout=60)
                        require(stderr.stat().st_size==0, "unexpected native diagnostic")
                        trace.write_bytes(raw.read_bytes().replace(b"\r\n",b"\n"))
                    else:
                        meta, stderr = command(label,["node",root/"tools/replay/run_interpreter_wasm.mjs",
                                                       artifact,trace,hashes],timeout=40)
                        require(stderr.stat().st_size==0,"unexpected WASM host diagnostic")
                        provenance = json.loads(meta.read_text())
                        require(provenance["wasm_sha256"]==sha(artifact)
                                and provenance["trace_sha256"]==sha(trace)
                                and provenance["state_hashes_sha256"]==sha(hashes)
                                and provenance["imports"]==[], "WASM execution provenance")
                        record["executables"][target+"-"+flavor][f"execution_{repeat}"] = provenance
                    parse_trace(trace.read_bytes())
                    parse_state_hashes(hashes.read_bytes())
                    runs.append((trace,hashes))
                    result = compare(reference_traces[0].read_bytes(),trace.read_bytes())
                    if flavor == "normal":
                        pair("original-"+label,reference_traces[0],trace)
                    else:
                        (output/(label+".difference.json")).write_text(json.dumps(result,indent=2)+"\n")
                        difference = result["first_difference"]
                        require(not result["passed"] and difference is not None, "fault was not detected")
                        require(difference == {
                            "case": "source4110", "ts1": 1, "tick": 7,
                            "field": "attribute_2" if flavor=="attribute" else "clock_ticks",
                            "expected": 30 if flavor=="attribute" else 7,
                            "actual": 31 if flavor=="attribute" else 6,
                        }, "fault first difference was not localized exactly")
                        record["fault_controls"].append({"target":target,"flavor":flavor,"detected":True,
                                                         "first_difference":difference})
                        print("detected "+label+": "+str(difference),flush=True)
                if flavor == "normal":
                    pair(target+"-repeat",runs[0][0],runs[1][0])
                    require(runs[0][1].read_bytes()==runs[1][1].read_bytes(), "native-state repeat drift")
                    normal[target], normal_hashes[target] = runs[0]
        pair("native-wasm",normal["native"],normal["wasm"])
        require(normal_hashes["native"].read_bytes()==normal_hashes["wasm"].read_bytes(),
                "full native/WASM canonical state hash mismatch")
        record["native_wasm_state_hashes"] = {
            "rows":128,"passed":True,"sha256":sha(normal_hashes["native"]),
            "cross_language_original_state_hash":False,
        }
        # An empty but valid WebAssembly binary must not be accepted as execution.
        empty = build/"empty.wasm"; empty.write_bytes(b"\0asm\x01\0\0\0")
        _, err = command("wasm-missing-exports",["node",root/"tools/replay/run_interpreter_wasm.mjs",
                         empty,output/"invalid.tsv",output/"invalid.hashes.tsv"],exit_code=1,timeout=40)
        require("missing WASM memory" in err.read_text()
                and not (output/"invalid.tsv").exists(), "WASM negative control did not fail before output")
        require(record["inputs"] == {str(p.relative_to(root)).replace("\\","/"):sha(p) for p in paths},
                "source inputs changed during differential execution")
        record["passed"] = True
    except (OSError,ValueError,AssertionError,KeyError,subprocess.SubprocessError) as error:
        record["error"] = str(error)
        print("FAIL: "+str(error),file=sys.stderr,flush=True)
    finally:
        record["evidence_sha256"] = {p.name:sha(p) for p in sorted(output.iterdir()) if p.is_file()}
        (output/"execution.json").write_text(json.dumps(record,indent=2)+"\n")
        print(json.dumps({k:v for k,v in record.items()
                          if k not in ("commands","inputs","evidence_sha256","executables","comparisons")},indent=2))
    return 0 if record["passed"] else 1

if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args()
    raise SystemExit(run(args.root,args.output))
