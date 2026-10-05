#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Execute one authored fixture program natively and under Node WASI; compare all JSON."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def difference(expected, actual, path='$'):
    """Return the first differing field, including type/key/list length differences."""
    if type(expected) is not type(actual):
        return f'{path}: type {type(expected).__name__} != {type(actual).__name__}'
    if isinstance(expected, dict):
        for key in sorted(set(expected) | set(actual)):
            if key not in expected:
                return f'{path}.{key}: unexpected field'
            if key not in actual:
                return f'{path}.{key}: missing field'
            found = difference(expected[key], actual[key], f'{path}.{key}')
            if found:
                return found
    elif isinstance(expected, list):
        if len(expected) != len(actual):
            return f'{path}: list length {len(expected)} != {len(actual)}'
        for i, (a, b) in enumerate(zip(expected, actual)):
            found = difference(a, b, f'{path}[{i}]')
            if found:
                return found
    elif expected != actual:
        return f'{path}: {expected!r} != {actual!r}'
    return None


def compare(expected, actual):
    error = difference(expected, actual)
    if error:
        print(f'FAIL first differing field: {error}', file=sys.stderr)
        return 1
    return 0


def run(command, env=None):
    print('+ ' + ' '.join(map(str, command)), file=sys.stderr)
    completed = subprocess.run(command, cwd=ROOT, env=env, capture_output=True)
    if completed.stderr:
        sys.stderr.buffer.write(completed.stderr)
    if completed.returncode:
        raise RuntimeError(f'command exited {completed.returncode}: {command[0]}')
    return completed.stdout


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode() + b'\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target-dir', type=Path, default=ROOT.parent / 'parity-target')
    parser.add_argument('--compare', nargs=2, type=Path, metavar=('EXPECTED_JSON', 'ACTUAL_JSON'),
                        help='compare supplied results without building; nonzero at first difference')
    args = parser.parse_args()
    if args.compare:
        return compare(*(json.loads(p.read_bytes()) for p in args.compare))
    target = args.target_dir.resolve()
    cargo = os.environ.get('CARGO') or (str(Path.home() / '.cargo/bin/cargo') if (Path.home() / '.cargo/bin/cargo').is_file() else shutil.which('cargo'))
    if cargo is None:
        raise RuntimeError('Rust 1.90 cargo is required')
    node = shutil.which('node')
    if node is None:
        raise RuntimeError('Node with node:wasi is required')
    env = dict(os.environ, CARGO_INCREMENTAL='0')
    build = [cargo, 'build', '--locked', '--manifest-path', str(HERE / 'Cargo.toml'), '--target-dir', str(target)]
    run(build, env)
    run(build + ['--target', 'wasm32-wasip1'], env)
    native = json.loads(run([str(target / 'debug/wonderland-swarm-b-parity')]))
    wasm = json.loads(run([node, str(HERE / 'run-wasi.mjs'),
                          str(target / 'wasm32-wasip1/debug/wonderland-swarm-b-parity.wasm')]))
    target.mkdir(parents=True, exist_ok=True)
    native_path, wasm_path = target / 'parity-native.json', target / 'parity-wasi.json'
    native_path.write_bytes(canonical(native))
    wasm_path.write_bytes(canonical(wasm))
    if compare(native, wasm):
        return 1
    if native_path.read_bytes() != wasm_path.read_bytes():
        raise RuntimeError('canonical bytes differ after structural equality')
    # Prove the same CLI comparator rejects a deliberately altered result.
    changed = json.loads(canonical(native))
    changed['content']['bhav']['instructions'][0]['opcode'] = 0
    changed_path = target / 'parity-deliberately-changed.json'
    changed_path.write_bytes(canonical(changed))
    negative = subprocess.run([sys.executable, str(HERE / 'check.py'), '--compare',
                               str(native_path), str(changed_path)], capture_output=True, text=True)
    expected_field = '$.content.bhav.instructions[0].opcode'
    if negative.returncode != 1 or expected_field not in negative.stderr:
        raise RuntimeError(f'negative comparator check failed: {negative.returncode} {negative.stderr}')
    digest = hashlib.sha256(native_path.read_bytes()).hexdigest()
    print(f'PASS native/WASI full canonical JSON: {len(native_path.read_bytes())} bytes; sha256={digest}')
    print(f'PASS negative comparator: exit 1; {expected_field}; altered 4660 to 0')
    print(f'Native result: {native_path}\nWASI result: {wasm_path}')
    print('Scope: authored content/import/pack/manifest and isolated UI fixture execution; no full VM, replay, renderer, or browser execution claim.')
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (RuntimeError, OSError, ValueError) as error:
        print(f'FAIL: {error}', file=sys.stderr)
        sys.exit(1)
