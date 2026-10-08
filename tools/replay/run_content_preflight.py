#!/usr/bin/env python3
"""Build and verify the real-content static preflight on native/WASM.

A successful verification means the pinned BLOCKERS were reproduced, not that
an object is playable. No original C# VM, scene, production provider or network
service runs here. All compilers, cache inputs and source must already be trusted.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tomllib

EXPECTED_SHA256='d05ecf34f43a397706ed9cbe8594f8ad5141e90893aad3bfed28d8aba5cfa3bf'
BASE='e32387449850b8ea2ffb93b77a48110b7b65ab65'
BASE_TREE='b822c1fce46d893ec3b04cf8e43d158fd3b79546'
SOURCE='TSOClient/FSO.Content.TSO/Content/Objects/'
COHORT=('Casino_2-Tile_Bar_CC.iff','Chair_fso_Bouncy_Beach_Ball.iff','fso_christmas_flag.iff','cursebook_set_permission.iff')
CRATES=('sim-core','content-ir','legacy-formats','content-runtime-bridge')
LIBS=('wonderland_content_runtime_bridge','wonderland_content_ir','wonderland_legacy_formats','serde','serde_json','sha2')
OWN=('tools/replay/content_scope.rs','tools/replay/content_preflight_probe.rs','tools/replay/run_content_preflight.py',
     'tests/parity/content_scope_cases.rs','tests/parity/test_content_preflight_evidence.py','fixtures/reference/content-preflight.expected.json',
     'docs/swarm-f/CONTENT_PREFLIGHT.md','.github/workflows/swarm-f-content-preflight.yml')
MAX_REPORT=1024*1024

def require(ok:bool,message:str)->None:
    if not ok: raise ValueError(message)

def sha(data:bytes)->str:return hashlib.sha256(data).hexdigest()
def encoded(value)->bytes:return (json.dumps(value,sort_keys=True,indent=2,allow_nan=False)+'\n').encode()
def pairs(items):
    result={}
    for k,v in items:
        require(k not in result,'Duplicate JSON key: '+k);result[k]=v
    return result
def bad_number(value):raise ValueError('Noninteger JSON number')
def load(data:bytes):
    require(isinstance(data,bytes) and 0<len(data)<=MAX_REPORT,'Evidence byte bound')
    return json.loads(data,object_pairs_hook=pairs,parse_float=bad_number,parse_constant=bad_number)
def verify_report(data:bytes,expected:bytes)->dict:
    require(isinstance(data,bytes) and 0<len(data)<=MAX_REPORT,'Evidence byte bound')
    require(sha(expected)==EXPECTED_SHA256,'The reviewed cohort expectation changed')
    document=load(data)
    require(data==expected,'Native/WASM content prerequisite evidence differs from reviewed source graph')
    require(document['audit_completed'] is True and document['static_cohort_complete'] is False,
            'Incomplete original scopes must remain blocked')
    require(document['original_vm_executed'] is False and document['placed_world_executed'] is False
            and document['whole_object_runtime_qualified'] is False,'Static preflight is not runtime qualification')
    return document

def read(path:Path,limit=16*1024*1024)->bytes:
    require(path.is_file() and not path.is_symlink(),'Not a regular file: '+str(path))
    require(path.stat().st_size<=limit,'Input size bound: '+str(path))
    with path.open('rb') as file:data=file.read(limit+1)
    require(len(data)<=limit,'Input grew past bound');return data

def source_inputs(root:Path)->dict:
    names=set(OWN)|{'Cargo.toml','Cargo.lock','rust-toolchain.toml','tools/replay/run_reference_wasm.mjs'}
    names.update(SOURCE+n for n in COHORT)
    for crate in CRATES:
        folder=root/'crates'/crate
        names.add(f'crates/{crate}/Cargo.toml')
        names.update(str(p.relative_to(root)).replace('\\','/') for p in (folder/'src').rglob('*') if p.is_file() or p.is_symlink())
        for extra in ('build.rs','build'):
            require(not (folder/extra).exists(),'Unreviewed library build hook: '+str(folder/extra))
    require(len(names)<2000,'Source set bound')
    for parent in [root,*root.parents]:
        require(not any((parent/'.cargo'/file).exists() for file in ('config','config.toml')),
                'Unreviewed repository/ancestor Cargo configuration')
    result={}
    for name in sorted(names):
        path=root
        for part in Path(name).parts:
            path=path/part;require(not path.is_symlink(),'Symlink source input: '+name)
        result[name]=sha(read(path))
    return result

def git(root:Path,*args:str)->str:
    return subprocess.check_output(['git','-C',str(root),*args],text=True,stderr=subprocess.PIPE,timeout=15).strip()

def identify(root:Path,pins:dict,reference:Path|None)->dict:
    if reference is not None:
        # Explicitly reconstructed input copy: do not invent a current Git head.
        doc=json.loads(read(reference,8*1024*1024),object_pairs_hook=pairs)
        require(doc['tree']==BASE_TREE,'Wrong prior reference tree')
        original={n:h for n,h in pins.items() if n not in OWN}
        for name,h in original.items():
            require(doc['files'].get(name,{}).get('sha256')==h,'Input differs from pinned PR45: '+name)
        relevant={n for n in doc['files'] if any(n.startswith(f'crates/{c}/src/') for c in CRATES)}
        require(relevant.issubset(original),'Missing source files from the pinned dependency closure')
        return {'mode':'reconstructed-dependency-input-copy','current_commit':None,'reference_commit':doc['commit'],
                'reference_tree':doc['tree'],'matching_original_inputs':len(original),'reference_record_sha256':sha(read(reference,8*1024*1024))}
    require(Path(git(root,'rev-parse','--show-toplevel')).resolve()==root,'Root is not repository top level')
    require(not git(root,'status','--porcelain','--untracked-files=all'),'Checkout must be clean; evidence belongs outside it')
    subprocess.run(['git','-C',str(root),'merge-base','--is-ancestor',BASE,'HEAD'],check=True,timeout=15)
    # Check actual bytes, not only status: skip-worktree cannot hide changed inputs.
    entries={}
    raw=subprocess.check_output(['git','-C',str(root),'ls-tree','-rz','HEAD'],timeout=15)
    require(len(raw)<=16*1024*1024,'Tracked tree size bound')
    for row in raw.split(b'\0'):
        if row:
            meta,name=row.split(b'\t',1);entries[name.decode()]=meta.decode().split()
    for name in pins:
        mode,kind,digest=entries.get(name,('','',''))
        require(kind=='blob' and mode in ('100644','100755'),'Input not tracked: '+name)
        data=read(root/name)
        require(hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()==digest,'Input differs from HEAD: '+name)
    return {'mode':'clean-tracked-checkout','current_commit':git(root,'rev-parse','HEAD'),'tree':git(root,'rev-parse','HEAD^{tree}')}

class Runner:
    def __init__(self,root:Path,out:Path,record:dict):
        self.root,self.out,self.record=root,out,record
        self.env=os.environ.copy()
        self.target=Path(self.env.get('CARGO_TARGET_DIR',str(out/'work'/'cargo'))).resolve()
        require(not self.target.is_relative_to(root),'Cargo build output must stay outside source')
        self.env['CARGO_TARGET_DIR']=str(self.target)
        self.env['PYTHONDONTWRITEBYTECODE']='1'
        for key in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTC_BOOTSTRAP'):
            self.env.pop(key,None)
    def run(self,label:str,args:list,expected=0,timeout=180)->Path:
        print('Executing '+label,flush=True)
        args=[str(a) for a in args];stdout=self.out/'evidence'/(label+'.stdout');stderr=self.out/'evidence'/(label+'.stderr')
        command={'label':label,'argv':args,'expected_exit':expected,'exit':None}
        self.record['commands'].append(command)
        with stdout.open('xb') as a,stderr.open('xb') as b:
            process=subprocess.Popen(args,cwd=self.root,env=self.env,stdout=a,stderr=b,start_new_session=os.name=='posix')
            try:command['exit']=process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                if os.name=='posix':os.killpg(process.pid,signal.SIGKILL)
                else:process.kill()
                process.wait();raise ValueError('Command deadline: '+label) from None
        require(stdout.stat().st_size<=32*1024*1024 and stderr.stat().st_size<=32*1024*1024,'Command output size bound')
        require(command['exit']==expected,f'{label}: exit {command["exit"]}; see {stderr.name}')
        return stdout
    def libraries(self,target:str,offline:bool)->dict:
        args=['cargo','build','--locked','--lib','-p','wonderland-content-runtime-bridge','--no-default-features','--message-format=json']
        if offline:args+=['--offline']
        if target=='wasm':args+=['--target','wasm32-unknown-unknown']
        output=self.run('build-'+target,args,timeout=300)
        libraries={}
        for line in output.read_text().splitlines():
            message=json.loads(line)
            if message.get('reason')=='compiler-artifact':
                name=message['target']['name']
                if name in LIBS:
                    candidates=[p for p in message['filenames'] if p.endswith('.rlib')]
                    require(len(candidates)==1,'Ambiguous Cargo library: '+name)
                    require(name not in libraries,'Duplicate Cargo library: '+name)
                    libraries[name]=Path(candidates[0])
        require(set(libraries)==set(LIBS),'Incomplete shipping dependency build')
        return libraries
    def compile(self,label:str,path:str,libraries:dict,wasm=False,test=False,fault=False,compiler='rustc')->Path:
        dest=self.out/'work'/(label+('.wasm' if wasm else ('.exe' if os.name=='nt' else '')))
        args=[compiler,'--edition=2024','--crate-name','swarm_f_content_preflight','-D','warnings','-C','opt-level=2']
        if test:args+=['--test']
        if wasm:args+=['--target','wasm32-unknown-unknown','--crate-type','cdylib']
        if fault:args+=['--cfg','preflight_fault_pin']
        dirs={p.parent for p in libraries.values()};dirs.add(self.target/'debug/deps')
        for folder in sorted(dirs):args+=['-L','dependency='+str(folder)]
        for name,file in sorted(libraries.items()):args+=['--extern',f'{name}={file}']
        args += [self.root/path,'-o',dest]
        self.run(label,args)
        self.record['executables'][label]={'sha256':sha(read(dest,32*1024*1024)),'bytes':dest.stat().st_size}
        return dest

def execute(root:Path,out:Path,reference:Path|None,offline:bool)->int:
    require(not out.exists(),'Output directory already exists')
    require(not out.is_relative_to(root),'Output must be outside source tree')
    out.mkdir(parents=False);(out/'evidence').mkdir();(out/'work').mkdir()
    record={'schema':1,'verification_passed':False,'static_admission':'not-evaluated','original_vm_executed':False,
            'placed_world_executed':False,'full_object_qualified':False,'commands':[],'executables':{},'scope':'static-content-prerequisite-evidence'}
    try:
        before=source_inputs(root);record['source_inputs']=before;record['checkout']=identify(root,before,reference)
        version=tomllib.loads((root/'rust-toolchain.toml').read_text())['toolchain']['channel']
        runner=Runner(root,out,record)
        compiler=runner.run('compiler',['rustc','-vV']).read_text();require(compiler.startswith('rustc '+version+' '),'Compiler must match pinned toolchain')
        runner.run('node-version',['node','--version'])
        runner.run('format',['rustfmt','--edition','2024','--check',*(root/p for p in OWN[:2]),root/'tests/parity/content_scope_cases.rs'])
        runner.run('evidence-tests',[sys.executable,'-B','-m','unittest','discover','-s','tests/parity','-p','test_content_preflight_evidence.py','-v'])
        native=runner.libraries('native',offline);wasm=runner.libraries('wasm',offline)
        tests=runner.compile('scope-tests','tests/parity/content_scope_cases.rs',native,test=True)
        runner.run('scope-tests-executed',[tests])
        runner.compile('scope-clippy','tests/parity/content_scope_cases.rs',native,test=True,compiler='clippy-driver')
        runner.compile('native-clippy','tools/replay/content_preflight_probe.rs',native,compiler='clippy-driver')
        runner.compile('wasm-clippy','tools/replay/content_preflight_probe.rs',wasm,wasm=True,compiler='clippy-driver')
        normal=runner.compile('native-normal','tools/replay/content_preflight_probe.rs',native)
        module=runner.compile('wasm-normal','tools/replay/content_preflight_probe.rs',wasm,wasm=True)
        expected=read(root/'fixtures/reference/content-preflight.expected.json',MAX_REPORT)
        outputs=[]
        for i in (1,2):
            data=runner.run('native-run-'+str(i),[normal],timeout=20).read_bytes();verify_report(data,expected);outputs.append(data)
        admission=runner.run('static-admission-blocked',[normal,'--require-static-closure'],expected=1,timeout=20).read_bytes()
        verify_report(admission,expected)
        wasm_output=out/'evidence/content-wasm.json'
        host=runner.run('wasm-run',['node',root/'tools/replay/run_reference_wasm.mjs',module,wasm_output],timeout=20)
        outputs.append(read(wasm_output,MAX_REPORT));verify_report(outputs[-1],expected)
        require(all(data==outputs[0] for data in outputs),'Native/WASM prerequisite reports diverged')
        record['native_wasm']={'bytes':len(outputs[0]),'sha256':sha(outputs[0]),'equal':True,'host':load(host.read_bytes())}
        fault=runner.compile('native-wrong-pin','tools/replay/content_preflight_probe.rs',native,fault=True)
        runner.run('wrong-pin-rejected',[fault],expected=2,timeout=20)
        require((out/'evidence/wrong-pin-rejected.stdout').stat().st_size==0,'Wrong source emitted a success report')
        require('Source digest changed: Casino_2-Tile_Bar_CC.iff' in (out/'evidence/wrong-pin-rejected.stderr').read_text(),'Wrong negative-control cause')
        require(source_inputs(root)==before,'Source changed during build or audit')
        require(identify(root,before,reference)==record['checkout'],'Checkout identity changed')
        record['static_admission']='blocked-missing-dependencies';record['verification_passed']=True
        record['source_inputs_rechecked']=len(before)
        record['summary']={'resources':4,'original_object_definitions':sum(len(r['audit']['objects']) for r in load(expected)['resources']),
                           'incomplete_resources':4,'source_bytes_modified':0,'missing_dependencies_substituted':0}
    except (ValueError,OSError,subprocess.SubprocessError,KeyError,json.JSONDecodeError) as error:
        record['error']=str(error)
    finally:
        record['evidence_sha256']={p.name:sha(p.read_bytes()) for p in sorted((out/'evidence').iterdir()) if p.is_file()}
        (out/'evidence/execution.json').write_bytes(encoded(record))
        print(json.dumps({k:v for k,v in record.items() if k not in ('source_inputs','commands','evidence_sha256','executables')},indent=2))
    return 0 if record['verification_passed'] else 1
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[2])
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--input-copy-reference',type=Path,help='Explicit prior hosted source-identity.json; never claims a current tracked checkout')
    parser.add_argument('--offline',action='store_true')
    args=parser.parse_args()
    try:raise SystemExit(execute(args.root.resolve(),args.output.resolve(),args.input_copy_reference,args.offline))
    except (ValueError,OSError) as error:parser.exit(2,str(error)+'\n')
