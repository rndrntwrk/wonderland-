#!/usr/bin/env python3
"""Read-only declared-source census and observed-boundary drift gate.

This does not evaluate MSBuild, execute test references, prove ABI equivalence,
freeze a multiplayer protocol, authenticate a server, or qualify a release.
Python 3.11+ standard library only. Never executes commands from a manifest.
"""
from __future__ import annotations

import argparse
import configparser
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import xml.etree.ElementTree as ET
from typing import Any

MAX_FILE_BYTES = 4 * 1024 * 1024
MAX_TOTAL_BYTES = 256 * 1024 * 1024
MAX_ITEMS = 50_000
SOURCE_COMMIT = '4c6b3e8f5835b228723caea3c9f683c62f244f73'
SOLUTION = 'TSOClient/FreeSO.sln'
SOURCE_MANIFEST = 'docs/compat/baseline.json'
BOUNDARY_MANIFEST = 'docs/contracts/boundary-map.json'
# These slots are the minimum mapping, not the full game's capability census.
REQUIRED_BOUNDARIES = {
    'ui-preview': 'presentation', 'native-identity': 'native',
    'native-accepted-tick': 'native', 'native-checkpoint': 'native',
    'native-effects': 'native', 'native-replay': 'native', 'native-wire': 'native',
    'native-player-admission': 'native', 'native-visual-projection': 'native-presentation',
    'legacy-vmnet': 'legacy', 'legacy-gateway': 'legacy', 'content-identity': 'content',
}
REALMS = {'presentation', 'native', 'native-presentation', 'legacy', 'content'}
EVIDENCE_KINDS = {
    'fixture-test-not-executed', 'extracted-reference-not-executed',
    'workflow-not-executed',
}
PROJECT_LINE = re.compile(
    r'^Project\("\{([0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12})\}"\) = "([^"]+)", "([^"]+)", '
    r'"\{([0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12})\}"\s*$'
)
FOLDER_GUID = '2150E333-8FDC-42A3-9474-1A3956D46DE8'


class BaselineError(ValueError):
    """Bounded input, declaration or evidence-contract failure."""


def require(value: bool, message: str) -> None:
    if not value:
        raise BaselineError(message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def encoded(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=True, sort_keys=True, indent=2,
                       allow_nan=False) + '\n').encode('utf-8')


def canonical_path(value: str, parent: str = '') -> str:
    require(isinstance(value, str) and 0 < len(value) <= 4096, 'invalid path')
    require(not any(ord(c) < 32 or ord(c) == 127 for c in value), 'control in path')
    value = value.replace('\\', '/')
    require(not value.startswith('/') and ':' not in value, 'absolute/network path')
    parts = list(PurePosixPath(parent).parts) if parent else []
    for part in value.split('/'):
        if part in ('', '.'):
            continue
        if part == '..':
            require(bool(parts), 'path escapes repository')
            parts.pop()
        else:
            parts.append(part)
    require(bool(parts), 'empty path')
    return '/'.join(parts)


def manifest_path(value: str) -> str:
    path = canonical_path(value)
    require(path == value, 'manifest path must be normalized: ' + str(value))
    return path


class Root:
    """Reads an immutable checkout, without following repository symlinks.

    This is not a defense against another process racing filesystem changes.
    CI uses an isolated checkout. No submodule payload is executed or inspected.
    """
    def __init__(self, path: Path):
        self.path = path.resolve(strict=True)
        require(self.path.is_dir(), 'root must be a directory')
        self.total = 0
        self.fingerprints: dict[str, dict[str, Any]] = {}

    def checked(self, name: str) -> Path:
        name = manifest_path(name)
        p = self.path
        for part in name.split('/'):
            p = p / part
            require(part != '.git', 'Git metadata is not a source input')
            require(not p.is_symlink(), 'symlink is not an admitted source: ' + name)
        return p

    def read(self, name: str, *, optional: bool = False) -> bytes | None:
        p = self.checked(name)
        try:
            info = p.stat()
        except FileNotFoundError:
            if optional:
                return None
            raise BaselineError('missing input: ' + name) from None
        require(stat.S_ISREG(info.st_mode), 'not a regular file: ' + name)
        require(info.st_size <= MAX_FILE_BYTES, 'source file byte limit: ' + name)
        with p.open('rb') as f:
            data = f.read(MAX_FILE_BYTES + 1)
        require(len(data) <= MAX_FILE_BYTES, 'source grew past byte limit: ' + name)
        self.total += len(data)
        require(self.total <= MAX_TOTAL_BYTES, 'aggregate input byte limit')
        return data

    def fingerprint(self, name: str) -> dict[str, Any]:
        if name not in self.fingerprints:
            data = self.read(name, optional=True)
            self.fingerprints[name] = ({'state': 'missing', 'sha256': None}
                                      if data is None else
                                      {'state': 'present', 'sha256': sha256(data)})
        return dict(self.fingerprints[name])


