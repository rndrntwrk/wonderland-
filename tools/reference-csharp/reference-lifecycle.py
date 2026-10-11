#!/usr/bin/env python3
"""Compile pinned original lifecycle/scheduler methods with explicit test shims.

Default execution verifies and prints the checked-in TSV vectors. --record is
only for intentionally refreshing that fixture after inspecting source/output.
No Rust result or full original VM execution is inferred from this harness.
"""
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

BASELINE = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
SHA256 = {
    "Entities/VMEntity.cs": "047c32652f4e0d456598b67609e7633836a59aedf3e4f5a9be3d04181c04f37a",
    "Engine/VMThread.cs": "294616ef1f4a67283657744f6e2970e6ee9fc42161135c709bb5029040a49692",
    "Engine/VMStackFrame.cs": "172ef549bb9ac6b9a4ee548b9d78b7ed30eb2b3bc94d37c6f86e5a3cedaf7cc2",
    "VM.cs": "a4bfc3203cb907ba12bef6aa21ba33a7024b915bc06689eead71b3bfa4fda866",
    "VMContext.cs": "765ee010a2391f9ac1b925304809d1af387de4dd064a5b7892dccdc82779c29e",
    "Engine/VMScheduler.cs": "ed06974cbd0817c2e255332ed4020420a3a6338896cb818a1cfda417756357c6",
    "Primitives/VMNotifyOutOfIdle.cs": "74bcea8e7746dd10b3c3d511e27897bb014b8fa1d245590f506124925427bfbd",
    "Engine/VMPrimitiveHandler.cs": "0d294cb7fa1577e0924c21855fe37b4f82cbc46c9570a600297245d4e39865bf",
    "Engine/VMPrimitiveOperand.cs": "2688f609594647149efa6456781961524f9c34b23a86ee04e8e57c1bfcd5e3d9",
    "Engine/VMPrimitiveExitCode.cs": "ec0e065b25cea2152c6a93de8936b84110219c38e77f998e45abb12b9ff962f3",
}
WHOLE_FILES = (
    "Engine/VMScheduler.cs",
    "Primitives/VMNotifyOutOfIdle.cs",
    "Engine/VMPrimitiveHandler.cs",
    "Engine/VMPrimitiveOperand.cs",
    "Engine/VMPrimitiveExitCode.cs",
)


def source_block(text: str, signature: str) -> str:
    """Return the original declaration/block, ignoring braces inside comments."""
    start = text.index(signature)
    lexical = re.sub(
        r'//[^\n]*|/\*.*?\*/|@"(?:[^"]|"")*"|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'',
        lambda match: "".join("\n" if char == "\n" else " " for char in match.group()),
        text,
        flags=re.S,
    )
    opening = lexical.index("{", start)
    depth = 0
    for index in range(opening, len(lexical)):
        if lexical[index] == "{":
            depth += 1
        elif lexical[index] == "}":
            depth -= 1
            if depth == 0:
                return text[start : index + 1]
    raise ValueError(f"unterminated source block: {signature}")


