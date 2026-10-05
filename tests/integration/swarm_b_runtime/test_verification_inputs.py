"""Bind original source-oracle inputs to aggregate verification results."""
import importlib.util
import json
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

    def test_every_executed_original_eod_dependency_changes_the_digest(self):
        manifest = json.loads((ROOT / "fixtures/eod/source-oracle/sources.json").read_text())
        for entry in manifest["files"]:
            with self.subTest(source=entry["path"]):
                path = self.root / entry["path"]
                path.parent.mkdir(parents=True, exist_ok=True)
                before = VERIFIER.source_digest()
                path.write_bytes(b"unchanged original EOD source input")
                added = VERIFIER.source_digest()
                self.assertNotEqual(before, added)
                path.write_bytes(b"original source changed after comparison")
                self.assertNotEqual(added, VERIFIER.source_digest())
                path.unlink()
                self.assertEqual(before, VERIFIER.source_digest())

    def test_original_eod_symlink_target_bytes_are_bound(self):
        manifest = json.loads((ROOT / "fixtures/eod/source-oracle/sources.json").read_text())
        path = self.root / manifest["files"][0]["path"]
        path.parent.mkdir(parents=True, exist_ok=True)
        target = self.root / "synthetic-original-input.cs"
        target.write_bytes(b"original bytes")
        path.symlink_to(target)
        before = VERIFIER.source_digest()
        target.write_bytes(b"different bytes supplied to original compiler")
        self.assertNotEqual(before, VERIFIER.source_digest())

    def test_generated_browser_output_is_excluded_but_browser_sources_are_bound(self):
        before = VERIFIER.source_digest()
        for relative in (
            "tools/creator-web/dist/pkg/generated.wasm",
            "tools/creator-web/node_modules/playwright/generated.js",
            "tools/creator-web/test-results/browser/report.json",
        ):
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"a local build or test result")
        self.assertEqual(before, VERIFIER.source_digest())
        for relative in (
            "tools/creator-web/public/fonts/inter-latin-400-normal.woff2",
            "tools/creator-web/package-lock.json",
            "tools/creator-web/src/session.rs",
            "tools/creator-web/tests/browser.mjs",
        ):
            with self.subTest(source=relative):
                path = self.root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"a real browser source input")
                current = VERIFIER.source_digest()
                self.assertNotEqual(before, current)
                before = current

    def test_added_avatar_and_legacy_text_inputs_are_bound(self):
        for relative in (
            "TSOClient/FSO.Content.TSO/Content/Avatar/Meshes/source.mesh",
            "TSOClient/tso.content/Content/MeshReplace/source.fsom",
            "TSOClient/tso.files/Utils/BCFReadProxy.cs",
            "TSOClient/tso.vitaboy.model/CFP.cs",
        ):
            with self.subTest(source=relative):
                path = self.root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                before = VERIFIER.source_digest()
                path.write_bytes(b"an original format input")
                self.assertNotEqual(before, VERIFIER.source_digest())
                path.unlink()
                self.assertEqual(before, VERIFIER.source_digest())

    def test_each_family_manifest_binds_all_original_dependencies(self):
        for manifest_path in (ROOT / "fixtures/eod").glob("*/sources.json"):
            manifest = json.loads(manifest_path.read_text())
            local_manifest = self.root / manifest_path.relative_to(ROOT)
            local_manifest.parent.mkdir(parents=True, exist_ok=True)
            local_manifest.write_text(manifest_path.read_text())
            for entry in manifest.get("files", manifest.get("sources", [])):
                with self.subTest(manifest=manifest_path.parent.name, source=entry["path"]):
                    path = self.root / entry["path"]
                    path.parent.mkdir(parents=True, exist_ok=True)
                    before = VERIFIER.source_digest()
                    path.write_bytes(b"an original family dependency")
                    self.assertNotEqual(before, VERIFIER.source_digest())
                    path.unlink()
                    self.assertEqual(before, VERIFIER.source_digest())


if __name__ == "__main__":
    unittest.main()