def strict_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in out, 'duplicate JSON key: ' + key)
        out[key] = value
    return out


def reject_number(_: str) -> Any:
    # Evidence schema uses exact integer counts and textual hashes/IDs only.
    # Reject floats as well as NaN/Infinity, including overflowing exponents.
    raise BaselineError('noninteger JSON number')


def read_json(root: Path, name: str) -> Any:
    data = Root(root).read(name)
    try:
        return json.loads(data, object_pairs_hook=strict_pairs,
                          parse_constant=reject_number, parse_float=reject_number)
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise BaselineError('invalid bounded JSON: ' + name) from error


def text(data: bytes, name: str) -> str:
    try:
        result = data.decode('utf-8-sig')
    except UnicodeError as error:
        raise BaselineError('expected UTF-8 declaration: ' + name) from error
    require('\x00' not in result, 'NUL in declaration: ' + name)
    return result


def submodules(root: Root) -> list[dict[str, str]]:
    data = root.read('.gitmodules')
    cfg = configparser.ConfigParser(interpolation=None, strict=True)
    try:
        cfg.read_string(text(data, '.gitmodules'))
        rows = []
        for section in cfg.sections():
            require(section.startswith('submodule "') and section.endswith('"'), 'unknown gitmodules section')
            path = manifest_path(cfg.get(section, 'path'))
            rows.append({'path': path, 'url': cfg.get(section, 'url'),
                         'scope': 'declaration-only-no-payload-or-rights-approval'})
        require(len({r['path'] for r in rows}) == len(rows), 'duplicate submodule path')
        return sorted(rows, key=lambda r: r['path'])
    except configparser.Error as error:
        raise BaselineError('invalid .gitmodules') from error


def under_submodule(name: str, modules: list[dict[str, str]]) -> bool:
    return any(name == m['path'] or name.startswith(m['path'] + '/') for m in modules)


def local_name(tag: str) -> str:
    return tag.rsplit('}', 1)[-1]


def project_census(root: Root, path: str, modules: list[dict[str, str]]) -> dict[str, Any]:
    if under_submodule(path, modules):
        return {'state': 'submodule-not-inspected', 'sha256': None, 'compile': [],
                'imports': [], 'references': [], 'implicit_compile': 'not-inspected',
                'default_compile_properties': []}
    data = root.read(path, optional=True)
    if data is None:
        return {'state': 'missing', 'sha256': None, 'compile': [], 'imports': [],
                'references': [], 'implicit_compile': 'not-inspected',
                'default_compile_properties': []}
    xml = text(data, path)
    require('<!DOCTYPE' not in xml.upper() and '<!ENTITY' not in xml.upper(), 'DTD/entities forbidden')
    try:
        tree = ET.fromstring(xml)
    except ET.ParseError as error:
        raise BaselineError('invalid project XML: ' + path) from error
    require(local_name(tree.tag) == 'Project', 'wrong XML root: ' + path)
    result: dict[str, Any] = {
        'state': 'present', 'sha256': sha256(data), 'compile': [], 'imports': [],
        'references': [], 'implicit_compile': ('unevaluated-sdk-defaults'
                                              if tree.get('Sdk') else 'not-msbuild-evaluated'),
        'default_compile_properties': [],
    }
    parent = str(PurePosixPath(path).parent)
    stack = [(tree, [], 0)]
    items = 0
    while stack:
        node, inherited, depth = stack.pop()
        items += 1
        require(depth <= 128 and items <= MAX_ITEMS, 'project XML depth/item limit')
        conditions = inherited + ([node.attrib['Condition']] if 'Condition' in node.attrib else [])
        tag = local_name(node.tag)
        if tag in ('Compile', 'ProjectReference', 'Import'):
            entry: dict[str, Any] = {'attributes': dict(sorted(node.attrib.items())),
                                     'conditions': conditions}
            if tag == 'Import':
                # Imports can add arbitrary source; do not execute or infer them.
                result['imports'].append(entry)
            else:
                include = node.get('Include')
                entry['link'] = next((n.text for n in node if local_name(n.tag) == 'Link'), None)
                if include is None or any(c in include for c in '*?$@%;') or node.get('Exclude'):
                    entry.update(target=None, state='unevaluated', sha256=None)
                else:
                    target = canonical_path(include, parent)
                    entry['target'] = target
                    if under_submodule(target, modules):
                        entry.update(state='submodule-not-inspected', sha256=None)
                    else:
                        entry.update(root.fingerprint(target))
                result['compile' if tag == 'Compile' else 'references'].append(entry)
        elif tag in ('EnableDefaultCompileItems', 'EnableDefaultItems'):
            result['default_compile_properties'].append({'property': tag, 'value': node.text,
                                                         'conditions': conditions})
        stack.extend((child, conditions, depth + 1) for child in reversed(list(node)))
    return result


