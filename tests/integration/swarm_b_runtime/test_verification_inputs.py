"""Bind original source-oracle inputs to aggregate verification results."""
import importlib.util
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "swarm_b_verifier", ROOT / "tools/swarm-b/verify.py"
)
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)


class VerificationInputTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="verification-input-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        original_root = VERIFIER.ROOT
        self.addCleanup(setattr, VERIFIER, "ROOT", original_root)
        VERIFIER.ROOT = self.root

    def test_original_reader_and_asset_mutations_change_the_digest(self):
        for relative in (
            "TSOClient/FSO.Content.TSO/Content/Objects/source.iff",
            "Other/tools/Iffinator/Iffinator/srcs.zip",
            "TSOClient/tso.files/Formats/IFF/Chunks/SPR2.cs",
            "TSOClient/tso.files/Formats/IFF/Chunks/SPR2FrameEncoder.cs",
            "TSOClient/tso.files/FAR3/FAR3Archive.cs",
            "TSOClient/tso.files/FAR3/Far3Entry.cs",
            "TSOClient/tso.files/FAR3/FAR3Exception.cs",
            "TSOClient/tso.files/FAR3/Decompresser.cs",
        ):
            with self.subTest(source=relative):
                path = self.root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"source before its oracle gate")
                before = VERIFIER.source_digest()
                path.write_bytes(b"source changed after its oracle gate")
                self.assertNotEqual(before, VERIFIER.source_digest())

    def test_added_and_removed_original_corpus_inputs_change_the_digest(self):
        path = self.root / "TSOClient/FSO.Content.TSO/Content/Objects/added.iff"
        path.parent.mkdir(parents=True)
        before = VERIFIER.source_digest()
        path.write_bytes(b"new source input")
        added = VERIFIER.source_digest()
        self.assertNotEqual(before, added)
        path.unlink()
        self.assertEqual(before, VERIFIER.source_digest())


if __name__ == "__main__":
    unittest.main()
