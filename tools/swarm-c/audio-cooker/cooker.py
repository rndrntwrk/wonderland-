#!/usr/bin/env python3
"""Bounded offline audio cooking (MPL-2.0).

XA/UTK use explicitly supplied normalized B metadata plus the Rust decoder;
this module does not reimplement B's audio/HIT metadata parsers. PCM WAVE uses
Python's standard-library wave reader. MP3 is an opt-in external FFmpeg process;
neither FFmpeg nor the conflicting-notice MP3Sharp code is bundled.
"""
from __future__ import annotations
import argparse, hashlib, json, math, os, pathlib, signal, subprocess, tempfile, time, wave
PROVENANCE={'synthetic','authorized-import','licensed-replacement','redistributable-pack'}
MAX_INPUT=32*1024*1024
MAX_OUTPUT=64*1024*1024

def run_bounded(argv,work,*,timeout=15,max_file_bytes=MAX_OUTPUT):
    if not 0<timeout<=60 or not isinstance(max_file_bytes,int) or not 1<=max_file_bytes<=MAX_OUTPUT:raise ValueError('invalid process budget')
    work=pathlib.Path(work);work.mkdir(parents=True,exist_ok=True)
    stdout_path=work/'process.stdout';stderr_path=work/'process.stderr'
    def limits():
        import resource
        resource.setrlimit(resource.RLIMIT_FSIZE,(max_file_bytes,max_file_bytes))
        resource.setrlimit(resource.RLIMIT_CPU,(math.ceil(timeout)+1,math.ceil(timeout)+2))
        resource.setrlimit(resource.RLIMIT_AS,(512*1024*1024,512*1024*1024))
    def stop(child):
        if child.poll() is None:
            if os.name=='posix':os.killpg(child.pid,signal.SIGKILL)
            else:child.kill()
        child.wait(timeout=2)
    with stdout_path.open('wb') as stdout,stderr_path.open('wb') as stderr:
        child=subprocess.Popen(list(map(str,argv)),stdin=subprocess.DEVNULL,stdout=stdout,stderr=stderr,cwd=work,start_new_session=True,preexec_fn=limits if os.name=='posix' else None)
        deadline=time.monotonic()+timeout
        try:
            while child.poll() is None:
                if time.monotonic()>=deadline:stop(child);raise RuntimeError('audio process deadline exceeded')
                if stdout_path.stat().st_size>min(max_file_bytes,65536) or stderr_path.stat().st_size>min(max_file_bytes,65536):stop(child);raise RuntimeError('audio process diagnostic budget exceeded')
                if any(p.is_file() and p.stat().st_size>max_file_bytes for p in work.iterdir() if p.name not in ('input.dat','input.pcm')):stop(child);raise RuntimeError('audio process file budget exceeded')
                time.sleep(.01)
        except BaseException:
            stop(child);raise
    with stdout_path.open('rb') as stream:out=stream.read(65537)
    with stderr_path.open('rb') as stream:err=stream.read(65537)
    if len(out)>65536 or len(err)>65536:raise RuntimeError('audio process diagnostic budget exceeded')
    if child.returncode:raise RuntimeError(f'audio process failed ({child.returncode}): {err.decode(errors="replace")}')
    if stdout_path.stat().st_size>=max_file_bytes or stderr_path.stat().st_size>=max_file_bytes:raise RuntimeError('audio process output budget reached')
    return out.decode(errors='replace'),err.decode(errors='replace')

def _inside(path,root):
    resolved=pathlib.Path(path).resolve(strict=True)
    try:resolved.relative_to(root)
    except ValueError:raise ValueError('audio input escapes authorized root') from None
    if not resolved.is_file():raise ValueError('audio input must be a file')
    return resolved

def _wave(path,max_output=MAX_OUTPUT):
    with wave.open(str(path),'rb') as sound:
        channels=sound.getnchannels();rate=sound.getframerate();width=sound.getsampwidth();frames=sound.getnframes()
        if channels not in (1,2) or not 1<=rate<=384000 or width not in (1,2,3,4) or frames<1:raise ValueError('unsupported PCM WAVE shape')
        if frames*channels*max(width,2)>max_output:raise ValueError('decoded PCM budget exceeded')
        data=sound.readframes(frames)
        if len(data)!=frames*channels*width:raise ValueError('truncated PCM WAVE data')
    return channels,rate,width,frames,data

