"""Source staging tests. Does not build or launch Horizon."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("native_tools", ROOT / "tools" / "native.py")
NATIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NATIVE)


class BundleTests(unittest.TestCase):
    def setUp(self):
        target = ROOT / "target"
        target.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="bundle-test-", dir=target)
        self.root = Path(self.temporary.name).resolve()
        assert self.root.is_relative_to(target.resolve())
        for name in ["native", "src", ".upstream"]:
            (self.root / name).mkdir()
        (self.root / "Cargo.toml").write_text('[package]\nname="test"\nversion="0.1.0"\n')
        (self.root / "src" / "lib.rs").write_text("pub fn sample() {}\n")
        (self.root / "native" / "horizon-mixin.toml").write_text('[mod]\nid="test"\n')
        (self.root / ".upstream" / "private-data.txt").write_text("must never enter bundle")
        (self.root / "native" / "recording.hfr").write_text("must never enter bundle")
        self.original_root = NATIVE.ROOT
        NATIVE.ROOT = self.root

    def tearDown(self):
        NATIVE.ROOT = self.original_root
        self.temporary.cleanup()

    def test_only_source_allowlist_is_staged(self):
        fixtures = self.root / "tests/fixtures"
        fixtures.mkdir(parents=True)
        (fixtures / "interpolation-java.json").write_text('{"numeric_reference": true}')
        (fixtures / "private.json").write_text("must never enter bundle")
        bundle = NATIVE.bundle()
        paths = {path.relative_to(bundle).as_posix() for path in bundle.rglob("*") if path.is_file()}
        self.assertEqual(paths, {
            "flashback/horizon-mixin.toml", "flashback/engine/Cargo.toml", "flashback/engine/src/lib.rs",
            "flashback/engine/tests/fixtures/interpolation-java.json"
        })
        self.assertEqual(NATIVE.bundle(), bundle)

    def test_source_edits_get_new_immutable_bundle(self):
        before = NATIVE.bundle()
        (self.root / "src" / "lib.rs").write_text("pub fn changed() {}\n")
        after = NATIVE.bundle()
        self.assertNotEqual(before, after)
        self.assertIn("sample", (before / "flashback/engine/src/lib.rs").read_text())
        self.assertIn("changed", (after / "flashback/engine/src/lib.rs").read_text())

    def test_modified_staged_bundle_is_rejected(self):
        bundle = NATIVE.bundle()
        (bundle / "flashback/engine/src/lib.rs").write_text("unexpected edit")
        with self.assertRaises(ValueError):
            NATIVE.bundle()


if __name__ == "__main__":
    unittest.main()
