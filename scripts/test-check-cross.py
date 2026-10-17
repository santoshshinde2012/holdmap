#!/usr/bin/env python3
"""Check cross-check isolation without installing targets or invoking compilers."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class CrossCheckTests(unittest.TestCase):
    def setUp(self):
        sandbox = tempfile.TemporaryDirectory(prefix="holdmap-cross-test-")
        self.addCleanup(sandbox.cleanup)
        self.root = Path(sandbox.name).resolve()
        self.source = self.root / "source checkout"
        self.source.mkdir()
        (self.source / "scripts").mkdir()
        shutil.copy2(ROOT / "scripts/check-cross.sh", self.source / "scripts/check-cross.sh")
        (self.source / "Cargo.toml").write_text(
            '[workspace]\nmembers = []\n[patch.crates-io]\n'
            'glib = { path = "vendor/glib" }\n'
            'global-hotkey = { path = "vendor/global-hotkey" }\n'
        )
        (self.source / "Cargo.lock").write_text("original lockfile\n")
        (self.source / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "stable"\n')
        for directory in ["crates/holdmap-core/src", "vendor/glib", "vendor/global-hotkey", "apps/desktop/src-tauri"]:
            (self.source / directory).mkdir(parents=True)
        (self.source / "crates/holdmap-core/src/lib.rs").write_text("// uncommitted source\n")
        self.netstat = self.root / "netstat2-0.11.0"
        (self.netstat / "src/types").mkdir(parents=True)
        (self.netstat / "src/types/mod.rs").write_text("// locked dependency types\n")
        (self.netstat / "Cargo.toml").write_text('[package]\nname = "netstat2"\nversion = "0.11.0"\n')
        self.calls = self.root / "calls.jsonl"
        self.tools = self.root / "tools"
        self.tools.mkdir()
        (self.tools / "rustup").write_text("#!/usr/bin/env bash\nexit 0\n")
        (self.tools / "cargo").write_text('''#!/usr/bin/env python3
import json, os, pathlib, sys, tomllib
root = pathlib.Path.cwd()
args = sys.argv[1:]
with pathlib.Path(os.environ["HOLDMAP_TEST_CALLS"]).open("a") as log:
    log.write(json.dumps({"cwd": str(root), "args": args}) + "\\n")
manifest = tomllib.loads((root / "Cargo.toml").read_text())
assert set(manifest["patch"]["crates-io"]) == {"glib", "global-hotkey"}
assert (root / "crates/holdmap-core/src/lib.rs").read_text() == "// uncommitted source\\n"
if args[0] == "metadata":
    print(json.dumps({"packages": [{"name": "netstat2", "manifest_path": os.environ["HOLDMAP_TEST_NETSTAT"]}]}))
elif args[0] == "clippy":
    # Core is checked as a library on its own, then the other crates with all targets.
    assert "holdmap-activity" in args or ("holdmap-core" in args and "--lib" in args)
    target = args[args.index("--target") + 1]
    if target == "aarch64-apple-darwin":
        override = args[args.index("--config") + 1]
        stub = pathlib.Path(tomllib.loads(override)["patch"]["crates-io"]["netstat2"]["path"])
        assert stub.is_relative_to(root)
        assert (stub / "src/types/mod.rs").read_text() == "// locked dependency types\\n"
    else:
        assert "--locked" in args
    (root / "Cargo.lock").write_text("compiler lockfile update\\n")
    if target == os.environ.get("HOLDMAP_TEST_FAIL_TARGET"):
        sys.exit(19)
''')
        for tool in self.tools.iterdir():
            tool.chmod(0o700)

    def run_check(self, fail_target=""):
        original = {name: (self.source / name).read_bytes() for name in ["Cargo.toml", "Cargo.lock"]}
        environment = os.environ.copy()
        environment.update({
            "PATH": str(self.tools) + os.pathsep + environment["PATH"],
            "TMPDIR": str(self.root),
            "HOLDMAP_TEST_CALLS": str(self.calls),
            "HOLDMAP_TEST_NETSTAT": str(self.netstat / "Cargo.toml"),
            "HOLDMAP_TEST_FAIL_TARGET": fail_target,
        })
        result = subprocess.run(["bash", str(self.source / "scripts/check-cross.sh")],
                                env=environment, capture_output=True, text=True, timeout=30)
        for name, content in original.items():
            self.assertEqual((self.source / name).read_bytes(), content)
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        for call in calls:
            self.assertNotEqual(Path(call["cwd"]), self.source)
            self.assertFalse(Path(call["cwd"]).exists(), "temporary source must be cleaned up")
        return result, calls

    def test_both_targets_use_current_source_and_preserve_existing_patches(self):
        result, calls = self.run_check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sum(call["args"][0] == "clippy" for call in calls), 4)

    def test_failed_checks_never_restore_over_the_working_checkout(self):
        for target in ["x86_64-pc-windows-gnu", "aarch64-apple-darwin"]:
            with self.subTest(target=target):
                self.calls.unlink(missing_ok=True)
                result, _ = self.run_check(target)
                self.assertEqual(result.returncode, 19, result.stderr)


if __name__ == "__main__":
    unittest.main()
