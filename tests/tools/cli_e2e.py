"""Actual creator CLI regression with generated fixtures; no original assets required."""
import hashlib
import json
import os
import pathlib
import struct
import subprocess
import sys
import tempfile

binary = pathlib.Path(sys.argv[1]).resolve()

def digest(data):
    return hashlib.sha256(data).hexdigest()

def chunk(kind, ident, data):
    return kind + struct.pack('>IHH', len(data) + 76, ident, 0x1234) + bytes([0x7e])*64 + data

header = b'IFF FILE 2.5:TYPE FOLLOWED BY SIZE\x00 JAMIE DOORNBOS & MAXIS 1'
header = header.ljust(64, b'\0')
bhav = struct.pack('<HHBBHH', 0x8002, 2, 0, 1, 2, 3) + b'\x12\x34'
bhav += struct.pack('<HBB', 1, 1, 255) + bytes(range(1,9))
bhav += struct.pack('<HBB', 2, 254, 255) + bytes(range(8,0,-1)) + b'\xab'
original = header + chunk(b'BHAV',4096,bhav) + chunk(b'ZZZZ',9,b'\x00\x01\xff\x02\x03')

with tempfile.TemporaryDirectory(prefix='creator-cli-') as root:
    root = pathlib.Path(root)
    (root/'in.iff').write_bytes(original)
    def call(*args, success=True):
        proc = subprocess.run([str(binary), '--root', str(root), *args], capture_output=True, text=True, timeout=3)
        assert (proc.returncode == 0) == success, (args, proc.returncode, proc.stdout, proc.stderr)
        return proc
    call('import','in.iff','imported.iff')
    assert (root/'imported.iff').read_bytes() == original
    info = json.loads(call('inspect','imported.iff').stdout)
    assert info['source_sha256'] == digest(original)
    assert info['chunks'][0]['instructions'][0]['operand_hex'] == '0102030405060708'
    assert info['chunks'][1]['kind_hex'] == '5a5a5a5a'
    call('metadata','in.iff','metadata.json')
    assert json.loads((root/'metadata.json').read_text()) == info
    call('list','in.iff')
    call('validate','in.iff')
    call('edit','imported.iff','edited.iff','BHAV','4096',digest(original),'0x8002','bhav-branch','0','254','255')
    expected = bytearray(original); expected[64+76+14] = 254
    edited = (root/'edited.iff').read_bytes()
    assert edited == bytes(expected)
    reopened = json.loads(call('inspect','edited.iff').stdout)
    assert reopened['chunks'][0]['instructions'][0]['true'] == 254
    assert reopened['chunks'][1] == info['chunks'][1]
    call('export','edited.iff','reopened.iff')
    assert (root/'reopened.iff').read_bytes() == edited
    call('extract','edited.iff','ZZZZ','9','unknown.bin')
    assert (root/'unknown.bin').read_bytes() == b'\x00\x01\xff\x02\x03'
    (root/'sentinel.iff').write_bytes(b'unchanged')
    for sha,version,index,branch in [(digest(original),'0x8002','0','2'),('0'*64,'0x8002','0','254'),(digest(original),'0x8003','0','254'),(digest(original),'0x8002','99','254')]:
        call('edit','in.iff','sentinel.iff','BHAV','4096',sha,version,'bhav-branch',index,branch,'255',success=False)
        assert (root/'sentinel.iff').read_bytes() == b'unchanged'
    call('import','in.iff','../escape.iff',success=False)
    (root/'link').symlink_to(root/'in.iff')
    call('inspect','link',success=False)
    call('import','in.iff','link',success=False)
    assert (root/'in.iff').read_bytes() == original
    os.mkfifo(root/'pipe')
    call('inspect','pipe',success=False)
    call('import','in.iff','pipe',success=False)
    assert (root/'pipe').is_fifo()
    assert not list(root.glob('.creator-*.tmp'))
    assert json.loads(call('debug-capabilities').stdout)['live_step'] is False
    assert 'unsupported' in call('debug-step',success=False).stderr
    inventory=json.loads(call('inventory').stdout)
    assert inventory['debug_provider']['provider_installed'] is False
    # External OTF tuning edits must preserve unknown XML, quote style, whitespace and comments.
    otf=b'<object extra="keep"><!-- preserve --><T i="3" n="table"><K i="7" l="key" v="-20" extension="x"/></T><other a="unchanged"/></object>'
    (root/'tuning.otf').write_bytes(otf)
    otf_info=json.loads(call('otf-inspect','tuning.otf').stdout)
    assert otf_info['tables'][0]['keys'][0]['value'] == -20
    call('otf-edit','tuning.otf','edited.otf',digest(otf),'3','7','-99')
    assert (root/'edited.otf').read_bytes()==otf.replace(b'v="-20"',b'v="-99"')
    (root/'existing.otf').write_bytes(b'unchanged')
    call('otf-edit','tuning.otf','existing.otf',digest(otf),'3','999','4',success=False)
    assert (root/'existing.otf').read_bytes()==b'unchanged'
    duplicate=otf.replace(b'</T>',b'<K i="7" l="duplicate" v="1"/></T>')
    (root/'duplicate.otf').write_bytes(duplicate)
    call('otf-edit','duplicate.otf','existing.otf',digest(duplicate),'3','7','4',success=False)
    assert (root/'existing.otf').read_bytes()==b'unchanged'
    # FAR1a container lists and extracts a generated entry to the explicit safe output path.
    archive=b'FAR!byAZ'+struct.pack('<II',1,19)+b'abc'
    archive+=struct.pack('<IIIII',1,3,3,16,4)+b'test'
    (root/'test.far').write_bytes(archive)
    call('container-list','far1a','test.far')
    call('container-extract','far1a','test.far','0','entry.bin')
    assert (root/'entry.bin').read_bytes()==b'abc'
    # True city dimensions, row padding, and RGB values; terrain starts with grass.
    pixels=bytes([0,255,0])*(512*512)
    bmp_header=struct.pack('<2sIHHI',b'BM',54+len(pixels),0,0,54)
    bmp_header+=struct.pack('<IiiHHIIiiII',40,512,512,1,24,0,len(pixels),0,0,0,0)
    bmp=bmp_header+pixels
    (root/'city.bmp').write_bytes(bmp)
    call('city-validate','city.bmp','terrain')
    call('city-edit','city.bmp','city-edited.bmp',digest(bmp),'terrain','1','2','255','0','0')
    city_expected=bytearray(bmp);p=54+(511-2)*512*3+1*3;city_expected[p:p+3]=b'\0\0\xff'
    assert (root/'city-edited.bmp').read_bytes()==city_expected
    call('city-export-ppm','city-edited.bmp','city.ppm')
    assert (root/'city.ppm').read_bytes().startswith(b'P6\n512 512\n255\n')
print('CLI end-to-end: import → inspect → guarded edit → export → reopen; atomic failures, unknown retention, traversal/symlink refusal, OTF XML exactness, container extraction, city pixel exactness and unsupported live stepping PASS')