def cook(source,output,*,root,provenance,decoder,metadata=None,ffmpeg=None,timeout=15,max_input_bytes=MAX_INPUT,max_output_bytes=MAX_OUTPUT):
    if provenance not in PROVENANCE:raise ValueError('explicit content provenance required')
    if not 1<=max_input_bytes<=MAX_INPUT or not 44<max_output_bytes<=MAX_OUTPUT:raise ValueError('invalid cooker byte budget')
    root=pathlib.Path(root).resolve(strict=True);source=_inside(source,root);output=pathlib.Path(output).absolute();sidecar=output.with_suffix(output.suffix+'.json')
    if output.exists() or sidecar.exists():raise FileExistsError('audio output already exists')
    if source.stat().st_size>max_input_bytes:raise ValueError('encoded input budget exceeded')
    with source.open('rb') as stream:raw=stream.read(max_input_bytes+1)
    if len(raw)>max_input_bytes:raise ValueError('encoded input grew beyond budget')
    if metadata is not None and len(json.dumps(metadata))>65536:raise ValueError('metadata budget exceeded')
    output.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.audio-cook-',dir=output.parent) as temporary:
        work=pathlib.Path(temporary);snapshot=work/'input.dat';snapshot.write_bytes(raw);cooked=work/'output.wav';external_version=None
        if raw.startswith(b'RIFF'):
            channels,rate,width,frames,pcm=_wave(snapshot,max_output_bytes);payload=work/'input.pcm';payload.write_bytes(pcm);encoding='pcm';offset=0;length=len(pcm);backend='rust-pcm';normalized={'encoding':'PcmWave','sample_rate':rate,'channels':channels,'bits_per_sample':width*8,'sample_frames':frames,'payload_offset':0,'payload_bytes':length}
        elif raw.startswith((b'XAI\0',b'XAJ\0',b'UTM0')):
            if metadata is None:raise ValueError('XA/UTK require normalized provider metadata')
            types={'XaSpeech':'xa-speech','XaMusic':'xa-music','Utk':'utk'};encoding=types.get(metadata.get('encoding'))
            if encoding is None:raise ValueError('unsupported normalized audio metadata')
            expected={'xa-speech':b'XAI\0','xa-music':b'XAJ\0','utk':b'UTM0'}[encoding]
            if not raw.startswith(expected):raise ValueError('encoding metadata does not match source identity')
            form=metadata.get('format',metadata);channels=form['channels'];rate=form['sample_rate'];bits=form['bits_per_sample'];frames=metadata['sample_frames'];offset=metadata['payload_offset'];length=metadata['payload_bytes'];width=bits//8;payload=snapshot;backend='rust-'+encoding;normalized=metadata
            for value in (channels,rate,bits,frames,offset,length):
                if not isinstance(value,int) or isinstance(value,bool) or value<0 or value>2**64-1:raise ValueError('invalid normalized metadata integer')
            if frames*channels*2>max_output_bytes:raise ValueError('declared output budget exceeded')
        else:
            if ffmpeg is None:raise ValueError('unsupported input; MP3 requires explicit external codec')
            if not (raw.startswith(b'ID3') or (len(raw)>=2 and raw[0]==255 and raw[1]&224==224)):
                raise ValueError('external codec accepts MP3 bytes only')
            # Force the MP3 demuxer. File-only protocols alone would still allow
            # a playlist/container to reference other local files.
            version,_=run_bounded([ffmpeg,'-version'],work,timeout=min(timeout,5),max_file_bytes=max_output_bytes);external_version=version.splitlines()[0] if version else 'unknown'
            run_bounded([ffmpeg,'-nostdin','-v','error','-protocol_whitelist','file,pipe','-f','mp3','-i',snapshot,'-map','0:a:0','-vn','-threads','1','-map_metadata','-1','-c:a','pcm_s16le','-f','wav',cooked],work,timeout=timeout,max_file_bytes=max_output_bytes)
            channels,rate,width,frames,_=_wave(cooked,max_output_bytes);backend='external-ffmpeg';normalized=None
        if not cooked.exists():
            run_bounded([pathlib.Path(decoder).resolve(),encoding,str(rate),str(channels),str(width*8),str(frames),str(offset),str(length),payload,cooked],work,timeout=timeout,max_file_bytes=max_output_bytes)
        channels,rate,width,frames,pcm=_wave(cooked,max_output_bytes)
        if width!=2:raise ValueError('cooker backend did not return PCM16')
        encoded=cooked.read_bytes()
        if len(encoded)>max_output_bytes:raise ValueError('cooked file budget exceeded')
        report={'schema':'wonderland-audio-cook-v1','status':'decoded','provenance':provenance,'source_name':str(source.relative_to(root)),'source_sha256':hashlib.sha256(raw).hexdigest(),'source_bytes':len(raw),'codec_backend':backend,'external_version':external_version,'normalized_metadata':normalized,'output_encoding':'PCM16-WAVE','output_sha256':hashlib.sha256(encoded).hexdigest(),'output_bytes':len(encoded),'sample_rate':rate,'channels':channels,'sample_frames':frames,'pcm_bytes':len(pcm),'duration_seconds':frames/rate}
        meta_bytes=(json.dumps(report,indent=2,sort_keys=True)+'\n').encode()
        # Exclusive final creation prevents accidental overwrite; remove both
        # outputs on any publication error rather than claiming a partial cook.
        written=[]
        try:
            with output.open('xb') as out:written.append(output);out.write(encoded)
            with sidecar.open('xb') as out:written.append(sidecar);out.write(meta_bytes)
        except BaseException:
            for path in written:path.unlink(missing_ok=True)
            raise
    return report

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('source');parser.add_argument('output');parser.add_argument('--root',required=True);parser.add_argument('--provenance',choices=sorted(PROVENANCE),required=True);parser.add_argument('--decoder',required=True);parser.add_argument('--metadata');parser.add_argument('--ffmpeg');parser.add_argument('--timeout',type=float,default=15);args=parser.parse_args()
    metadata=None
    if args.metadata:
        path=pathlib.Path(args.metadata)
        if path.stat().st_size>65536:raise ValueError('metadata file budget exceeded')
        metadata=json.loads(path.read_text())
    print(json.dumps(cook(args.source,args.output,root=args.root,provenance=args.provenance,decoder=args.decoder,metadata=metadata,ffmpeg=args.ffmpeg,timeout=args.timeout),indent=2))
if __name__=='__main__':main()
