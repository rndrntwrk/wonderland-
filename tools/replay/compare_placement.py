"""Strict ground-placement observation comparison; not full VM or content parity."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re

MAX_BYTES = 1024 * 1024
FIXTURE_HEADER = 'case\tx\ty\tflags\twall_flags\theights\tallow_intersection\texpected_status'
CASE_NAMES = ('ground','self','adjacent','overlap','collision','floor_required',
    'terrain_only_on_floor','normal_floor','water_denied','water_allowed',
    'water_required_dry','water_required','pool_denied','pool_allowed',
    'pool_required_dry','pool_required','pool_required_on_water','water_required_on_pool',
    'height_denied','wall_required','diagonal_required','overlap_allowed','out_left','out_right')
EXPECTED_STATUS = (-1,-1,-1,10,10,19,18,-1,28,-1,29,-1,28,-1,30,-1,30,28,23,5,22,-1,0,0)
FIELDS = ('case','ts1','facing','query_status','query_blocker','commit_status','commit_blocker',
    'query_unchanged','after_x','after_y','after_level','after_facing','blocker_x','blocker_y',
    'blocker_level','blocker_facing','entities','recover_status','recover_x','recover_y',
    'retry_status','retry_blocker','final_x','final_y','final_facing')
MODES = (1,0)
FACINGS = (0,2,4,6)
NUMBER = re.compile(r'(?:0|[1-9][0-9]*|-[1-9][0-9]*)\Z')


def _lines(data: bytes, maximum: int = MAX_BYTES) -> list[str]:
    if not isinstance(data, bytes) or not 0 < len(data) <= maximum:
        raise ValueError('placement input byte limit')
    try:
        text = data.decode('ascii')
    except UnicodeError as error:
        raise ValueError('placement input requires ASCII, without BOM') from error
    if '\r' in text or not text.endswith('\n') or text.endswith('\n\n'):
        raise ValueError('one final LF required on every placement record')
    return text[:-1].split('\n')


def _number(cell: str, low: int, high: int) -> int:
    if len(cell) > 21 or not NUMBER.fullmatch(cell):
        raise ValueError('noncanonical placement integer')
    value = int(cell)
    if not low <= value <= high:
        raise ValueError('placement integer range')
    return value


def read_cases(data: bytes) -> list[list[str | int]]:
    lines = _lines(data, 16384)
    if lines[0] != FIXTURE_HEADER or len(lines) != len(CASE_NAMES)+1:
        raise ValueError('incomplete placement case contract')
    cases = []
    limits = [(-32768,32767),(-32768,32767),(0,16383),(0,4095),(0,32767),(0,1),(-1,50)]
    for name,status,line in zip(CASE_NAMES,EXPECTED_STATUS,lines[1:]):
        cells=line.split('\t')
        if len(cells)!=8 or cells[0]!=name:
            raise ValueError('unexpected or reordered placement case')
        row=[name]+[_number(c,*limit) for c,limit in zip(cells[1:],limits)]
        if row[-1]!=status:
            raise ValueError('required source error-code coverage changed')
        cases.append(row)
    return cases


def parse_trace(cases: bytes, data: bytes) -> list[list[str | int]]:
    specification=read_cases(cases)
    lines=_lines(data)
    if lines[0] != '\t'.join(FIELDS) or len(lines) != 1+len(specification)*8:
        raise ValueError('incomplete placement trace schema or row count')
    rows=[]
    ranges={'ts1':(0,1),'facing':(0,7),'query_status':(-1,50),'commit_status':(-1,50),
            'recover_status':(-1,50),'retry_status':(-1,50),'query_unchanged':(0,1),
            'query_blocker':(0,32767),'commit_blocker':(0,32767),'retry_blocker':(0,32767),
            'entities':(0,32767)}
    for index,line in enumerate(lines[1:]):
        cells=line.split('\t')
        if len(cells)!=len(FIELDS):
            raise ValueError('placement trace column count')
        row=[cells[0]]
        for name,cell in zip(FIELDS[1:],cells[1:]):
            limit=ranges.get(name,(0,7) if name.endswith('facing') else ((1,5) if name.endswith('level') else (-32768,32767)))
            row.append(_number(cell,*limit))
        key=[specification[index//8][0],MODES[(index//4)%2],FACINGS[index%4]]
        if row[:3]!=key:
            raise ValueError(f'placement key missing/duplicated/reordered at row {index+1}')
        rows.append(row)
    return rows


def _reference_contract(cases: bytes, rows: list[list[str | int]]) -> None:
    specification=read_cases(cases)
    for index,row in enumerate(rows):
        name,x,y,flags,wall,height,allow,status=specification[index//8]
        r=dict(zip(FIELDS,row))
        blocker=2 if status==10 else 0
        target=[x,y,1,r['facing']] if status==-1 else [40,40,1,0]
        checks=(
            [r['query_status'],r['commit_status']]==[status,status],
            [r['query_blocker'],r['commit_blocker']]==[blocker,blocker],
            r['query_unchanged']==1,
            [r['after_x'],r['after_y'],r['after_level'],r['after_facing']]==target,
            [r['blocker_x'],r['blocker_y'],r['blocker_level'],r['blocker_facing']]==[88,56,1,0],
            r['entities']==2,
            [r['recover_status'],r['recover_x'],r['recover_y']]==[-1,40,72],
            [r['retry_status'],r['retry_blocker'],r['final_x'],r['final_y'],r['final_facing']]==[10,2,40,72,0],
        )
        if not all(checks):
            raise ValueError(f'original placement invariant not observed: {name}/{r["ts1"]}/{r["facing"]}')


def compare(cases: bytes, reference: bytes, candidate: bytes) -> dict:
    left=parse_trace(cases,reference)
    right=parse_trace(cases,candidate)
    _reference_contract(cases,left)
    result={'schema':1,'scope':'ground-placement-observations-v1','passed':True,
            'full_vm_state_parity':False,'original_content_installation':False,
            'rows':len(left),'fields':list(FIELDS),'first_difference':None,
            'cases_sha256':hashlib.sha256(cases).hexdigest(),
            'reference_sha256':hashlib.sha256(reference).hexdigest(),
            'candidate_sha256':hashlib.sha256(candidate).hexdigest()}
    for a,b in zip(left,right):
        for field,expected,actual in zip(FIELDS,a,b):
            if expected!=actual:
                result['passed']=False
                result['first_difference']={'case':a[0],'ts1':a[1],'facing':a[2],
                    'field':field,'expected':expected,'actual':actual}
                return result
    return result


def read_bounded(path: Path) -> bytes:
    if path.is_symlink() or not path.is_file():
        raise ValueError('placement input must be a regular file')
    with path.open('rb') as handle:
        data=handle.read(MAX_BYTES+1)
    if len(data)>MAX_BYTES:
        raise ValueError('placement input exceeds byte limit')
    return data


def main() -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('cases',type=Path);parser.add_argument('reference',type=Path);parser.add_argument('candidate',type=Path)
    args=parser.parse_args()
    try:
        result=compare(read_bounded(args.cases),read_bounded(args.reference),read_bounded(args.candidate))
    except (OSError,ValueError) as error:
        print(json.dumps({'schema':1,'passed':False,'error':str(error)}));return 2
    print(json.dumps(result,indent=2));return 0 if result['passed'] else 1
if __name__=='__main__':raise SystemExit(main())
