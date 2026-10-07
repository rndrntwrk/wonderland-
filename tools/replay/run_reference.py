#!/usr/bin/env python3
"""Execute an explicitly scoped original-component / native / WASM cohort.

Requires an isolated clean checkout, Python 3.11+, Git, mcs/Mono, the repository's
Rust toolchain+WASM target and Node on a POSIX host. Output must be a NEW directory
outside the repository. No download, package installation, product-source rewrite,
production server, original game content pack or persistent effect is performed.
"""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import tomllib

sys.dont_write_bytecode = True
import baseline as baseline_gate
import compare_reference as comparison

ROOT = Path(__file__).resolve().parents[2]
SOURCE_BASELINE = '4c6b3e8f5835b228723caea3c9f683c62f244f73'
REUSED_FROM = '8a0e251d19e222a0a6833d7408ca629f674e1729'
# Whole-byte Git blob identities read from the pinned original and A source.
PINS = {
    'TSOClient/tso.simantics/VMClock.cs':'c8b5d97452d35768b88284a0137562d575738be4',
    'TSOClient/tso.simantics/VMContext.cs':'758d6d29f143af363646fed159fb1004443cbc60',
    'TSOClient/tso.simantics/Marshals/VMContextMarshal.cs':'38850bded859f18235d403912cae7fccbbef4a5c',
    'TSOClient/tso.simantics/NetPlay/Model/VMSerializable.cs':'77cde4b3d75059a8c22ebbe2afae7897c99f9e2f',
    'tools/reference-csharp/reference-lifecycle.py':'56e3f72aa3f7ec0726cd5c7727c5ac43b0157df9',
    'tools/reference-csharp/reference-lifecycle.cs':'1641bdfa0a4d0b7504eaf35f6a3d1726fce9d240',
    'tools/reference-csharp/reference-lifecycle.expected.tsv':'3468d8664de78c892b2c233fd9c69a126fc61368',
}

class RunError(ValueError):
    pass

def require(value,message):
    if not value:
        raise RunError(message)

def sha(data):
    return hashlib.sha256(data).hexdigest()

def blob(data):
    return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()

def write_json(path,value):
    with path.open('xb') as file:
        file.write(comparison.encode(value))

def pin_inputs(root):
    reader=baseline_gate.Root(root)
    tree=subprocess.check_output(['git','-C',str(root),'ls-tree','-rz','--full-tree','HEAD'],timeout=15)
    require(len(tree)<=16*1024*1024,'Git tree metadata limit')
    entries={}
    for row in tree.split(b'\0'):
        if row:
            meta,path=row.split(b'\t',1)
            mode,kind,digest=meta.decode().split(' ')
            entries[path.decode()]=(mode,kind,digest)
    selected=set(PINS)
    for path in entries:
        if path.startswith(('tools/reference-csharp/','tools/replay/','fixtures/reference/','crates/sim-core/src/')):
            selected.add(path)
    selected.update(['Cargo.toml','Cargo.lock','rust-toolchain.toml','crates/sim-core/Cargo.toml',
                     'crates/sim-core/tests/avatar_source_reference.rs',
                     'tests/parity/test_reference_trace.py','.github/workflows/swarm-f-reference.yml'])
    result={}
    for path in sorted(selected):
        entry=entries.get(path)
        require(entry is not None and entry[0] in ('100644','100755') and entry[1]=='blob','Untracked/nonregular reference input: '+path)
        data=reader.read(path)
        actual=blob(data)
        require(actual==entry[2],'Input differs from committed bytes: '+path)
        require(path not in PINS or actual==PINS[path],'Original/reused source pin mismatch: '+path)
        result[path]={'git_blob':actual,'sha256':sha(data)}
    return result

