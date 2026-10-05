"""Exercises real external FFplay with an explicitly silent SDL dummy device.
This proves adapter/process/output behavior, not audible physical hardware.
"""
import importlib.util,pathlib,shutil,struct,tempfile,time,unittest,wave
path=pathlib.Path(__file__).parents[4]/'crates/audio-runtime/adapters/native_player.py'
spec=importlib.util.spec_from_file_location('native_player',path);native=importlib.util.module_from_spec(spec);spec.loader.exec_module(native)
def sample(path,frames=22050):
    with wave.open(str(path),'wb') as w:w.setparams((1,2,22050,0,'NONE','not compressed'));w.writeframes(struct.pack('<h',100)*frames)
@unittest.skipUnless(shutil.which('ffplay'),'external FFplay unavailable')
class NativePlayerTests(unittest.TestCase):
    def test_actual_dummy_output_finishes_with_explicit_capability_boundary(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d);src=root/'s.wav';sample(src,2205);player=native.NativePlayer(executable=shutil.which('ffplay'),audio_driver='dummy')
            caps=player.capabilities();self.assertTrue(caps['buffered_file_playback']);self.assertFalse(caps['physical_output_verified']);self.assertEqual(caps['audio_driver'],'dummy');player.start(src,root=root,deadline=5)
            until=time.monotonic()+6
            while player.poll()=='playing' and time.monotonic()<until:time.sleep(.02)
            self.assertEqual(player.poll(),'finished',getattr(player,'error',None));player.dispose();self.assertEqual(player.poll(),'disposed')
    def test_pause_resume_stop_and_deadline_own_the_child(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d);src=root/'s.wav';sample(src,44100);player=native.NativePlayer(executable=shutil.which('ffplay'),audio_driver='dummy');player.start(src,root=root,deadline=5);player.pause();self.assertEqual(player.poll(),'paused');player.resume();self.assertEqual(player.poll(),'playing');player.stop();self.assertEqual(player.poll(),'stopped');player.start(src,root=root,deadline=.05)
            until=time.monotonic()+2
            while player.poll()=='playing' and time.monotonic()<until:time.sleep(.01)
            self.assertEqual(player.poll(),'timed_out');player.dispose()
    def test_root_and_byte_budget_fail_before_player_start(self):
        with tempfile.TemporaryDirectory() as d,tempfile.TemporaryDirectory() as other:
            root=pathlib.Path(d);src=pathlib.Path(other)/'s.wav';sample(src,5);player=native.NativePlayer(executable=shutil.which('ffplay'),audio_driver='dummy')
            with self.assertRaises(ValueError):player.start(src,root=root)
            src=root/'large.wav';sample(src,20)
            with self.assertRaises(ValueError):player.start(src,root=root,max_file_bytes=45)
            player.dispose()
if __name__=='__main__':unittest.main()
