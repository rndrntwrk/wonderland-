import importlib.util,json,os,pathlib,shutil,struct,subprocess,sys,tempfile,time,unittest,wave
spec=importlib.util.spec_from_file_location('audio_cooker',pathlib.Path(__file__).parents[1]/'cooker.py');cooker=importlib.util.module_from_spec(spec);spec.loader.exec_module(cooker)
DECODER=os.environ.get('AUDIO_DECODER')
@unittest.skipUnless(DECODER,'AUDIO_DECODER executable must be supplied')
class CookerTests(unittest.TestCase):
    def test_wave_cooking_and_provenance_are_measured_and_atomic(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d);src=root/'in.wav';out=root/'cooked.wav'
            with wave.open(str(src),'wb') as w:w.setparams((1,2,22050,3,'NONE','not compressed'));w.writeframes(struct.pack('<hhh',-100,0,100))
            report=cooker.cook(src,out,root=root,provenance='synthetic',decoder=DECODER)
            self.assertEqual(report['sample_frames'],3);self.assertEqual(report['channels'],1);self.assertEqual(report['status'],'decoded');self.assertEqual(report['provenance'],'synthetic');self.assertTrue(out.exists());self.assertEqual(json.loads(out.with_suffix('.wav.json').read_text()),report)
            with self.assertRaises(FileExistsError):cooker.cook(src,out,root=root,provenance='synthetic',decoder=DECODER)
    def test_xa_requires_normalized_provider_metadata_and_truncation_fails(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d);src=root/'in.xa';out=root/'out.wav';src.write_bytes(b'XAI\0'+struct.pack('<IHHIIHH',56,1,1,22050,44100,2,16)+bytes([0])+bytes([0x12])*14)
            with self.assertRaisesRegex(ValueError,'metadata'):cooker.cook(src,out,root=root,provenance='synthetic',decoder=DECODER)
            metadata={'encoding':'XaSpeech','format':{'sample_rate':22050,'channels':1,'bits_per_sample':16},'sample_frames':28,'payload_offset':24,'payload_bytes':15}
            report=cooker.cook(src,out,root=root,provenance='synthetic',decoder=DECODER,metadata=metadata);self.assertEqual(report['sample_frames'],28)
            out.unlink();out.with_suffix('.wav.json').unlink();src.write_bytes(src.read_bytes()[:-1])
            with self.assertRaises(RuntimeError):cooker.cook(src,out,root=root,provenance='synthetic',decoder=DECODER,metadata=metadata)
            self.assertFalse(out.exists());self.assertFalse(out.with_suffix('.wav.json').exists())
    def test_outside_root_and_unsupported_mp3_do_not_create_outputs(self):
        with tempfile.TemporaryDirectory() as d,tempfile.TemporaryDirectory() as outside:
            root=pathlib.Path(d);src=pathlib.Path(outside)/'in.mp3';src.write_bytes(b'ID3'+bytes(100));out=root/'out.wav'
            with self.assertRaises(ValueError):cooker.cook(src,out,root=root,provenance='synthetic',decoder=DECODER)
            link=root/'link.mp3';link.symlink_to(src)
            with self.assertRaises(ValueError):cooker.cook(link,out,root=root,provenance='synthetic',decoder=DECODER)
            local=root/'in.mp3';local.write_bytes(src.read_bytes())
            with self.assertRaisesRegex(ValueError,'external'):cooker.cook(local,out,root=root,provenance='synthetic',decoder=DECODER)
            self.assertFalse(out.exists())
    def test_process_deadline_and_output_budget_kill_child(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d);start=time.monotonic()
            with self.assertRaisesRegex(RuntimeError,'deadline'):cooker.run_bounded([sys.executable,'-c','import time;time.sleep(10)'],root,timeout=.1,max_file_bytes=1024)
            self.assertLess(time.monotonic()-start,2)
            with self.assertRaises(RuntimeError):cooker.run_bounded([sys.executable,'-c','import sys;sys.stdout.write("x"*100000)'],root,timeout=2,max_file_bytes=1024)
    def test_external_decoder_refuses_playlist_inputs_before_starting_a_process(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d);src=root/'input.m3u8';out=root/'out.wav';src.write_bytes(b'#EXTM3U\\nfile:///outside/audio.wav\\n')
            with self.assertRaisesRegex(ValueError,'MP3 bytes'):
                cooker.cook(src,out,root=root,provenance='synthetic',decoder=DECODER,ffmpeg='/must/not/be/invoked')
            self.assertFalse(out.exists())
    def test_fast_exiting_child_cannot_bypass_diagnostic_budget(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaisesRegex(RuntimeError,'diagnostic'):
                cooker.run_bounded([sys.executable,'-c','import sys;sys.stdout.write("x"*100000)'],pathlib.Path(d),timeout=2,max_file_bytes=1024*1024)
    @unittest.skipUnless(shutil.which('ffmpeg'),'installed external FFmpeg unavailable')
    def test_explicit_external_mp3_codec_decodes_real_synthetic_payload(self):
        with tempfile.TemporaryDirectory() as d:
            root=pathlib.Path(d);src=root/'in.wav';mp3=root/'in.mp3';out=root/'out.wav'
            with wave.open(str(src),'wb') as w:w.setparams((1,2,22050,0,'NONE','not compressed'));w.writeframes(struct.pack('<h',1000)*2205)
            subprocess.run(['ffmpeg','-v','error','-i',str(src),str(mp3)],check=True,timeout=10)
            report=cooker.cook(mp3,out,root=root,provenance='synthetic',decoder=DECODER,ffmpeg=shutil.which('ffmpeg'));self.assertEqual(report['sample_rate'],22050);self.assertGreater(report['sample_frames'],0);self.assertEqual(report['codec_backend'],'external-ffmpeg');self.assertIn('ffmpeg version',report['external_version'])
if __name__=='__main__':unittest.main()