class Commands:
    def __init__(self,root,build,evidence,report):
        self.root,self.build,self.evidence,self.report=root,build,evidence,report
        channel=tomllib.loads((root/'rust-toolchain.toml').read_text())['toolchain']['channel']
        require(isinstance(channel,str) and len(channel)<80,'Invalid pinned Rust channel')
        self.env=dict(os.environ,LC_ALL='C',TZ='UTC',PYTHONDONTWRITEBYTECODE='1',
                      CARGO_TARGET_DIR=str(build/'cargo'),RUSTUP_TOOLCHAIN=channel,
                      RUSTFLAGS='',CARGO_ENCODED_RUSTFLAGS='',RUSTC_WRAPPER='',RUSTC_WORKSPACE_WRAPPER='')
        report['rust_channel']=channel

    def run(self,label,args,expected=0,timeout=180):
        output=self.evidence/(label+'.stdout')
        error=self.evidence/(label+'.stderr')
        record={'name':label,'command':[str(arg) for arg in args],'expected_exit':expected}
        self.report['commands'].append(record)
        with output.open('xb') as stdout,error.open('xb') as stderr:
            process=subprocess.Popen(record['command'],cwd=self.root,env=self.env,stdout=stdout,stderr=stderr,start_new_session=True)
            deadline=time.monotonic()+timeout
            try:
                while process.poll() is None:
                    require(time.monotonic()<deadline,'Command deadline exceeded: '+label)
                    require(output.stat().st_size<=16*1024*1024 and error.stat().st_size<=16*1024*1024,'Command log limit: '+label)
                    time.sleep(0.05)
                status=process.returncode
            except BaseException:
                try:os.killpg(process.pid,signal.SIGKILL)
                except ProcessLookupError:pass
                process.wait()
                record['aborted']=True
                raise
        record['exit']=status
        require(output.stat().st_size<=16*1024*1024 and error.stat().st_size<=16*1024*1024,'Command log limit: '+label)
        record['stdout_sha256']=sha(output.read_bytes())
        record['stderr_sha256']=sha(error.read_bytes())
        if status!=expected:
            print(error.read_text(errors='replace')[-12000:],file=sys.stderr)
            raise RunError('Unexpected command result: '+label+' exit '+str(status))
        print('PASS '+label+' (exit '+str(status)+')',flush=True)
        return output

    def compare(self,label,cases,reference,candidate,expected=0):
        destination=self.evidence/(label+'.json')
        self.run(label,[sys.executable,'-B',self.root/'tools/replay/compare_reference.py',
            '--cases',cases,'--reference',reference,'--candidate',candidate,'--output',destination],expected,30)
        return json.loads(destination.read_text())


