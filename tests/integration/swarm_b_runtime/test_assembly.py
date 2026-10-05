"""Exercise assembly path protection against the actual pinned Git subtree."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "runtime_bridge_assembly", ROOT / "tools/swarm-b/runtime-bridge.py"
)
ASSEMBLY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ASSEMBLY)


class AssemblyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="runtime-assembly-test-")
        self.addCleanup(self.temporary.cleanup)
        self.scratch = Path(self.temporary.name)
        self.destination = self.scratch / "assembly"
        self.manifest = ASSEMBLY.assemble(ROOT, self.destination)

    def test_exact_reassembly_succeeds(self):
        self.assertEqual(ASSEMBLY.assemble(ROOT, self.destination), self.manifest)

    def test_manifest_symlink_never_overwrites_external_file(self):
        sentinel = self.scratch / "external-manifest"
        sentinel.write_bytes(b"external manifest must remain unchanged")
        self.manifest.unlink()
        self.manifest.symlink_to(sentinel)
        with self.assertRaises(ValueError):
            ASSEMBLY.assemble(ROOT, self.destination)
        self.assertEqual(sentinel.read_bytes(), b"external manifest must remain unchanged")

    def test_parent_symlink_is_rejected_before_refresh(self):
        owned = self.destination / "crates"
        external = self.scratch / "external-crates"
        owned.rename(external)
        owned.symlink_to(external, target_is_directory=True)
        sentinel = external / "content-runtime-bridge/Cargo.toml"
        sentinel.write_bytes(b"external parent must remain unchanged")
        with self.assertRaises(ValueError):
            ASSEMBLY.assemble(ROOT, self.destination)
        self.assertEqual(sentinel.read_bytes(), b"external parent must remain unchanged")

    def test_manifest_and_lock_hardlinks_never_overwrite_external_files(self):
        for name in ("Cargo.toml", "Cargo.lock"):
            with self.subTest(name=name):
                target = self.manifest.parent / name
                sentinel = self.scratch / ("external-" + name)
                original = target.read_bytes()
                target.rename(sentinel)
                os.link(sentinel, target)
                with self.assertRaises(ValueError):
                    ASSEMBLY.assemble(ROOT, self.destination)
                self.assertEqual(sentinel.read_bytes(), original)
                target.unlink()
                sentinel.rename(target)

    def test_destination_symlink_is_rejected(self):
        alias = self.scratch / "alias"
        alias.symlink_to(self.destination, target_is_directory=True)
        with self.assertRaises(ValueError):
            ASSEMBLY.assemble(ROOT, alias)

    def test_stamp_symlink_is_rejected(self):
        stamp = self.destination / "assembly.json"
        external = self.scratch / "external-stamp"
        stamp.rename(external)
        stamp.symlink_to(external)
        with self.assertRaises(ValueError):
            ASSEMBLY.assemble(ROOT, self.destination)

    def test_extra_simulation_build_script_is_rejected(self):
        (self.destination / "crates/sim-core/build.rs").write_text("fn main() {}\n")
        with self.assertRaises(ValueError):
            ASSEMBLY.assemble(ROOT, self.destination)

    def test_simulation_source_drift_is_rejected(self):
        (self.destination / "crates/sim-core/src/lib.rs").write_text("// drift\n")
        with self.assertRaises(ValueError):
            ASSEMBLY.assemble(ROOT, self.destination)

    def test_invalid_late_link_is_rejected_before_manifest_refresh(self):
        self.manifest.write_bytes(b"owned manifest before failed preflight")
        target = self.destination / "TSOClient"
        target.unlink()
        target.mkdir()
        with self.assertRaises(ValueError):
            ASSEMBLY.assemble(ROOT, self.destination)
        self.assertEqual(self.manifest.read_bytes(), b"owned manifest before failed preflight")


if __name__ == "__main__":
    unittest.main()
