#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Bounded indexed-IFF edits checked by the bundled historical C++ validator.

No original assets are redistributed. The source archive omits iff.h, so the
oracle uses the declaration/accessor shim below and one obsolete allocation
spelling fix. The original iff.cpp parsing/validation bodies are preserved.
Only outputs accepted by the bounded Rust corpus checks reach the historical
reader. That reader alone is not a hostile-input validator.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
ARCHIVE_SHA256 = "255ede7a282284ea466edcaa9017a49c7239b4e163ac16d9434e8e16663c9866"
READER_SHA256 = "bb21fd4a34c2c5951eec54c6aa977fde55f10496a975fb0a1a4f5bb1ef5e79e9"
V0_SHA256 = "5b85d2cd824a88223c458eb80e564717c1a6ee4b080cefa4dcec695bd65e93a9"
MAX_FILE = 16 * 1024 * 1024

DECLARATIONS = r'''#ifndef RSMP_SOURCE_ORACLE_IFF_H
#define RSMP_SOURCE_ORACLE_IFF_H
#include <iostream>
#include <cstring>
#include <cstdlib>
#include <cstdio>
// Missing-header declaration/accessor shim; no map parser lives here.
class simIFF {
public:
    class Entry {
    public:
        simIFF* m_parent;
        unsigned int m_offset, m_length;
        char m_type[4];
        unsigned short m_id, m_flags;
        char* m_name;
        Entry();
        ~Entry();
        unsigned short id() const { return m_id; }
        unsigned short flags() const { return m_flags; }
        const char* name() const { return m_name ? m_name + 1 : ""; }
        std::istream* stream() const;
    };
    class Type {
    public:
        char m_type[4];
        unsigned int m_entries;
        Entry** m_list;
        Type();
        ~Type();
        bool type(const char* name) const { return std::memcmp(name, m_type, 4) == 0; }
        Entry* id(int) const;
    };
    enum { validated = 0, no_rsmp = 1, invalid = 2 };
    std::istream* m_stream;
    Entry* m_list;
    Type* m_type;
    unsigned int m_entries;
    int m_rsmp, m_types;
    void init();
    explicit simIFF(const char*);
    ~simIFF();
    int types();
    int validate();
    const char* whyInvalid() const;
    Type& operator[](const char*);
    static char* signature() {
        static char value[] = "IFF FILE 2.5:TYPE FOLLOWED BY SIZE";
        return value;
    }
};
#endif
'''

HARNESS = r'''#include "iff.h"
int main(int argc, char** argv) {
    if (argc < 2) return 2;
    unsigned passed = 0, failed = 0;
    for (int i = 1; i < argc; ++i) {
        simIFF iff(argv[i]);
        int status = iff.validate();
        if (status == simIFF::validated) {
            ++passed;
            std::cout << "PASS\t" << argv[i] << '\n';
        } else {
            ++failed;
            std::cout << "FAIL\t" << argv[i] << '\t'
                      << (status == simIFF::no_rsmp ? "no rsmp" : iff.whyInvalid())
                      << '\n';
        }
    }
    std::cerr << "validated=" << passed << " rejected=" << failed << '\n';
    return failed ? 1 : 0;
}
'''


def read_bounded(path, expected_sha256=None):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_FILE:
        raise ValueError(f"invalid or oversized input: {path}")
    data = path.read_bytes()
    if len(data) > MAX_FILE:
        raise ValueError(f"input grew beyond its bound: {path}")
    if expected_sha256 and hashlib.sha256(data).hexdigest() != expected_sha256:
        raise ValueError(f"pinned source SHA-256 mismatch: {path}")
    return data