def run(root,output):
    report={'schema':1,'passed':False,'full_original_engine':'not-tested','source_baseline':SOURCE_BASELINE,
            'scope':'whole-original-clock-plus-extracted-rng-component-cohort',
            'private_eod_and_production_effects':'not-in-cohort','commands':[]}
    evidence=None
    try:
        identity=baseline_gate.git_identity(root)
        require(Path(__file__).resolve()==root/'tools/replay/run_reference.py','Wrong checker checkout')
        output=output.absolute()
        require(root.resolve() not in output.resolve().parents and output.resolve()!=root.resolve(),'Output must be outside the source checkout')
        for part in (output,*output.parents):
            require(not part.is_symlink(),'Output symlink is not permitted')
        output.mkdir(mode=0o700,parents=False,exist_ok=False)
        build=output/'build';build.mkdir(mode=0o700)
        evidence=output/'evidence';evidence.mkdir(mode=0o700)
        report['tested_checkout']=identity
        initial_inputs=pin_inputs(root)
        report['inputs']=dict(initial_inputs)
        report['baseline_input_verification']=baseline_gate.verify_git_inputs(root,baseline_gate.collect_source(root))
        runner=Commands(root,build,evidence,report)
        cases=root/'fixtures/reference/clock-rng-cases.tsv'
        cases_data=comparison.load(cases,65536)
        declared=comparison.parse_cases(cases_data)
        require([case['case'] for case in declared]==['tso_zero','ts1_rollover','tso_max','ts1_mixed'],'Required original-component cohort changed')
        require([case['steps'] for case in declared]==[180,64,40,96],'Required cohort coverage changed')
        report['fixture']={'sha256':sha(cases_data),'cases':len(declared),'records':sum(case['steps'] for case in declared),
                          'classification':'authored-numeric-starts-and-command-schedule-no-game-assets'}
        for label,args in [('mcs-version',['mcs','--version']),('mono-version',['mono','--version']),
                           ('rustc-version',['rustc','-vV']),('cargo-version',['cargo','--version']),
                           ('node-version',['node','--version']),('git-version',['git','--version'])]:
            runner.run(label,args,timeout=30)
        report['python']=sys.version

        # Reuse A's exact source-block extractor and hash-pinned lifecycle driver.
        spec=importlib.util.spec_from_file_location('f_existing_reference',root/'tools/reference-csharp/reference-lifecycle.py')
        original=importlib.util.module_from_spec(spec);spec.loader.exec_module(original)
        source=root/'TSOClient/tso.simantics'
        reader=baseline_gate.Root(root)
        for path,expected in original.SHA256.items():
            name='TSOClient/tso.simantics/'+path
            data=reader.read(name)
            require(sha(data)==expected,'A original source checksum mismatch: '+name)
            require(blob(data)==subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD:'+name],text=True,timeout=10).strip(),'Original method input differs from HEAD: '+name)
            report['inputs'][name]={'git_blob':blob(data),'sha256':sha(data)}
        lifecycle=[]
        for index in (1,2):
            lifecycle.append(runner.run('original-lifecycle-'+str(index),[sys.executable,'-B',
                root/'tools/reference-csharp/reference-lifecycle.py','--source',root],timeout=120))
        expected=(root/'tools/reference-csharp/reference-lifecycle.expected.tsv').read_bytes()
        require(lifecycle[0].read_bytes()==lifecycle[1].read_bytes()==expected,'Original extracted lifecycle output not reproducible')
        report['reused_lifecycle']={'source_commit':REUSED_FROM,'scope':'extracted-original-methods-with-test-shims-not-full-vm',
                                  'runs':2,'records':len(expected.splitlines()),'sha256':sha(expected),'matches_unchanged_A_vectors':True}

        template=(root/'tools/reference-csharp/clock-rng.cs.in').read_text()
        extractions={}
        for marker,path,signature in [
            ('VM_NEXT_RANDOM','VMContext.cs','public ulong NextRandom('),
            ('CLOCK_MARSHAL','Marshals/VMContextMarshal.cs','public class VMClockMarshal : VMSerializable'),
        ]:
            text=(source/path).read_bytes().decode('utf-8-sig')
            require(text.count(signature)==1,'Ambiguous original extraction signature')
            block=original.source_block(text,signature)
            require(template.count('@@'+marker+'@@')==1,'Invalid reference template')
            template=template.replace('@@'+marker+'@@',block)
            start=text.index(block)
            extractions[marker]={'source':'TSOClient/tso.simantics/'+path,'first_line':text[:start].count('\n')+1,
                                 'last_line':text[:start+len(block)].count('\n')+1,'block_sha256':sha(block.encode())}
        require('@@' not in template,'Unresolved reference template')
        generated=build/'clock-rng.generated.cs'
        generated.write_text(template,encoding='utf-8')
        report['extractions']=extractions
        report['generated_driver_sha256']=sha(generated.read_bytes())
        exe=build/'clock-rng-reference.exe'
        runner.run('compile-original-clock-rng',['mcs','-langversion:7.2','-checked-','-warn:4','-warnaserror',
            '-r:System.Core','-out:'+str(exe),generated,source/'VMClock.cs',source/'NetPlay/Model/VMSerializable.cs'])
        cs1=runner.run('original-clock-rng-1',['mono',exe,cases],timeout=30)
        cs2=runner.run('original-clock-rng-2',['mono',exe,cases],timeout=30)
        require(cs1.read_bytes()==cs2.read_bytes(),'Original component replay is not repeatable')
        runner.compare('reference-repeat',cases,cs1,cs2)

        # Cargo builds the unchanged shipping library. The separate F probe links
        # its actual rlib for each target; no copied Rust implementation is used.
        runner.run('build-native-core',['cargo','build','--locked','-p','sim-core'],timeout=300)
        runner.run('build-wasm-core',['cargo','build','--locked','-p','sim-core','--target','wasm32-unknown-unknown'],timeout=300)
        probe=root/'tools/replay/reference_probe.rs'
        targets={}
        comparisons={}
        faults=[]
        for flavor in ('normal','rng','branch'):
            outputs={}
            for target in ('native','wasm'):
                library=build/'cargo'
                if target=='wasm':library=library/'wasm32-unknown-unknown'
                library=library/'debug'
                destination=build/('probe-'+flavor+('.wasm' if target=='wasm' else ''))
                args=['rustc','--edition=2024','--crate-name','swarm_f_reference','-D','warnings','-C','opt-level=2',
                      '--extern','sim_core='+str(library/'libsim_core.rlib'),'-L','dependency='+str(library/'deps')]
                if target=='wasm':args+=['--target','wasm32-unknown-unknown','--crate-type','cdylib']
                if flavor!='normal':args+=['--cfg','reference_fault_'+flavor]
                args += [probe,'-o',destination]
                runner.run('compile-'+target+'-'+flavor,args,timeout=120)
                targets[target+'-'+flavor]={'sha256':sha(destination.read_bytes()),'bytes':destination.stat().st_size,
                                           'fault_mode':None if flavor=='normal' else flavor}
                if target=='native':
                    outputs[target]=runner.run('execute-native-'+flavor,[destination],timeout=30)
                else:
                    outputs[target]=evidence/('execute-wasm-'+flavor+'.tsv')
                    metadata=runner.run('execute-wasm-'+flavor,['node',root/'tools/replay/run_reference_wasm.mjs',destination,outputs[target]],timeout=30)
                    targets[target+'-'+flavor]['execution']=json.loads(metadata.read_text())
                record=runner.compare('compare-'+target+'-'+flavor,cases,cs1,outputs[target],0 if flavor=='normal' else 1)
                if flavor=='normal':
                    comparisons[target]=record
                else:
                    first=record['first_difference']
                    require(first['case']=='ts1_rollover' and first['step']==7 and first['field']==('rng_after' if flavor=='rng' else 'branch'),
                            'Injected fault was not localized at the expected first field')
                    faults.append({'target':target,'fault':flavor,'first_difference':first,'detected':True})
            if flavor=='normal':
                runner.compare('native-wasm',cases,outputs['native'],outputs['wasm'])
                repeated=runner.run('execute-native-repeat',[build/'probe-normal'],timeout=30)
                require(repeated.read_bytes()==outputs['native'].read_bytes(),'Native repeat changed')
        report['comparisons']=comparisons
        report['fault_controls']=faults
        report['executables']=targets
        report['original_executable']={'sha256':sha(exe.read_bytes()),'bytes':exe.stat().st_size}

        # Existing avatar method oracle is executed, never relabelled a full VM.
        avatar=runner.run('existing-avatar-extracted-reference',['cargo','test','--locked','-p','sim-core',
            '--test','avatar_source_reference','avatar_extracted_mono_reference','--','--ignored','--exact'],timeout=180)
        require('1 passed; 0 failed' in avatar.read_text(),'Existing extracted avatar gate did not actually execute')
        report['avatar_reference']={'scope':'existing-extracted-method-harness','passed':1,'full_vm':False}
        require(pin_inputs(root)==initial_inputs,'Reference inputs changed during execution')
        baseline_gate.verify_git_inputs(root,baseline_gate.collect_source(root))
        require(baseline_gate.git_identity(root)==identity,'Checkout changed during reference execution')
        report['passed']=True
    except (OSError,ValueError,subprocess.SubprocessError,ImportError,KeyError) as error:
        report['error']=str(error)
        print('FAIL: '+str(error),file=sys.stderr)
    finally:
        if evidence is not None:
            report['evidence_sha256']={path.name:sha(path.read_bytes()) for path in sorted(evidence.iterdir()) if path.is_file()}
            write_json(evidence/'execution.json',report)
        print(json.dumps({key:value for key,value in report.items() if key not in ('inputs','commands','evidence_sha256','executables')},indent=2))
    return 0 if report['passed'] else 1

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    raise SystemExit(run(ROOT,parser.parse_args().output))