def collect_source(path: Path) -> dict[str, Any]:
    root = Root(path)
    modules = submodules(root)
    raw = root.read(SOLUTION)
    projects = []
    folders = []
    ids: set[str] = set()
    paths: set[str] = set()
    for line in text(raw, SOLUTION).splitlines():
        line = line.strip()
        if not re.match(r'^Project(?:\s|\()', line):
            continue
        match = PROJECT_LINE.fullmatch(line)
        require(match is not None, 'unparsed solution project declaration')
        kind, name, include, identity = match.groups()
        identity = identity.upper()
        require(identity not in ids, 'duplicate solution project identity')
        ids.add(identity)
        require(len(ids) <= 512, 'solution project limit')
        if kind.upper() == FOLDER_GUID:
            folders.append({'name': name, 'identity': identity})
            continue
        target = canonical_path(include, str(PurePosixPath(SOLUTION).parent))
        require(target not in paths, 'duplicate solution project path')
        paths.add(target)
        require(target.endswith('.csproj'), 'unhandled non-C# solution project: ' + target)
        row = {'name': name, 'path': target, 'identity': identity}
        row.update(project_census(root, target, modules))
        projects.append(row)
        require(sum(len(p['compile']) + len(p['references']) + len(p['imports'])
                    for p in projects) <= MAX_ITEMS, 'aggregate declaration limit')
    require(bool(projects), 'empty declared solution')
    projects.sort(key=lambda r: r['path'])
    compile_rows = [r for p in projects for r in p['compile']]
    counts = {
        'declared_projects': len(projects),
        'present_projects': sum(p['state'] == 'present' for p in projects),
        'missing_projects': sum(p['state'] == 'missing' for p in projects),
        'submodule_projects': sum(p['state'] == 'submodule-not-inspected' for p in projects),
        'sdk_projects_unevaluated': sum(p['implicit_compile'] == 'unevaluated-sdk-defaults' for p in projects),
        'compile_declarations': len(compile_rows),
        'present_compile': sum(r['state'] == 'present' for r in compile_rows),
        'missing_compile': sum(r['state'] == 'missing' for r in compile_rows),
        'unevaluated_compile': sum(r['state'] == 'unevaluated' for r in compile_rows),
        'submodule_compile': sum(r['state'] == 'submodule-not-inspected' for r in compile_rows),
        'unique_present_compile_paths': len({r['target'] for r in compile_rows if r['state'] == 'present'}),
    }
    return {'schema': 1, 'scope': 'declared-solution-not-msbuild-evaluated',
            'solution': {'path': SOLUTION, 'sha256': sha256(raw)},
            'gitmodules_sha256': sha256(root.read('.gitmodules')), 'submodules': modules,
            'solution_folders': sorted(folders, key=lambda r: r['identity']),
            'counts': counts, 'projects': projects}


def freeze_source(census: dict[str, Any]) -> dict[str, Any]:
    """Compact tracked pins; the detailed per-file census is an output artifact."""
    return {
        'scope': census['scope'], 'solution': census['solution'],
        'gitmodules_sha256': census['gitmodules_sha256'], 'submodules': census['submodules'],
        'counts': census['counts'], 'census_sha256': sha256(encoded(census)),
        'projects': [
            {'path': p['path'], 'name': p['name'], 'state': p['state'], 'sha256': p['sha256'],
             'compile_declarations': len(p['compile']), 'compile_sha256': sha256(encoded(p['compile'])),
             'implicit_compile': p['implicit_compile']}
            for p in census['projects']
        ],
    }


