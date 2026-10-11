"""Synthetic decoder CLI tests. Set AUDIO_DECODER to the compiled Rust binary."""
import os, pathlib, subprocess, tempfile, unittest, wave
DECODER = os.environ.get('AUDIO_DECODER')
@unittest.skipUnless(DECODER, 'AUDIO_DECODER executable must be supplied')
class DecoderCliTests(unittest.TestCase):
    def test_pcm16_roundtrip_and_metadata_rejection(self):
        with tempfile.TemporaryDirectory() as d:
            source=pathlib.Path(d)/'raw.pcm';target=pathlib.Path(d)/'out.wav'
            source.write_bytes(b'\0\x80\0\0\xff\x7f')
            args=[DECODER,'pcm','22050','1','16','3','0','6',str(source),str(target)]
            result=subprocess.run(args,capture_output=True,timeout=5)
            self.assertEqual(result.returncode,0,result.stderr.decode())
            with wave.open(str(target),'rb') as output:
                self.assertEqual((output.getnchannels(),output.getframerate(),output.getnframes()),(1,22050,3))
                self.assertEqual(output.readframes(3),source.read_bytes())
            target.unlink();args[5]='18446744073709551615'
            result=subprocess.run(args,capture_output=True,timeout=5)
            self.assertNotEqual(result.returncode,0);self.assertFalse(target.exists())
if __name__=='__main__':unittest.main()
