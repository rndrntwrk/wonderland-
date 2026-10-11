#!/usr/bin/env python3
"""Execute the pinned existing SQLx tests. Never publish from this CI runner."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
from claim_evidence import UNIT_TESTS, SQL_TESTS, database_identity, disposable_url, parse_tests, validate_command

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = 'crates/persistence/Cargo.toml'


def file_digest(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def fingerprint() -> dict[str, str]:
    files = subprocess.check_output([
        'git', 'ls-files', '-z', '--', 'crates/persistence', 'tools/swarm-e',
        'tests/swarm_e', '.github/workflows/swarm-e-claims.yml'], cwd=ROOT).split(b'\0')
    result = {}
    for name in filter(None, files):
        relative = name.decode('utf-8')
        path = ROOT / relative
        if path.is_symlink() or not path.is_file():
            raise ValueError('Build inputs must be ordinary tracked files')
        result[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    if 'crates/persistence/Cargo.lock' not in result or 'tools/swarm-e/run_claims.py' not in result:
        raise ValueError('A complete tracked source checkout is required')
    return result


def execute(name: str, command: list[str], output: Path, records: list[dict], timeout: int = 600) -> str:
    log = output / f'{name}.log'
    start = time.monotonic()
    timed_out = False
    log_limited = False
    with log.open('xb') as stream:
        process = subprocess.Popen(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        while process.poll() is None:
            timed_out = time.monotonic() - start >= timeout
            log_limited = log.stat().st_size > 4 * 1024 * 1024
            if timed_out or log_limited:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
                break
            time.sleep(0.05)
        log_limited = log_limited or log.stat().st_size > 4 * 1024 * 1024
        code = 124 if timed_out else 125 if log_limited else process.returncode
    record = {'name': name, 'command': command, 'exit_code': code, 'timed_out': timed_out,
              'log_limited': log_limited, 'duration_seconds': round(time.monotonic() - start, 3),
              'log_sha256': file_digest(log)}
    records.append(record)
    validate_command(record, name)
    if log.stat().st_size > 4 * 1024 * 1024:
        raise ValueError('Command log exceeded the evidence bound')
    return log.read_text(encoding='utf-8')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine', choices=['mysql', 'mariadb'], required=True)
    parser.add_argument('--output', type=Path, required=True, help='New evidence directory; must not exist')
    parser.add_argument('--server-identity', type=Path, required=True)
    parser.add_argument('--image-identity', type=Path, required=True)
    args = parser.parse_args()
    report = {'schema': 'wonderland-claims-db-evidence-v1', 'passed': False, 'commands': []}
    try:
        args.output.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        print('Refusing to reuse an evidence directory', file=sys.stderr)
        return 2
    try:
        database = disposable_url(os.environ.get('DATABASE_URL'), os.environ.get('WONDERLAND_CLAIMS_DISPOSABLE'))
        report['database'] = database_identity(args.engine, args.server_identity.read_text(),
                                                json.loads(args.image_identity.read_text()), database)
        subprocess.run(['git', 'diff', '--exit-code', 'HEAD', '--'], cwd=ROOT,
                       check=True, stdout=subprocess.DEVNULL)
        before = fingerprint()
        report['commit'] = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
        report['tree'] = subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=ROOT, text=True).strip()
        report['inputs_before'] = before
        records = report['commands']
        report['compiler'] = execute('compiler', ['rustc', '+1.99.0', '-Vv'], args.output, records)
        execute('format', ['cargo', '+1.99.0', 'fmt', '--manifest-path', MANIFEST, '--', '--check'], args.output, records)
        common = ['cargo', '+1.99.0', 'test', '--manifest-path', MANIFEST, '--locked']
        suffix = ['--', '--test-threads=1', '--color', 'never']
        report['unit_tests'] = parse_tests(execute('unit', common + ['--lib'] + suffix,
                                                   args.output, records), UNIT_TESTS)
        report['sql_tests'] = parse_tests(execute('database', common + ['--test', 'claims_mysql'] + suffix,
                                                  args.output, records), SQL_TESTS)
        execute('clippy', ['cargo', '+1.99.0', 'clippy', '--manifest-path', MANIFEST,
                          '--all-targets', '--locked', '--', '-D', 'warnings'], args.output, records)
        after = fingerprint()
        report['inputs_after'] = after
        if before != after:
            raise ValueError('Source or resolved dependencies changed during verification')
        subprocess.run(['git', 'diff', '--exit-code', 'HEAD', '--'], cwd=ROOT, check=True)
        report['passed'] = True
    except (ValueError, OSError, UnicodeError, subprocess.SubprocessError) as error:
        # Never include DATABASE_URL in diagnostics. The runner only permits
        # disposable local services, and all commands omit credentials.
        report['failure'] = f'{type(error).__name__}: {error}'
        print('Claims verification failed; see the retained report and command logs', file=sys.stderr)
    finally:
        (args.output / 'report.json').write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
