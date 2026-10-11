"""Native buffered PCM/WAVE playback boundary (MPL-2.0).

NativeMixer produces stereo PCM; codec.encode_wave supplies the output file.
This adapter owns an explicit external FFplay child, with bounded file input,
wall-clock deadline, diagnostics, pause/resume, cancellation, and cleanup.
No engine or simulation callbacks exist. The supported mode is buffered-file
playback, not a low-latency device callback or unbounded MP3 stream.
"""
from __future__ import annotations
import os,pathlib,shutil,signal,subprocess,tempfile,threading,time,wave
class NativePlayer:
    def __init__(self,*,executable=None,audio_driver=None):
        self.executable=shutil.which(executable or 'ffplay');self.audio_driver=audio_driver
        if audio_driver is not None and (not isinstance(audio_driver,str) or not audio_driver.isalnum() or len(audio_driver)>32):raise ValueError('invalid native audio driver')
        self.state='idle';self.error=None;self._child=None;self._temporary=None;self._log=None;self._watcher=None;self._deadline=None;self._lock=threading.RLock();self._generation=0
    def capabilities(self):
        return {'backend':'external-ffplay','executable':self.executable,'buffered_file_playback':self.executable is not None,'pcm_mixing':'wonderland_audio_runtime::pcm::NativeMixer','live_device_callback':False,'pause_resume':os.name=='posix','audio_driver':self.audio_driver or 'system-default','physical_output_verified':False,'bundled_decoder':False}
    def start(self,path,*,root,deadline=30,max_file_bytes=64*1024*1024):
        if self.state=='disposed':raise RuntimeError('native player disposed')
        if self.executable is None:raise RuntimeError('external FFplay capability unavailable')
        if not 0<deadline<=60 or not 44<max_file_bytes<=64*1024*1024:raise ValueError('invalid native playback budget')
        root=pathlib.Path(root).resolve(strict=True);path=pathlib.Path(path).resolve(strict=True)
        try:path.relative_to(root)
        except ValueError:raise ValueError('playback input escapes authorized root') from None
        if not path.is_file() or path.stat().st_size>max_file_bytes:raise ValueError('native input byte budget')
        with wave.open(str(path),'rb') as sound:
            if sound.getnchannels() not in (1,2) or sound.getsampwidth()!=2 or not 1<=sound.getframerate()<=384000 or sound.getnframes()==0:raise ValueError('native playback requires PCM16 WAVE')
            if sound.getnframes()*sound.getnchannels()*2+44>max_file_bytes:raise ValueError('native PCM byte budget')
        self.stop();self._cleanup()
        self._temporary=tempfile.TemporaryDirectory(prefix='wonderland-native-audio-');log_path=pathlib.Path(self._temporary.name)/'player.log';self._log=log_path.open('wb');env=os.environ.copy()
        if self.audio_driver:env['SDL_AUDIODRIVER']=self.audio_driver
        def limits():
            import resource
            resource.setrlimit(resource.RLIMIT_FSIZE,(65536,65536));resource.setrlimit(resource.RLIMIT_AS,(512*1024*1024,512*1024*1024))
        with self._lock:
            self._generation+=1;generation=self._generation
            self._child=subprocess.Popen([self.executable,'-nodisp','-autoexit','-loglevel','error','-protocol_whitelist','file,pipe','-i',str(path)],stdin=subprocess.DEVNULL,stdout=self._log,stderr=self._log,env=env,start_new_session=True,preexec_fn=limits if os.name=='posix' else None)
            self.state='playing';self.error=None;self._deadline=time.monotonic()+deadline
        self._watcher=threading.Thread(target=self._watch,args=(generation,log_path),daemon=True);self._watcher.start()
    def _watch(self,generation,log_path):
        while True:
            with self._lock:
                if generation!=self._generation or self.state not in ('playing','paused'):return
                code=self._child.poll()
                if code is not None:
                    self.state='finished' if code==0 else 'failed'
                    if code:self.error=log_path.read_bytes()[:65536].decode(errors='replace')
                    return
                if time.monotonic()>=self._deadline or log_path.stat().st_size>=65536:
                    if os.name=='posix':os.killpg(self._child.pid,signal.SIGKILL)
                    else:self._child.kill()
                    self._child.wait(timeout=2);self.state='timed_out' if time.monotonic()>=self._deadline else 'failed';self.error='native playback deadline' if self.state=='timed_out' else 'native diagnostic budget';return
            time.sleep(.01)
    def pause(self):
        if os.name!='posix':raise RuntimeError('native pause capability unavailable')
        with self._lock:
            if self.state=='playing' and self._child.poll() is None:os.kill(self._child.pid,signal.SIGSTOP);self.state='paused'
    def resume(self):
        if os.name!='posix':raise RuntimeError('native resume capability unavailable')
        with self._lock:
            if self.state=='paused' and self._child.poll() is None:os.kill(self._child.pid,signal.SIGCONT);self.state='playing'
    def poll(self):
        with self._lock:return self.state
    def stop(self):
        with self._lock:
            if self.state=='disposed':return
            self._generation+=1
            if self._child is not None and self._child.poll() is None:
                if os.name=='posix':os.killpg(self._child.pid,signal.SIGKILL)
                else:self._child.kill()
                self._child.wait(timeout=2)
            self._child=None
            if self.state!='idle':self.state='stopped'
        if self._watcher is not None and self._watcher is not threading.current_thread():self._watcher.join(timeout=2)
        self._watcher=None
    def _cleanup(self):
        if self._log:self._log.close();self._log=None
        if self._temporary:self._temporary.cleanup();self._temporary=None
    def dispose(self):
        if self.state!='disposed':self.stop();self._cleanup();self.state='disposed'
    def __enter__(self):return self
    def __exit__(self,*args):self.dispose()
