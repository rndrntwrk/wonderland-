#!/usr/bin/env python3
"""Compare exact component traces. Supplied files alone do not attest execution.

No commands, plugins, evaluator, network, unsafe deserialization or automatic
baseline updates. Python 3.11+ standard library. The accompanying run_reference
runner binds real executions to source, inputs and executable hashes.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re
import stat
import sys

MAX_BYTES = 4 * 1024 * 1024
CASE_COLUMNS = 'case ts1 seed utc_start start_tick fraction minutes hours day month year fire steps'.split()
COLUMNS = 'case step clock_tick rng_before bound random branch extra rng_after fraction minutes hours day month year fire seconds utc'.split()
BOUNDS = (0, 1, 2, 100, 65535, (1 << 64)-1)
DATETIME_MAX = 3155378975999999999

class TraceError(ValueError):
    pass

def require(value, message):
    if not value:
        raise TraceError(message)

def digest(data):
    return hashlib.sha256(data).hexdigest()

def load(path, maximum=MAX_BYTES):
    path = Path(path).absolute()
    for component in (path, *path.parents):
        require(not component.is_symlink(), 'symlink input is not permitted')
    info = path.stat()
    require(stat.S_ISREG(info.st_mode) and 0 < info.st_size <= maximum, 'input type/size limit')
    with path.open('rb') as file:
        data = file.read(maximum+1)
    require(0 < len(data) <= maximum, 'input changed beyond byte limit')
    return data

def table(data, columns, max_rows):
    require(isinstance(data, bytes) and 0 < len(data) <= MAX_BYTES, 'input byte limit')
    require(data.endswith(b'\n'), 'missing final newline')
    require(all(value in (9,10) or 32 <= value <= 126 for value in data), 'noncanonical text/control bytes')
    lines = data.decode('ascii').splitlines()
    require(lines[0] == '\t'.join(columns), 'unsupported header/field order')
    require(1 < len(lines) <= max_rows+1, 'empty or oversized record set')
    rows = []
    for line in lines[1:]:
        values = line.split('\t')
        require(len(values) == len(columns), 'wrong field count')
        require(all(0 < len(value) <= 64 for value in values), 'invalid field length')
        rows.append(dict(zip(columns, values)))
    return rows

def integer(value, low, high, field):
    require(isinstance(value,str) and len(value)<=21 and re.fullmatch(r'-?(0|[1-9][0-9]*)',value) is not None and value!='-0', 'noncanonical integer: '+field)
    number = int(value)
    require(low <= number <= high, 'integer range: '+field)
    return number

def parse_cases(data):
    rows = table(data, CASE_COLUMNS, 64)
    result = []
    ids = set()
    for row in rows:
        name=row['case']
        require(re.fullmatch(r'[a-z][a-z0-9_-]{0,63}',name) is not None and name not in ids,'invalid/duplicate case identity')
        ids.add(name)
        limits = {
            'ts1':(0,1),'seed':(0,(1<<64)-1),'utc_start':(0,DATETIME_MAX-10**12),
            'start_tick':(0,1_000_000),'fraction':(0,149),'minutes':(0,59),
            'hours':(0,23),'day':(1,30),'month':(1,12),'year':(1,2_000_000_000),
            'fire':(-(1<<31),(1<<31)-1),'steps':(1,2048),
        }
        values = {key:integer(row[key],*limits[key],key) for key in CASE_COLUMNS[1:]}
        require(values['fraction'] < (30 if values['ts1'] else 150), 'invalid clock fraction for case mode')
        result.append({'case':name,**values})
    require(sum(case['steps'] for case in result)<=10000,'total step limit')
    return result

def parse_trace(data,cases):
    rows=table(data,COLUMNS,10000)
    require(len(rows)==sum(case['steps'] for case in cases),'trace does not cover complete declared scenarios')
    cursor=0
    for case in cases:
        previous_seed=case['seed']
        for step in range(1,case['steps']+1):
            row=rows[cursor];cursor+=1
            require(row['case']==case['case'],'out-of-order or undeclared trace case')
            require(integer(row['step'],1,2048,'step')==step,'out-of-order/duplicate/missing trace step')
            expected_tick=case['start_tick']+step
            require(integer(row['clock_tick'],0,(1<<63)-1,'clock_tick')==expected_tick,'clock tick does not match declared input')
            values={key:integer(row[key],0,(1<<64)-1,key) for key in ('rng_before','bound','random','extra','rng_after')}
            require(values['rng_before']==previous_seed,'rng_before does not continue the previous state')
            previous_seed=values['rng_after']
            bound=BOUNDS[(step-1)%len(BOUNDS)]
            require(values['bound']==bound,'random bound differs from declared command schedule')
            require(values['random'] < bound if bound else values['random']==0,'impossible bounded random value')
            require(values['extra']<17,'extra random value outside harness bound')
            integer(row['branch'],0,1,'branch')
            integer(row['fraction'],0,(30 if case['ts1'] else 150)-1,'fraction')
            for field,low,high in [('minutes',0,59),('hours',0,23),('day',1,30),('month',1,12),
                                   ('year',1,(1<<31)-1),('fire',-(1<<31),(1<<31)-1),('seconds',0,59),('utc',0,DATETIME_MAX)]:
                integer(row[field],low,high,field)
    return rows

def compare(cases_data,reference_data,candidate_data):
    cases=parse_cases(cases_data)
    reference=parse_trace(reference_data,cases)
    candidate=parse_trace(candidate_data,cases)
    difference=None
    compared=0
    for left,right in zip(reference,candidate):
        for field in COLUMNS[2:]:
            compared+=1
            if left[field]!=right[field]:
                difference={'case':left['case'],'step':int(left['step']),'clock_tick':left['clock_tick'],
                            'field':field,'reference':left[field],'candidate':right[field]}
                break
        if difference:
            break
    return {'schema':1,'passed':difference is None,'evidence_scope':'supplied-traces-not-execution-attestation',
            'full_original_engine':'not-tested','records':len(reference),'cases':len(cases),
            'compared_fields':compared,'first_difference':difference,'input_sha256':digest(cases_data),
            'reference_sha256':digest(reference_data),'candidate_sha256':digest(candidate_data)}

def encode(value):
    return (json.dumps(value,sort_keys=True,indent=2,allow_nan=False)+'\n').encode()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cases',type=Path,required=True)
    parser.add_argument('--reference',type=Path,required=True)
    parser.add_argument('--candidate',type=Path,required=True)
    parser.add_argument('--output',type=Path)
    args=parser.parse_args()
    code=2
    try:
        report=compare(load(args.cases,65536),load(args.reference),load(args.candidate))
        code=0 if report['passed'] else 1
    except (OSError,TraceError,ValueError) as error:
        report={'schema':1,'passed':False,'error':str(error),'full_original_engine':'not-tested',
                'evidence_scope':'supplied-traces-not-execution-attestation'}
    if args.output:
        try:
            with args.output.open('xb') as file:
                file.write(encode(report))
        except OSError as error:
            report={'schema':1,'passed':False,'error':'evidence output unavailable: '+str(error),
                    'full_original_engine':'not-tested','evidence_scope':'supplied-traces-not-execution-attestation'}
            code=2
    sys.stdout.buffer.write(encode(report))
    return code

if __name__=='__main__':
    raise SystemExit(main())