def first_difference(left: Any, right: Any, at: str = '$') -> str | None:
    if type(left) is not type(right):
        return at
    if isinstance(left, dict):
        for key in sorted(set(left) | set(right)):
            if key not in left or key not in right:
                return at + '.' + key
            difference = first_difference(left[key], right[key], at + '.' + key)
            if difference:
                return difference
        return None
    if isinstance(left, list):
        for index, (a, b) in enumerate(zip(left, right)):
            difference = first_difference(a, b, f'{at}[{index}]')
            if difference:
                return difference
        return None if len(left) == len(right) else at + '.length'
    return None if left == right else at


def record(value: Any, keys: set[str], name: str) -> dict[str, Any]:
    require(isinstance(value, dict) and set(value) == keys, 'unexpected fields in ' + name)
    return value


def nonempty(value: Any, name: str) -> str:
    require(isinstance(value, str) and 0 < len(value.strip()) <= 16_384, 'invalid ' + name)
    return value


def fingerprint(value: Any, digits: int = 64) -> str:
    require(isinstance(value, str) and re.fullmatch('[0-9a-f]{'+str(digits)+'}', value) is not None,
            'invalid source digest')
    return value


def sequence(value: Any, name: str, maximum: int = 512) -> list[Any]:
    require(isinstance(value, list) and len(value) <= maximum, 'invalid list: ' + name)
    return value


def source_realm(path: str) -> str | None:
    if path.startswith('crates/contracts/'):
        return 'presentation'
    if path in ('crates/game-runtime/src/avatar_projection.rs', 'crates/game-runtime/src/world_projection.rs'):
        return 'native-presentation'
    if path.startswith(('crates/sim-core/', 'crates/game-runtime/')):
        return 'native'
    if path.startswith(('crates/vm-protocol/', 'crates/game-services/', 'services/browser-gateway/')):
        return 'legacy'
    if path.startswith(('crates/content-ir/', 'crates/legacy-formats/', 'crates/content-runtime-bridge/')):
        return 'content'
    return None


def check_boundary(path: Path, spec: dict[str, Any]) -> list[str]:
    record(spec, {'id', 'realm', 'sources', 'consumers', 'evidence', 'status', 'owner', 'notes'}, 'boundary')
    nonempty(spec['id'], 'boundary id')
    require(spec['realm'] in REALMS, 'unknown boundary realm')
    require(spec['status'] == 'observed-not-frozen', 'source inspection is not acceptance or an ABI freeze')
    require(spec['owner'] in {'A', 'B', 'C', 'D', 'E', 'F'}, 'invalid boundary owner')
    nonempty(spec['notes'], 'boundary notes')
    root = Root(path)
    pins = sequence(spec['sources'], 'sources')
    require(bool(pins), 'empty boundary sources')
    seen = set()
    issues = []
    for pin in pins:
        record(pin, {'path', 'sha256'}, 'source pin')
        name = manifest_path(pin['path'])
        require(name not in seen, 'duplicate source pin: ' + name)
        seen.add(name)
        declared = source_realm(name)
        require(declared is None or declared == spec['realm'], 'wrong realm for ' + name)
        expected = fingerprint(pin['sha256'])
        actual = root.fingerprint(name)
        if actual['sha256'] != expected:
            issues.append(spec['id'] + ': source changed/missing: ' + name)
    for name in sequence(spec['consumers'], 'consumers'):
        if root.fingerprint(manifest_path(name))['state'] != 'present':
            issues.append(spec['id'] + ': consumer missing: ' + name)
    for evidence in sequence(spec['evidence'], 'evidence'):
        record(evidence, {'path', 'kind'}, 'evidence')
        require(evidence['kind'] in EVIDENCE_KINDS, 'inspection cannot promote evidence to execution/equivalence')
        name = manifest_path(evidence['path'])
        if root.fingerprint(name)['state'] != 'present':
            issues.append(spec['id'] + ': evidence source missing: ' + name)
    return issues


