"""Run real original placement against shipping native/WASM world code on Windows.

Requires #44's isolated original build; no product or manifest is rewritten.
Only text observations, logs, and identities are retained in the output directory.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib
from compare_placement import compare, parse_trace, read_cases

CASES_HASH = '602716fdff80c06454707fbe4ed4d2194f0c4dacf292cfb786ba5655d7a33f13'

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def require(condition: bool, message: str) -> None:
    if not condition: raise ValueError(message)

def run(root: Path, output: Path) -> int:
    root=root.resolve(strict=True)
    output.mkdir(parents=True,exist_ok=False); output=output.resolve()
    build=output.parent/'placement-build';build.mkdir(exist_ok=False)
    record={'schema':1,'passed':False,'scope':'ground-placement-observations-v1',
        'original_content_installation':False,'full_vm_state_parity':False,
        'production_build_or_payment':False,'rendered_browser_test':False,
        'commands':[],'comparisons':{},'fault_controls':[],'executables':{}}
    env=dict(os.environ,CARGO_TARGET_DIR=str(build/'cargo'),PYTHONDONTWRITEBYTECODE='1',
             WONDERLAND_PLACEMENT_TEST_ONLY='1')
    def command(label,args,timeout=120,expected=0,enabled=True):
        args=list(map(str,args));entry={'label':label,'argv':args,'expected_exit':expected}
        record['commands'].append(entry)
        stdout,stderr=output/(label+'.stdout'),output/(label+'.stderr')
        child_env=env.copy()
        if not enabled:child_env.pop('WONDERLAND_PLACEMENT_TEST_ONLY',None)
        with stdout.open('xb') as out,stderr.open('xb') as err:
            try: result=subprocess.run(args,cwd=root,env=child_env,stdout=out,stderr=err,timeout=timeout,check=False)
            except subprocess.TimeoutExpired:
                entry['timeout']=timeout;raise
        entry.update(exit=result.returncode,stdout_sha256=sha(stdout),stderr_sha256=sha(stderr))
        require(stdout.stat().st_size<=16*1024*1024 and stderr.stat().st_size<=16*1024*1024,label+': output limit')
        if result.returncode!=expected:
            print(stdout.read_text(errors='replace')[-8000:]);print(stderr.read_text(errors='replace')[-8000:])
            raise ValueError(f'{label}: exit {result.returncode}, expected {expected}')
        print(f'{label}: exit {result.returncode}',flush=True)
        return stdout,stderr
    def compare_paths(label,left,right):
        result=compare(cases,left.read_bytes(),right.read_bytes())
        (output/(label+'.json')).write_text(json.dumps(result,indent=2)+'\n')
        record['comparisons'][label]=result
        require(result['passed'],label+': '+str(result['first_difference']))
        print(f'{label}: {result["rows"]} records matched',flush=True)
    try:
        require(os.name=='nt','original .NET Framework placement requires Windows')
        paths=[root/p for p in ['tools/reference-csharp/placement-differential.cs',
            'tools/reference-csharp/vm-bootstrap.cs','tools/replay/placement_probe.rs',
            'tools/replay/compare_placement.py','tools/replay/run_placement.py',
            'tools/replay/run_placement_wasm.mjs','tests/parity/test_placement_compare.py',
            'fixtures/reference/placement-cases.tsv','Cargo.toml','Cargo.lock','rust-toolchain.toml']]
        inputs={str(p.relative_to(root)).replace('\\','/'):sha(p) for p in paths};record['inputs']=inputs
        cases=(root/'fixtures/reference/placement-cases.tsv').read_bytes()
        require(hashlib.sha256(cases).hexdigest()==CASES_HASH,'unreviewed placement fixture')
        require(len(read_cases(cases))==24,'placement case coverage')
        command('placement-comparator-tests',[sys.executable,'-B','-S',root/'tests/parity/test_placement_compare.py'])
        binary=root/'TSOClient/tso.simantics/bin/Release'
        refs=root.parent/'framework/Microsoft.NETFramework.ReferenceAssemblies.net45.1.0.3/build/.NETFramework/v4.5'
        vswhere=Path(os.environ['ProgramFiles(x86)'])/'Microsoft Visual Studio/Installer/vswhere.exe'
        found,_=command('find-msbuild',[vswhere,'-latest','-requires','Microsoft.Component.MSBuild','-find','MSBuild/**/Bin/MSBuild.exe'])
        csc=Path(found.read_text().strip().splitlines()[0]).parent/'Roslyn/csc.exe'
        exe=binary/'swarm-f-placement.exe'
        args=[csc,'/nologo','/target:exe','/platform:anycpu','/nostdlib+','/warn:4','/warnaserror+',
              '/main:PlacementDifferential','/out:'+str(exe)]
        for name in ['mscorlib','System','System.Core']:args.append('/reference:'+str(refs/(name+'.dll')))
        for name in ['FSO.SimAntics','FSO.LotView','FSO.Content','FSO.Files','FSO.Common',
                     'FSO.Vitaboy','FSO.Vitaboy.Engine','FSO.HIT','MonoGame.Framework']:
            require((binary/(name+'.dll')).is_file(),'missing original assembly '+name)
            args.append('/reference:'+str(binary/(name+'.dll')))
        args += [root/'tools/reference-csharp/vm-bootstrap.cs',root/'tools/reference-csharp/placement-differential.cs']
        command('compile-original-placement',args)
        record['executables']['original']={'sha256':sha(exe)}
        original=[]
        for repeat in [1,2]:
            identity=output/f'original-{repeat}.runtime.tsv'
            raw,errors=command(f'original-{repeat}',[exe,root,identity],timeout=90)
            require(errors.stat().st_size==0,'unexpected original placement diagnostic')
            trace=output/f'original-{repeat}.tsv';trace.write_bytes(raw.read_bytes().replace(b'\r\n',b'\n'))
            parse_trace(cases,trace.read_bytes())
            lines=identity.read_text().splitlines();pairs=[line.split('\t') for line in lines]
            require(all(len(pair)==2 for pair in pairs),'invalid original runtime identity')
            values=dict(pairs)
            require(len(values)==len(lines)==7,'ambiguous original runtime identity')
            require(values['clr'].startswith('4.0.') and values['vm_type']=='FSO.SimAntics.VM'
                    and values['entity_type']=='FSO.SimAntics.VMGameObject','wrong original runtime types')
            require(values['vm_sha256']==sha(binary/'FSO.SimAntics.dll') and values['witness_sha256']==sha(exe)
                    and values['cases_sha256']==CASES_HASH and values['provider']=='authored-two-object-ground-geometry',
                    'wrong original input/binary/provider identity')
            record['executables']['original'][f'runtime_{repeat}']=values;original.append(trace)
        compare_paths('original-repeat',*original)
        missing_identity=output/'unapproved.runtime.tsv'
        raw,errors=command('no-opt-in',[exe,root,missing_identity],expected=1,enabled=False)
        require(raw.stat().st_size==0 and not missing_identity.exists()
                and 'explicit placement test opt-in required' in errors.read_text(),'unapproved original execution occurred')
        toolchain=tomllib.loads((root/'rust-toolchain.toml').read_text())['toolchain']['channel']
        require(toolchain=='1.99.0','review changed Rust toolchain before qualification')
        command('rust-toolchain',['rustup','toolchain','install',toolchain,'--profile','minimal','--target','wasm32-unknown-unknown'],timeout=300)
        command('rust-version',['rustc','+'+toolchain,'--version','--verbose'])
        normal={}
        for target in ['native','wasm']:
            args=['cargo','+'+toolchain,'build','--locked','--lib','-p','sim-core']
            if target=='wasm':args += ['--target','wasm32-unknown-unknown']
            command('build-shipping-'+target,args,timeout=300)
            libs=build/'cargo'
            if target=='wasm':libs/='wasm32-unknown-unknown'
            libs/='debug'
            for flavor in ['normal','blocker','rollback']:
                executable=build/(target+'-'+flavor+('.wasm' if target=='wasm' else '.exe'))
                args=['rustc','+'+toolchain,'--edition=2024','--crate-name','swarm_f_placement','-D','warnings','-C','opt-level=2',
                      '--extern','sim_core='+str(libs/'libsim_core.rlib'),'-L','dependency='+str(libs/'deps')]
                if target=='wasm':args += ['--target','wasm32-unknown-unknown','--crate-type','cdylib',
                    '-L','dependency='+str(build/'cargo/debug/deps'),'-C','link-arg=--max-memory=268435456']
                if flavor!='normal':args += ['--cfg','placement_fault_'+flavor]
                args += [root/'tools/replay/placement_probe.rs','-o',executable]
                command('compile-'+target+'-'+flavor,args,timeout=180)
                record['executables'][target+'-'+flavor]={'sha256':sha(executable),'bytes':executable.stat().st_size}
                traces=[]
                for repeat in ([1,2] if flavor=='normal' else [1]):
                    label=target+'-'+flavor+f'-{repeat}';trace=output/(label+'.tsv')
                    if target=='native':
                        raw,errors=command(label,[executable],timeout=45)
                        require(errors.stat().st_size==0,'unexpected native diagnostic')
                        trace.write_bytes(raw.read_bytes().replace(b'\r\n',b'\n'))
                    else:
                        metadata,errors=command(label,['node',root/'tools/replay/run_placement_wasm.mjs',executable,trace],timeout=30)
                        require(errors.stat().st_size==0,'unexpected WASM host diagnostic')
                        meta=json.loads(metadata.read_text())
                        require(meta['wasm_sha256']==sha(executable) and meta['trace_sha256']==sha(trace)
                                and meta['imports']==[] and not meta['shared_memory'] and meta['repeated_reads']=='identical',
                                'WASM provenance mismatch')
                        record['executables'][target+'-'+flavor][f'execution_{repeat}']=meta
                    parse_trace(cases,trace.read_bytes());traces.append(trace)
                    if flavor=='normal':compare_paths('original-'+label,original[0],trace)
                    else:
                        result=compare(cases,original[0].read_bytes(),trace.read_bytes())
                        (output/(label+'.difference.json')).write_text(json.dumps(result,indent=2)+'\n')
                        expected=dict(case='collision',ts1=1,facing=0,field='query_status' if flavor=='blocker' else 'after_x',
                                      expected=10 if flavor=='blocker' else 40,actual=-1 if flavor=='blocker' else 88)
                        require(not result['passed'] and result['first_difference']==expected,'execution fault was not localized exactly')
                        record['fault_controls'].append(dict(target=target,flavor=flavor,detected=True,first_difference=expected))
                        print('detected '+label+': '+str(expected),flush=True)
                if flavor=='normal':compare_paths(target+'-repeat',*traces);normal[target]=traces[0]
        compare_paths('native-wasm',normal['native'],normal['wasm'])
        empty=build/'empty.wasm';empty.write_bytes(b'\0asm\x01\0\0\0')
        invalid=output/'invalid.tsv'
        _,errors=command('wasm-missing-exports',['node',root/'tools/replay/run_placement_wasm.mjs',empty,invalid],timeout=30,expected=1)
        require('missing WASM memory' in errors.read_text() and not invalid.exists(),'empty WASM accepted')
        require(inputs=={str(p.relative_to(root)).replace('\\','/'):sha(p) for p in paths},'placement source inputs changed')
        record['passed']=True
    except (OSError,ValueError,AssertionError,KeyError,subprocess.SubprocessError) as error:
        record['error']=str(error);print('FAIL: '+str(error),file=sys.stderr,flush=True)
    finally:
        record['evidence_sha256']={p.name:sha(p) for p in sorted(output.iterdir()) if p.is_file()}
        (output/'execution.json').write_text(json.dumps(record,indent=2)+'\n')
        print(json.dumps({k:v for k,v in record.items() if k not in ('commands','inputs','executables','comparisons','evidence_sha256')},indent=2))
    return 0 if record['passed'] else 1

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();raise SystemExit(run(args.root,args.output))