def main() -> int:
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=here.parents[1], help="FreeSO repository root")
    parser.add_argument("--record", action="store_true", help="write inspected Mono output as the expected TSV")
    args = parser.parse_args()
    source = args.source.resolve() / "TSOClient/tso.simantics"
    texts = {}
    for relative, expected in SHA256.items():
        data = (source / relative).read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError(f"source checksum mismatch for {relative}; expected baseline {BASELINE}")
        texts[relative] = data.decode("utf-8-sig")
    if not shutil.which("mcs") or not shutil.which("mono"):
        raise RuntimeError("mcs and mono are required; this native reference check is not a WASM test")

    entity = texts["Entities/VMEntity.cs"]
    thread = texts["Engine/VMThread.cs"]
    frame = texts["Engine/VMStackFrame.cs"]
    blocks = {
        "VM_ADD_TO_OBJ_LIST": source_block(texts["VM.cs"], "public static void AddToObjList("),
        "VM_NEXT_RANDOM": source_block(texts["VMContext.cs"], "public ulong NextRandom("),
        "VM_ENTITY_TICK": source_block(entity, "public virtual void Tick()"),
        "VM_RUN_EVERY_FRAME": source_block(entity, "public bool RunEveryFrame()"),
        "VM_ENTITY_INIT": source_block(entity, "public virtual void Init("),
        "VM_ENTITY_RESET": source_block(entity, "public virtual void Reset("),
        "VM_ENTRYPOINT_3": source_block(entity, "public bool ExecuteEntryPoint(int entry, VMContext context, bool runImmediately)"),
        "VM_ENTRYPOINT_4": source_block(entity, "public bool ExecuteEntryPoint(int entry, VMContext context, bool runImmediately, VMEntity stackOBJ)"),
        "VM_ENTRYPOINT_5": source_block(entity, "public bool ExecuteEntryPoint(int entry, VMContext context, bool runImmediately, VMEntity stackOBJ, short[] args)"),
        "VM_GENERIC_ENTRYPOINT": source_block(entity, "public bool ExecuteGenericEntryPoint("),
        "VM_STACK_OBJECT": source_block(frame, "public VMEntity StackObject\n"),
        "VM_STACK_OBJECT_ID": source_block(frame, "public short StackObjectID\n"),
        "VM_THREAD_CTOR": source_block(thread, "public VMThread(VMContext context, VMEntity entity, int stackSize)"),
        "VM_THREAD_PUSH": source_block(thread, "public bool Push(VMStackFrame frame)"),
        "VM_CHECK_3": source_block(thread, "public static VMPrimitiveExitCode EvaluateCheck(VMContext context, VMEntity entity, VMStackFrame initFrame)"),
        "VM_CHECK_4": source_block(thread, "public static VMPrimitiveExitCode EvaluateCheck(VMContext context, VMEntity entity, VMStackFrame initFrame, VMQueuedAction action)"),
        "VM_CHECK_5": source_block(thread, "public static VMPrimitiveExitCode EvaluateCheck(VMContext context, VMEntity entity, VMStackFrame initFrame, VMQueuedAction action, List<VMPieMenuInteraction> actionStrings)"),
        "VM_EMPTY_STACK_MAIN": source_block(thread[thread.index("public void Tick()") :], "if (Stack.Count == 0)"),
    }
    generated = (here / "reference-lifecycle.cs").read_text()
    for name, block in blocks.items():
        marker = "@@" + name + "@@"
        if generated.count(marker) != 1:
            raise ValueError(f"expected one template marker: {marker}")
        generated = generated.replace(marker, block)
    if "@@" in generated:
        raise ValueError("unexpanded template marker")

    with tempfile.TemporaryDirectory(prefix="freeso-lifecycle-") as temporary:
        build = Path(temporary)
        harness = build / "reference-lifecycle.generated.cs"
        executable = build / "reference-lifecycle.exe"
        harness.write_text(generated)
        subprocess.run(
            ["mcs", "-langversion:7.2", "-checked-", "-warn:0", "-r:System.Core",
             "-out:" + str(executable), str(harness), *(str(source / name) for name in WHOLE_FILES)],
            check=True, timeout=60,
        )
        output = subprocess.run(
            ["mono", str(executable)], check=True, text=True, capture_output=True, timeout=30
        ).stdout.replace("\r\n", "\n")
    fixture = here / "reference-lifecycle.expected.tsv"
    if args.record:
        fixture.write_text(output)
    elif fixture.read_text() != output:
        import difflib
        sys.stderr.writelines(difflib.unified_diff(
            fixture.read_text().splitlines(True), output.splitlines(True),
            fromfile="expected", tofile="actual Mono",
        ))
        return 1
    sys.stdout.write(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
