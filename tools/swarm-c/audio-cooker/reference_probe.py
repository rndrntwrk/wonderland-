#!/usr/bin/env python3
"""Differential synthetic XA/UTK probe against unchanged FreeSO C# sources.
No game payloads are read or distributed. Requires installed mcs/mono and the
Rust audio-decode CLI. Source tree is read-only; all build/output is temporary.
"""
import argparse, hashlib, json, pathlib, struct, subprocess, tempfile
class Bits:
    def __init__(self): self.data=bytearray();self.n=0
    def put(self,value,count):
        for i in range(count):
            if self.n%8==0:self.data.append(0)
            self.data[-1]|=((value>>i)&1)<<(self.n%8);self.n+=1

def utk_payload(voiced,half=None,frames=2,complex_rc=False,custom=False):
    b=Bits();b.put(half is not None,1);b.put(0,4);b.put(0,4);b.put(7,6)
    for frame in range(frames):
        for i in range(12):
            if i<4:
                index=(0 if voiced else 32) if i==0 else ([0,25,45,50][i] if complex_rc else 32)
            else:index=(3+i*3+frame)%32 if complex_rc else 16
            b.put(index,6 if i<4 else 5)
        for sub in range(4):
            b.put(245 if sub==0 else 254,8);b.put(5 if frame else 0,4);b.put(frame+sub,6)
            if half:b.put(half[0],1);b.put(half[1],1)
            count=54 if half else 108
            i=0
            if voiced and custom:
                b.put(127,8);b.put(7,3);b.put(0,1);b.put(1,1);i+=1 # +10 custom magnitude
                b.put(0,2);i+=1 # table1 code4, zero and return to table0
                b.put(255,8);b.put(0,6);i+=7 # table0 code2, seven-zero run
            while i<count:
                value=(i+sub)%3
                if voiced:b.put([1,2,0][value],2)
                elif value==0:b.put(0,1)
                else:b.put(1,1);b.put(value==2,1)
                i+=1
    b.data.append(0);return bytes(b.data)

def header(encoding,channels,frames):
    rate=22050;fmt=struct.pack('<HHIIHH',1,channels,rate,rate*channels*2,channels*2,16)
    return (b'UTM0'+struct.pack('<II',frames*2,20)+fmt+struct.pack('<I',0)) if encoding=='utk' else (b'XAI\0'+struct.pack('<I',frames*channels*2)+fmt)

def vectors():
    result=[]
    for channels in (1,2):
        for predictor in (0,1,2,3,8,15):
            blocks=[]
            for block in range(3):
                predictors=bytes((((predictor+c)%16)<<4)|(block%3) for c in range(channels))
                codes=bytes(((i*23+block*31+c*17)&255) for i in range(14) for c in range(channels))
                blocks.append(predictors+codes)
            result.append((f'xa-{channels}ch-p{predictor}','xa-speech',channels,84,b''.join(blocks)))
    for voiced in (False,True):
        for half in (None,(0,1),(1,1),(0,0),(1,0)):
            for complex_rc in (False,True):
                name=f'utk-v{int(voiced)}-half{half}-rc{int(complex_rc)}'
                result.append((name,'utk',1,450,utk_payload(voiced,half,2,complex_rc,voiced)))
    return result

def run_probe(source_root,decoder):
    root=pathlib.Path(source_root).resolve();paths=[root/'TSOClient/tso.files/XA/XAFile.cs',root/'TSOClient/tso.files/UTK/UTKFile2.cs']
    report={'synthetic':True,'source_baseline':'4c6b3e8f5835b228723caea3c9f683c62f244f73','source_sha256':{str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},'vectors':[]}
    with tempfile.TemporaryDirectory(prefix='wonderland-audio-reference-') as d:
        d=pathlib.Path(d);harness=d/'Probe.cs';exe=d/'Probe.exe'
        harness.write_text('''using System;using System.IO;class Probe{static void Main(string[] a){byte[] data=File.ReadAllBytes(a[1]);byte[] wave;if(a[0]=="utk"){var u=new FSO.Files.UTK.UTKFile2(data);u.UTKDecode();wave=u.DecompressedWav;}else{wave=new FSO.Files.XA.XAFile(data).DecompressedData;}File.WriteAllBytes(a[2],wave);}}''')
        compile_run=subprocess.run(['mcs','-out:'+str(exe),str(harness),*map(str,paths)],capture_output=True,timeout=30)
        if compile_run.returncode:raise RuntimeError(compile_run.stderr.decode())
        for i,(name,encoding,channels,frames,payload) in enumerate(vectors()):
            data=header(encoding,channels,frames)+payload;path=d/f'{i}.dat';path.write_bytes(data);reference=d/f'{i}.ref.wav';actual=d/f'{i}.rust.wav';offset=32 if encoding=='utk' else 24
            ref=subprocess.run(['mono',str(exe),'utk' if encoding=='utk' else 'xa',str(path),str(reference)],capture_output=True,timeout=5)
            if ref.returncode:raise RuntimeError(ref.stderr.decode())
            rust=subprocess.run([str(decoder),encoding,'22050',str(channels),'16',str(frames),str(offset),str(len(payload)),str(path),str(actual)],capture_output=True,timeout=5)
            if rust.returncode:raise RuntimeError(rust.stderr.decode())
            expected=reference.read_bytes();observed=actual.read_bytes()
            if expected!=observed:
                where=next((j for j,(a,b) in enumerate(zip(expected,observed)) if a!=b),min(len(expected),len(observed)))
                raise AssertionError(f'{name}: first differing WAVE byte {where}; expected {expected[where:where+12].hex()}, observed {observed[where:where+12].hex()}')
            report['vectors'].append({'name':name,'encoding':encoding,'channels':channels,'frames':frames,'source_bytes':len(data),'source_sha256':hashlib.sha256(data).hexdigest(),'wave_sha256':hashlib.sha256(expected).hexdigest(),'byte_equal':True})
    report['passed']=len(report['vectors']);return report
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--source-root',required=True);parser.add_argument('--decoder',required=True);parser.add_argument('--report');args=parser.parse_args()
    report=run_probe(args.source_root,pathlib.Path(args.decoder).resolve());text=json.dumps(report,indent=2)+'\n'
    if args.report:pathlib.Path(args.report).write_text(text)
    print(json.dumps({'passed':report['passed'],'synthetic':True,'byte_equal':True}))