def audit(root: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    baseline = read_json(root, SOURCE_MANIFEST)
    boundaries = read_json(root, BOUNDARY_MANIFEST)
    record(baseline, {'schema', 'source_commit', 'inspected_commit', 'source', 'dispositions', 'gitlinks', 'limits'}, 'baseline')
    record(boundaries, {'schema', 'inspected_commit', 'boundaries', 'open_decisions'}, 'boundary map')
    require(type(baseline['schema']) is int and baseline['schema'] == 1, 'unsupported baseline schema')
    require(type(boundaries['schema']) is int and boundaries['schema'] == 1, 'unsupported boundary schema')
    require(baseline['source_commit'] == SOURCE_COMMIT, 'wrong original source baseline')
    fingerprint(baseline['inspected_commit'], 40)
    require(boundaries['inspected_commit'] == baseline['inspected_commit'], 'mixed inspected revisions')
    nonempty(baseline['limits'], 'scope limitations')
    for note in sequence(boundaries['open_decisions'], 'open decisions'):
        nonempty(note, 'decision')
    require(bool(boundaries['open_decisions']), 'unresolved native/production decisions cannot be erased silently')
    census = collect_source(root)
    observed = freeze_source(census)
    issues = []
    record(baseline['source'], set(observed), 'frozen source')
    frozen_rows = sequence(baseline['source']['projects'], 'frozen projects')
    require(all(isinstance(p, dict) and isinstance(p.get('path'), str) for p in frozen_rows), 'invalid frozen project')
    old = {manifest_path(p['path']): p for p in frozen_rows}
    require(len(old) == len(frozen_rows), 'duplicate frozen project')
    current = {p['path']: p for p in observed['projects']}
    for name in sorted(set(old) | set(current)):
        difference = first_difference(old.get(name), current.get(name))
        if difference:
            issues.append('declared source project drift: ' + name + ' at ' + difference)
    if not issues:
        difference = first_difference(baseline['source'], observed)
        if difference:
            issues.append('declared source drift at ' + difference)
    gitlinks = baseline['gitlinks']
    require(isinstance(gitlinks, dict) and set(gitlinks) == {m['path'] for m in census['submodules']},
            'gitlink/declaration coverage mismatch')
    for name, commit in gitlinks.items():
        manifest_path(name)
        if commit is not None:
            fingerprint(commit, 40)
    expected_paths = {p['path'] for p in observed['projects']}
    dispositions = baseline['dispositions']
    require(isinstance(dispositions, dict), 'dispositions must be keyed by source project path')
    if set(dispositions) != expected_paths:
        issues.append('project disposition coverage mismatch: missing=' + ','.join(sorted(expected_paths-set(dispositions))) +
                      '; extra=' + ','.join(sorted(set(dispositions)-expected_paths)))
    for name, disposition in dispositions.items():
        manifest_path(name)
        record(disposition, {'owner', 'destination', 'acceptance'}, 'disposition')
        require(disposition['owner'] in {'A','B','C','D','E','F'}, 'invalid project owner')
        nonempty(disposition['destination'], 'destination')
        require(disposition['acceptance'] == 'not-assessed', 'project census alone cannot qualify a capability')
    specs = sequence(boundaries['boundaries'], 'boundaries', 128)
    ids = [s.get('id') if isinstance(s, dict) else None for s in specs]
    require(all(isinstance(i, str) for i in ids), 'invalid boundary IDs')
    require(len(set(ids)) == len(ids), 'duplicate boundary ID')
    require(set(ids) == set(REQUIRED_BOUNDARIES), 'required boundary map coverage mismatch')
    for spec in specs:
        require(spec.get('realm') == REQUIRED_BOUNDARIES[spec['id']], 'required boundary realm mismatch')
        issues.extend(check_boundary(root, spec))
    inputs = Root(root)
    report = {
        'schema': 1, 'gate': 'swarm-f-declaration-and-boundary-drift', 'passed': not issues,
        'original_equivalence': 'not-tested', 'release_qualification': 'not-tested',
        'observed_baseline_commit': baseline['inspected_commit'],
        'manifest_sha256': {p: sha256(inputs.read(p)) for p in (SOURCE_MANIFEST, BOUNDARY_MANIFEST)},
        'source_counts': census['counts'], 'census_sha256': observed['census_sha256'],
        'boundary_count': len(specs), 'source_pins': sum(len(s['sources']) for s in specs),
        'issues': issues,
    }
    return report, census


def git_identity(root: Path) -> dict[str, str]:
    def git(*args: str) -> str:
        try:
            return subprocess.check_output(['git', '-C', str(root), *args], text=True,
                                           stderr=subprocess.PIPE, timeout=10).strip()
        except (OSError, subprocess.SubprocessError) as error:
            raise BaselineError('cannot establish checkout identity') from error
    require(Path(git('rev-parse', '--show-toplevel')).resolve() == root.resolve(), 'root is not repository top level')
    require(not git('status', '--porcelain', '--untracked-files=all'), 'tracked checkout is dirty')
    return {'commit': fingerprint(git('rev-parse', 'HEAD'), 40),
            'tree': fingerprint(git('rev-parse', 'HEAD^{tree}'), 40)}



def verify_git_inputs(root: Path, census: dict[str, Any]) -> dict[str, Any]:
    """Bind every inspected regular input and declared gitlink to HEAD.

    This catches ignored/untracked files and skip-worktree changes that a plain
    git diff alone can miss. No fetch, checkout, submodule update or write occurs.
    """
    try:
        data = subprocess.check_output(['git', '-C', str(root), 'ls-tree', '-rz', '--full-tree', 'HEAD'],
                                       stderr=subprocess.PIPE, timeout=15)
    except (OSError, subprocess.SubprocessError) as error:
        raise BaselineError('cannot inspect tracked source tree') from error
    require(len(data) <= 16 * 1024 * 1024, 'Git tree metadata limit')
    entries = {}
    for row in data.split(b'\0'):
        if row:
            metadata, name = row.split(b'\t', 1)
            mode, kind, digest = metadata.decode('ascii').split(' ')
            entries[name.decode('utf-8')] = (mode, kind, digest)
    baseline = read_json(root, SOURCE_MANIFEST)
    boundaries = read_json(root, BOUNDARY_MANIFEST)
    observed_links = {}
    for path, expected in baseline['gitlinks'].items():
        entry = entries.get(path)
        actual = entry[2] if entry and entry[:2] == ('160000', 'commit') else None
        require(actual == expected and (entry is None or actual is not None), 'gitlink changed/type mismatch: ' + path)
        require(not (root/path).is_symlink(), 'symlink at declared submodule')
        observed_links[path] = actual
    inputs = {SOLUTION, '.gitmodules', SOURCE_MANIFEST, BOUNDARY_MANIFEST, 'tools/replay/baseline.py'}
    for project in census['projects']:
        if project['state'] == 'present':
            inputs.add(project['path'])
        for item in project['compile'] + project['references']:
            if item['state'] == 'present':
                inputs.add(item['target'])
    for spec in boundaries['boundaries']:
        inputs.update(pin['path'] for pin in spec['sources'])
        inputs.update(spec['consumers'])
        inputs.update(item['path'] for item in spec['evidence'])
    reader = Root(root)
    for path in sorted(inputs):
        entry = entries.get(path)
        require(entry is not None and entry[0] in ('100644', '100755') and entry[1] == 'blob',
                'input is not a tracked regular file: ' + path)
        content = reader.read(path)
        digest = hashlib.sha1(b'blob ' + str(len(content)).encode('ascii') + b'\0' + content).hexdigest()
        require(digest == entry[2], 'input differs from committed bytes: ' + path)
    return {'regular_inputs_matched': len(inputs), 'gitlinks': observed_links}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--capture-source', action='store_true', help='print a candidate source snapshot; never edit the baseline')
    parser.add_argument('--output', type=Path, help='new evidence directory; existing directories are not overwritten')
    parser.add_argument('--require-git', action='store_true', help='require an identified clean tracked checkout')
    args = parser.parse_args()
    try:
        if args.capture_source:
            require(args.output is None and not args.require_git, 'capture does not run the acceptance gate')
            sys.stdout.buffer.write(encoded(freeze_source(collect_source(args.root))))
            return 0
        identity = git_identity(args.root) if args.require_git else None
        report, census = audit(args.root)
        if identity is not None:
            require(Path(__file__).resolve() == (args.root/'tools/replay/baseline.py').resolve(),
                    'executed checker is not in the identified checkout')
            report['git_verification'] = verify_git_inputs(args.root, census)
        report['tested_checkout'] = identity
        report['execution_scope'] = 'clean-tracked-checkout' if identity else 'unattested-input-copy'
        if args.output:
            # Exclusive directory creation prevents accidental artifact replacement.
            args.output.mkdir(mode=0o700, parents=False, exist_ok=False)
            (args.output/'source-census.json').write_bytes(encoded(census))
            report['census_artifact_sha256'] = sha256((args.output/'source-census.json').read_bytes())
            (args.output/'report.json').write_bytes(encoded(report))
        sys.stdout.buffer.write(encoded(report))
        return 0 if report['passed'] else 1
    except (BaselineError, OSError, TypeError, KeyError, RecursionError) as error:
        sys.stdout.buffer.write(encoded({'schema': 1, 'passed': False, 'error': str(error),
                                        'original_equivalence': 'not-tested', 'release_qualification': 'not-tested'}))
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