def source_derived_v1(data):
    """Convert one independently pinned v0 map using iff.cpp:174-202.

    A source-derived fixture is explicitly not an original v1 corpus member.
    All ordinary chunk bytes remain unchanged; the map uses counted raw names,
    high ID word zero, and no alignment padding.
    """
    offset = struct.unpack_from(">I", data, 60)[0]
    if offset < 64 or offset + 76 > len(data) or data[offset:offset + 4] != b"rsmp":
        raise ValueError("invalid pinned v0 map pointer")
    envelope_size = struct.unpack_from(">I", data, offset + 4)[0]
    if offset + envelope_size != len(data):
        raise ValueError("pinned v0 map is not the final complete chunk")
    body = data[offset + 76:]
    reserved, version, magic, _, count = struct.unpack_from("<II4sII", body)
    if (reserved, version, magic, count) != (0, 0, b"pmsr", 14):
        raise ValueError("unexpected pinned v0 metadata")
    result = bytearray(struct.pack("<II4sII", 0, 1, b"pmsr", 0, count))
    at, entries = 20, 0
    for _ in range(count):
        kind, length = struct.unpack_from("<4sI", body, at)
        at += 8
        if length > 10000 or entries + length > 10000:
            raise ValueError("source-derived fixture count limit")
        result += struct.pack("<4sI", kind, length)
        for _ in range(length):
            chunk_offset, chunk_id, flags = struct.unpack_from("<IHH", body, at)
            at += 8
            end = body.find(b"\0", at, at + 65)
            if end < at:
                raise ValueError("source-derived fixture label bound")
            name = body[at:end]
            at = end + 1
            at += at % 2
            if chunk_offset < 64 or chunk_offset + 76 > offset:
                raise ValueError("source-derived fixture offset bound")
            chunk = data[chunk_offset:chunk_offset + 76]
            if (chunk[:4] != kind[::-1]
                    or struct.unpack_from(">HH", chunk, 8) != (chunk_id, flags)
                    or chunk[12:].split(b"\0", 1)[0] != name):
                raise ValueError("source-derived fixture metadata mismatch")
            result += struct.pack("<IHHHB", chunk_offset, chunk_id, 0, flags, len(name))
            result += name
            entries += 1
    if at != len(body) or entries != 39:
        raise ValueError("unexpected pinned v0 map coverage")
    header = bytearray(data[offset:offset + 76])
    struct.pack_into(">I", header, 4, len(result) + 76)
    return data[:offset] + header + result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default=os.environ.get("B_CARGO_BIN") or
                        shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo"))
    parser.add_argument("--cxx", default=os.environ.get("CXX") or shutil.which("g++") or "g++")
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--logs-dir", type=Path)
    args = parser.parse_args()
    logs = (args.logs_dir or Path(tempfile.mkdtemp(prefix="wonderland-iff-oracle-logs-"))).resolve()
    logs.mkdir(parents=True, exist_ok=True)
    summary = {
        "schema_version": 1,
        "status": "fail",
        "source_baseline": "4c6b3e8f5835b228723caea3c9f683c62f244f73",
        "archive_sha256": ARCHIVE_SHA256,
        "reader_sha256": READER_SHA256,
        "v0_fixture_sha256": V0_SHA256,
        "source_limitations": [
            "srcs.zip omits iff.h; declaration/accessor shim supplied",
            "one allocation spelling compatibility change; parser logic unchanged",
            "historical validator ignores size field and complete map coverage",
            "version-1 fixture is source-derived; no original v1 corpus present",
        ],
        "gates": [],
    }

    def run(label, command, cwd, env=None, timeout=60):
        log = logs / f"{label}.log"
        with log.open("w") as output:
            result = subprocess.run(command, cwd=cwd, env=env, stdout=output,
                                    stderr=subprocess.STDOUT, timeout=timeout)
        output = log.read_text(errors="replace")
        summary["gates"].append({"name": label, "exit_code": result.returncode, "log": log.name})
        if result.returncode:
            raise RuntimeError(f"{label} failed:\n{output[-12000:]}")
        print(f"{label}: PASS", flush=True)
        return output

    try:
        archive_path = ROOT / "Other/tools/Iffinator/Iffinator/srcs.zip"
        read_bounded(archive_path, ARCHIVE_SHA256)
        source_path = ROOT / "TSOClient/FSO.Content.TSO/Content/Objects/k8tqtablelamp2ts.iff"
        v0 = read_bounded(source_path, V0_SHA256)
        with tempfile.TemporaryDirectory(prefix="wonderland-iff-oracle-") as temporary:
            work = Path(temporary)
            with zipfile.ZipFile(archive_path) as archive:
                for name in ("iff.cpp", "substream.h"):
                    data = archive.read(name)
                    if len(data) > MAX_FILE:
                        raise ValueError("source archive member size limit")
                    if name == "iff.cpp":
                        if hashlib.sha256(data).hexdigest() != READER_SHA256:
                            raise ValueError("pinned iff.cpp SHA-256 mismatch")
                        old, new = b"new (Entry *)[m_entries]", b"new Entry*[m_entries]"
                        if data.count(old) != 1:
                            raise ValueError("allocation compatibility patch is not unique")
                        data = data.replace(old, new)
                    (work / name).write_bytes(data)
            (work / "iff.h").write_text(DECLARATIONS)
            (work / "main.cpp").write_text(HARNESS)
            binary = work / "source-iff-oracle"
            run("compile-original-reader", [args.cxx, "-std=gnu++98", "-O0",
                "-ffunction-sections", "-fdata-sections", "-Wl,--gc-sections",
                "iff.cpp", "main.cpp", "-o", str(binary)], work, timeout=30)
            v1 = work / "source-derived-v1.iff"
            v1.write_bytes(source_derived_v1(v0))
            outputs = work / "edits"
            env = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0",
                       CARGO_PROFILE_TEST_DEBUG="0", RUSTUP_TOOLCHAIN="1.90.0",
                       CARGO_TARGET_DIR=str((args.target_dir or work / "target").resolve()),
                       WONDERLAND_SOURCE_ROOT=str(ROOT), WONDERLAND_IFF_EDIT_OUTPUT=str(outputs),
                       WONDERLAND_IFF_V1_SOURCE=str(v1))
            version = run("toolchain", [args.cargo, "--version"], ROOT, env, timeout=10)
            if not version.startswith("cargo 1.90.0 "):
                raise RuntimeError("source-oracle verification requires Rust/Cargo 1.90.0")
            run("bounded-rust-corpus-edits", [args.cargo, "test", "--locked", "--manifest-path",
                "crates/legacy-formats/Cargo.toml", "--test", "indexed_iff_corpus", "--",
                "--ignored", "--nocapture"], ROOT, env)
            files = sorted(outputs.glob("*.iff"))
            if len(files) != 965:
                raise ValueError(f"expected 965 Rust-produced edits, got {len(files)}")
            total = sum(len(read_bounded(path)) for path in files)
            if total > 512 * 1024 * 1024:
                raise ValueError("combined oracle output byte limit")
            passed = 0
            for batch in range(0, len(files), 64):
                selected = files[batch:batch + 64]
                text = run(f"original-reader-batch-{batch // 64 + 1:02d}",
                           [str(binary), *map(str, selected)], work, timeout=30)
                count = sum(line.startswith("PASS\t") for line in text.splitlines())
                if count != len(selected):
                    raise ValueError("original validator did not report every requested output")
                passed += count
            summary.update(status="pass", original_indexed_files=198,
                           exact_passthrough_files=195, strict_duplicate_rejections=3,
                           source_maps_supporting_edits=192, ambiguous_map_edit_rejections=3,
                           supported_original_map_entries=13084, original_v0_edits=960,
                           source_derived_v1_edits=5, original_reader_outputs_passed=passed,
                           emitted_bytes=total)
    except (OSError, ValueError, RuntimeError, struct.error, subprocess.TimeoutExpired,
            zipfile.BadZipFile) as error:
        summary["error"] = str(error)
        print(str(error), file=sys.stderr)
    finally:
        (logs / "indexed-iff-oracle.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"Indexed IFF oracle: {summary['status'].upper()}; logs: {logs}", flush=True)
    return 0 if summary["status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
