"""Real CLI transactions: literal envelopes, atomic failures, strict guards/schema."""
import copy
import hashlib
import json
import pathlib
import struct
import subprocess
import sys
import tempfile

binary = pathlib.Path(sys.argv[1]).resolve()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def chunk(kind, ident, payload, flags=0x1234, label=bytes([0x7e]) * 64):
    return kind + struct.pack('>IHH', len(payload) + 76, ident, flags) + label + payload


def key(kind, ident):
    return {'kind_hex': kind.hex(), 'id': ident}


def expected(payload, version):
    return {'resource_sha256': digest(payload), 'format_version': version}


header = b'IFF FILE 2.5:TYPE FOLLOWED BY SIZE\x00 JAMIE DOORNBOS & MAXIS 1'.ljust(64, b'\0')
bhav = bytes.fromhex('0280010000010200030012340100feff0102030405060708ab')
opaque = b'\x00\x01\xff\x02\x03'
bcon = bytes.fromhex('02a107000800cc')
original = header + chunk(b'BHAV', 4096, bhav) + chunk(b'ZZZZ', 9, opaque)
label = b'Exact label\0'.ljust(64, b'\xa3')

with tempfile.TemporaryDirectory(prefix='creator-transactions-') as root:
    root = pathlib.Path(root)
    (root / 'in.iff').write_bytes(original)
    (root / 'bcon.bin').write_bytes(bcon)

    def call(*args, success=True):
        result = subprocess.run([str(binary), '--root', str(root), *args],
                                capture_output=True, text=True, timeout=5)
        assert result.returncode == (0 if success else 2), (
            args, result.returncode, result.stdout, result.stderr)
        return result

    # Single structural commands preserve all unrelated envelope bytes.
    call('add', 'in.iff', 'added.iff', 'BCON', '12', digest(original), '0xabcd',
         label.hex(), 'bcon.bin')
    added = original + chunk(b'BCON', 12, bcon, 0xabcd, label)
    assert (root / 'added.iff').read_bytes() == added
    call('set-metadata', 'added.iff', 'renamed.iff', 'BCON', '12', digest(added),
         digest(bcon), 'none', 'BCON', '13', '0x5678', bytes(64).hex())
    renamed = original + chunk(b'BCON', 13, bcon, 0x5678, bytes(64))
    assert (root / 'renamed.iff').read_bytes() == renamed
    call('remove', 'renamed.iff', 'removed.iff', 'BCON', '13', digest(renamed),
         digest(bcon), 'none')
    assert (root / 'removed.iff').read_bytes() == original

    # All three operations use the same source snapshot, regardless of order.
    spec = {
        'schema_version': 1,
        'source_sha256': digest(original),
        'operations': [
            {'op': 'edit', 'key': key(b'BHAV', 4096), 'expected': expected(bhav, 0x8002),
             'edit': {'op': 'bhav-operand', 'instruction': 0, 'operand_hex': '0807060504030201'}},
            {'op': 'remove', 'key': key(b'ZZZZ', 9), 'expected': expected(opaque, None)},
            {'op': 'add', 'key': key(b'BCON', 12), 'flags': 0xabcd,
             'label_hex': label.hex(), 'payload_file': 'bcon.bin', 'payload_sha256': digest(bcon)},
        ],
    }

    def transaction(value, output='out.iff', source='in.iff', success=True):
        (root / 'transaction.json').write_text(json.dumps(value))
        return call('transaction', source, output, 'transaction.json', success=success)

    transaction(spec)
    changed_bhav = bytearray(bhav)
    changed_bhav[16:24] = bytes.fromhex('0807060504030201')
    changed = header + chunk(b'BHAV', 4096, bytes(changed_bhav)) + chunk(b'BCON', 12, bcon, 0xabcd, label)
    assert (root / 'out.iff').read_bytes() == changed
    info = json.loads(call('inspect', 'out.iff').stdout)
    assert info['source_sha256'] == digest(changed)
    assert [c['kind'] for c in info['chunks']] == ['BHAV', 'BCON']
    call('validate', 'out.iff')
    call('export', 'out.iff', 'roundtrip.iff')
    assert (root / 'roundtrip.iff').read_bytes() == changed
    reordered = json.loads(json.dumps(spec), object_pairs_hook=lambda pairs: dict(reversed(pairs)))
    transaction(reordered, output='reordered.iff')
    assert (root / 'reordered.iff').read_bytes() == changed

    # Replay against updated bytes must fail, including same-file publication.
    transaction(spec, output='out.iff', source='out.iff', success=False)
    assert (root / 'out.iff').read_bytes() == changed

    def rejected(value):
        (root / 'sentinel.iff').write_bytes(b'unchanged existing output')
        transaction(value, output='sentinel.iff', success=False)
        assert (root / 'sentinel.iff').read_bytes() == b'unchanged existing output'
        assert (root / 'in.iff').read_bytes() == original
        assert not list(root.glob('.creator-*.tmp'))

    # A late invalid operation cannot publish earlier valid edits.
    bad = copy.deepcopy(spec)
    bad['operations'][2]['payload_file'] = 'malformed.bin'
    (root / 'malformed.bin').write_bytes(b'\x02')
    bad['operations'][2]['payload_sha256'] = digest(b'\x02')
    rejected(bad)
    for field, value in [('source_sha256', '0' * 64), ('schema_version', 2)]:
        bad = copy.deepcopy(spec)
        bad[field] = value
        rejected(bad)
    for field, value in [('resource_sha256', '0' * 64), ('format_version', 0x8003)]:
        bad = copy.deepcopy(spec)
        bad['operations'][0]['expected'][field] = value
        rejected(bad)

    # Repeated writes, duplicate additions and collisions are all explicit errors.
    bad = copy.deepcopy(spec)
    bad['operations'].append(copy.deepcopy(bad['operations'][0]))
    rejected(bad)
    bad = copy.deepcopy(spec)
    bad['operations'].append(copy.deepcopy(bad['operations'][2]))
    rejected(bad)
    bad = copy.deepcopy(spec)
    bad['operations'][2]['key'] = key(b'BHAV', 4096)
    rejected(bad)
    metadata = {'op': 'metadata', 'key': key(b'ZZZZ', 9), 'expected': expected(opaque, None),
                'new_key': key(b'BHAV', 4096), 'flags': 0, 'label_hex': bytes(64).hex()}
    rejected({'schema_version': 1, 'source_sha256': digest(original), 'operations': [metadata]})

    # Unknown fields, missing explicit version, wrong JSON types and raw map writes fail.
    # Serde's positional struct form is not part of the JSON object schema.
    rejected([spec['schema_version'], spec['source_sha256'], spec['operations']])
    for position in ['key', 'expected']:
        bad = copy.deepcopy(spec)
        bad['operations'][1][position] = list(bad['operations'][1][position].values())
        rejected(bad)
    array_metadata = copy.deepcopy(metadata)
    array_metadata['new_key'] = ['5a5a5a5a', 10]
    rejected({'schema_version': 1, 'source_sha256': digest(original), 'operations': [array_metadata]})
    for mutate in [
        lambda s: s.update(unexpected=True),
        lambda s: s['operations'][0].update(extra='ignored?'),
        lambda s: s['operations'][0]['key'].update(extra=1),
        lambda s: s['operations'][0]['expected'].update(extra=1),
        lambda s: s['operations'][0]['edit'].update(extra=1),
        lambda s: s['operations'][1]['expected'].pop('format_version'),
        lambda s: s['operations'][2].update(flags='0'),
        lambda s: s['operations'][2].update(flags=65536),
        lambda s: s['operations'][2].update(flags=None),
        lambda s: s['operations'][2].update(expected=expected(bhav, 0x8002)),
        lambda s: s['operations'][2].update(expected=None),
        lambda s: s['operations'][2].update(edit=None),
        lambda s: s['operations'][2].update(new_key=None),
        lambda s: s['operations'][0]['edit'].update(index=0),
        lambda s: s['operations'][0]['edit'].update(value=None),
        lambda s: s['operations'][0].update(edit=[]),
        lambda s: s['operations'].__setitem__(0, list(s['operations'][0].values())),
        lambda s: s['operations'][2].update(label_hex='00'),
        lambda s: s['operations'][2].update(payload_sha256='0' * 64),
        lambda s: s['operations'][2]['key'].update(kind_hex='72736d70'),
        lambda s: s['operations'][2].update(payload_file='../escape.bin'),
        lambda s: s['operations'][2].update(payload_file='payload-link'),
    ]:
        bad = copy.deepcopy(spec)
        mutate(bad)
        if not (root / 'payload-link').exists():
            (root / 'payload-link').symlink_to(root / 'bcon.bin')
        rejected(bad)
    for encoded in [
        json.dumps(spec)[:-1] + ',"schema_version":1}',
        json.dumps(spec).replace('"id": 4096', '"id": 4096, "id": 4096', 1),
        json.dumps(spec).replace('"format_version": null', '"format_version": null, "format_version": null', 1),
        json.dumps(spec).replace('"op": "edit"', '"op": "edit", "op": "edit"', 1),
        json.dumps(spec).replace('"instruction": 0', '"instruction": 0, "instruction": 0', 1),
        json.dumps(spec) + '{}',
    ]:
        (root / 'transaction.json').write_text(encoded)
        call('transaction', 'in.iff', 'sentinel.iff', 'transaction.json', success=False)
        assert (root / 'sentinel.iff').read_bytes() == b'unchanged existing output'
    (root / 'transaction.json').write_bytes(b' ' * (1024 * 1024 + 1))
    call('transaction', 'in.iff', 'sentinel.iff', 'transaction.json', success=False)
    assert (root / 'sentinel.iff').read_bytes() == b'unchanged existing output'

    # Same-key metadata changes retain opaque payloads and arbitrary label bytes.
    metadata['new_key'] = key(b'ZZZZ', 10)
    metadata['flags'] = 0xfedc
    metadata['label_hex'] = label.hex()
    transaction({'schema_version': 1, 'source_sha256': digest(original), 'operations': [metadata]},
                output='metadata.iff')
    assert (root / 'metadata.iff').read_bytes() == (
        header + chunk(b'BHAV', 4096, bhav) + chunk(b'ZZZZ', 10, opaque, 0xfedc, label))

    # The shared JSON value field retains distinct string and integer contracts.
    tuning_spec = {'schema_version': 1, 'source_sha256': digest(added), 'operations': [
        {'op': 'edit', 'key': key(b'BCON', 12), 'expected': expected(bcon, None),
         'edit': {'op': 'tuning', 'index': 1, 'value': 65000}}
    ]}
    transaction(tuning_spec, source='added.iff', output='tuning-transaction.iff')
    assert (root / 'tuning-transaction.iff').read_bytes() == (
        original + chunk(b'BCON', 12, bytes.fromhex('02a10700e8fdcc'), 0xabcd, label))
    strings = bytes.fromhex('fdff020001610063000262006400a3a3')
    string_source = header + chunk(b'STR#', 1, strings)
    (root / 'strings.iff').write_bytes(string_source)
    string_spec = {'schema_version': 1, 'source_sha256': digest(string_source), 'operations': [
        {'op': 'edit', 'key': key(b'STR#', 1), 'expected': expected(strings, 0xfffd),
         'edit': {'op': 'string', 'set': 1, 'index': 0, 'value': 'é'}}
    ]}
    transaction(string_spec, source='strings.iff', output='string-transaction.iff')
    assert (root / 'string-transaction.iff').read_bytes() == (
        header + chunk(b'STR#', 1, bytes.fromhex('fdff0200016100630002e9006400a3a3')))
    for source, value_spec, wrong_values in [
        ('added.iff', tuning_spec, ['65000', 1.5, None, [], 65536]),
        ('strings.iff', string_spec, [65000, None, []]),
    ]:
        for wrong in wrong_values:
            bad = copy.deepcopy(value_spec)
            bad['operations'][0]['edit']['value'] = wrong
            transaction(bad, source=source, output='sentinel.iff', success=False)
            assert (root / 'sentinel.iff').read_bytes() == b'unchanged existing output'

    # Indexed IFF authoring delegates offsets/counts/flags/names to the real writer.
    indexed_header = bytearray(header)
    indexed_header[60:64] = struct.pack('>I', 142)
    indexed_label = b'a\0'.ljust(64, b'\0')
    resource_map = bytes(8) + b'pmsr' + struct.pack('<II', 0, 1)
    resource_map += b'ATAD' + struct.pack('<IIHH', 1, 64, 1, 0x10) + b'a\0'
    indexed = (bytes(indexed_header) + chunk(b'DATA', 1, b'\x91\xfe', 0x10, indexed_label)
               + chunk(b'rsmp', 0, resource_map, 0x10, bytes(64)))
    (root / 'indexed.iff').write_bytes(indexed)
    call('add', 'indexed.iff', 'indexed-added.iff', 'BCON', '12', digest(indexed), '0xabcd',
         label.hex(), 'bcon.bin')
    indexed_added = (root / 'indexed-added.iff').read_bytes()
    assert indexed_added[:142] == indexed[:142]
    map_size = struct.unpack_from('>I', indexed_added, 146)[0]
    assert struct.unpack_from('<I', indexed_added, 142 + 76 + 16)[0] == 2
    # New type BCON's map entry points beyond the expanded map.
    assert indexed_added[142 + 76 + 38:142 + 76 + 42] == b'NOCB'
    assert struct.unpack_from('<I', indexed_added, 142 + 76 + 46)[0] == 142 + map_size
    call('remove', 'indexed-added.iff', 'indexed-removed.iff', 'BCON', '12', digest(indexed_added),
         digest(bcon), 'none')
    assert (root / 'indexed-removed.iff').read_bytes() == indexed
    call('set-metadata', 'indexed.iff', 'indexed-renamed.iff', 'DATA', '1', digest(indexed),
         digest(b'\x91\xfe'), 'none', 'DATA', '2', '3', bytes(64).hex())
    indexed_renamed = (root / 'indexed-renamed.iff').read_bytes()
    assert struct.unpack_from('<H', indexed_renamed, 142 + 76 + 32)[0] == 2
    assert struct.unpack_from('<H', indexed_renamed, 142 + 76 + 34)[0] == 3
    # Unsupported or inconsistent source maps pass through exactly and reject edits.
    for relative_offset, replacement in [(4, struct.pack('<I', 99)), (28, struct.pack('<I', 65))]:
        broken = bytearray(indexed)
        broken[142 + 76 + relative_offset:142 + 76 + relative_offset + 4] = replacement
        (root / 'map-invalid.iff').write_bytes(broken)
        call('export', 'map-invalid.iff', 'map-pass.iff')
        assert (root / 'map-pass.iff').read_bytes() == broken
        call('remove', 'map-invalid.iff', 'sentinel.iff', 'DATA', '1', digest(broken),
             digest(b'\x91\xfe'), 'none', success=False)
        assert (root / 'sentinel.iff').read_bytes() == b'unchanged existing output'

    # Typed palette edits change one exact RGB triple and retain version/reserved bytes.
    palette = struct.pack('<II', 1, 2) + bytes([0xa3]) * 8 + bytes([1, 2, 3, 9, 8, 7])
    palette_source = header + chunk(b'PALT', 6, palette) + chunk(b'ZZZZ', 9, opaque)
    (root / 'palette.iff').write_bytes(palette_source)
    call('edit', 'palette.iff', 'palette-edited.iff', 'PALT', '6', digest(palette_source),
         '1', 'palette', '1', '40', '50', '60')
    palette_expected = bytearray(palette_source)
    palette_expected[64 + 76 + 19:64 + 76 + 22] = bytes([40, 50, 60])
    assert (root / 'palette-edited.iff').read_bytes() == palette_expected
    palette_info = json.loads(call('inspect', 'palette-edited.iff').stdout)
    assert palette_info['chunks'][0]['format_version'] == 1
    assert palette_info['chunks'][0]['colors_rgb'] == [[1, 2, 3], [40, 50, 60]]
    palette_spec = {'schema_version': 1, 'source_sha256': digest(palette_source), 'operations': [
        {'op': 'edit', 'key': key(b'PALT', 6), 'expected': expected(palette, 1),
         'edit': {'op': 'palette', 'index': 1, 'rgb': [40, 50, 60]}}
    ]}
    transaction(palette_spec, output='palette-transaction.iff', source='palette.iff')
    assert (root / 'palette-transaction.iff').read_bytes() == palette_expected
    for args in [('1', '2', '40', '50', '60'), ('0', '1', '40', '50', '60'),
                 ('1', '1', '256', '50', '60')]:
        call('edit', 'palette.iff', 'sentinel.iff', 'PALT', '6', digest(palette_source),
             args[0], 'palette', *args[1:], success=False)
        assert (root / 'sentinel.iff').read_bytes() == b'unchanged existing output'
    call('edit', 'palette.iff', 'sentinel.iff', 'PALT', '6', digest(palette_source),
         '1', 'unknown', 'bcon.bin', success=False)
    assert (root / 'sentinel.iff').read_bytes() == b'unchanged existing output'

print('CLI transactions PASS: add/remove/metadata, one-snapshot multi-edit, exact bytes, replay, late failure, strict schema, workspace boundaries, rebuilt indexed maps, typed palette RGB')
