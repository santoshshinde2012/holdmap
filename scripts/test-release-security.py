#!/usr/bin/env python3
"""Exercise release context handling without calling dist or publishing artifacts."""
import json
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


class ReleaseSecurityTests(unittest.TestCase):
    def setUp(self):
        self.sandbox = tempfile.TemporaryDirectory(prefix="holdmap-release-test-")
        self.addCleanup(self.sandbox.cleanup)
        self.directory = Path(self.sandbox.name)
        self.record = self.directory / "arguments.json"
        stub = self.directory / "dist"
        stub.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "from pathlib import Path\n"
            "Path(os.environ['HOLDMAP_TEST_RECORD']).write_text(json.dumps(sys.argv[1:]))\n"
            "print('{}')\n"
        )
        stub.chmod(0o700)

    def run_plan(self, event, tag, ref_type="tag"):
        environment = os.environ.copy()
        environment.update({
            "PATH": str(self.directory) + os.pathsep + environment["PATH"],
            "GITHUB_EVENT_NAME": event,
            "GITHUB_REF_NAME": tag,
            "GITHUB_REF_TYPE": ref_type,
            "HOLDMAP_TEST_RECORD": str(self.record),
        })
        return subprocess.run(
            ["bash", str(ROOT / "scripts/release-plan.sh")],
            env=environment, capture_output=True, text=True, check=False,
        )

    def test_pull_request_only_plans_even_with_hostile_ref(self):
        result = self.run_plan("pull_request", "$(false)", "branch")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(self.record.read_text()), ["plan", "--output-format=json"])

    def test_release_and_prerelease_are_literal_arguments(self):
        for tag in ["v0.3.0", "v1.2.3-rc.1"]:
            with self.subTest(tag=tag):
                result = self.run_plan("push", tag)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(self.record.read_text()),
                                 ["host", "--steps=create", "--tag", tag, "--output-format=json"])

    def test_git_valid_shell_payload_is_rejected_before_dist(self):
        marker = self.directory / "executed"
        tag = "v0.3.0$(touch${IFS}" + str(marker) + ")"
        valid = subprocess.run(["git", "check-ref-format", "refs/tags/" + tag], check=False)
        self.assertEqual(valid.returncode, 0, "fixture must be a legal Git tag")
        result = self.run_plan("push", tag)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(marker.exists())
        self.assertFalse(self.record.exists())

    def test_unexpected_events_refs_and_tags_do_not_call_dist(self):
        for event, tag, ref_type in [
            ("workflow_dispatch", "v0.3.0", "tag"),
            ("push", "v0.3.0", "branch"),
            ("push", "--help", "tag"),
            ("push", "v0.3.0\ncontents=write", "tag"),
        ]:
            with self.subTest(event=event, tag=tag, ref_type=ref_type):
                result = self.run_plan(event, tag, ref_type)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.record.exists())


class DesktopReleaseAssetTests(unittest.TestCase):
    def collect(self, matrix, names, updater, mutate=None):
        sandbox = tempfile.TemporaryDirectory(prefix="holdmap-desktop-release-test-")
        self.addCleanup(sandbox.cleanup)
        root = Path(sandbox.name)
        paths = []
        for name in names:
            path = root / "target/release/bundle" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(name.encode())
            paths.append(str(path))
        if mutate:
            mutate(root, paths)
        environment = os.environ.copy()
        environment.update({"RELEASE_MATRIX": matrix, "RELEASE_VERSION": "0.4.0",
                            "RELEASE_UPDATER": str(updater).lower(),
                            "TAURI_ARTIFACT_PATHS": json.dumps(paths), "GITHUB_OUTPUT": str(root / "output")})
        result = subprocess.run(["node", str(ROOT / "scripts/collect-desktop-release-assets.mjs")],
                                cwd=root, env=environment, capture_output=True, text=True, check=False)
        return root, result

    def assert_manifest(self, root, matrix, names):
        manifest = (root / f"holdmap-desktop-{matrix}.sha256").read_text().splitlines()
        self.assertEqual({line.split("  ", 1)[1] for line in manifest}, set(names))
        paths = (root / f"holdmap-desktop-{matrix}.sha256.paths").read_text().splitlines()
        self.assertEqual({Path(path).name for path in paths}, set(names))
        for line in manifest:
            expected, name = line.split("  ", 1)
            path = root / next(path for path in paths if Path(path).name == name)
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), expected)

    def test_unsigned_release_only_collects_installers(self):
        root, result = self.collect("macos-arm64", ["dmg/holdmap_0.4.0_aarch64.dmg",
                                    "macos/holdmap.app.tar.gz", "macos/holdmap.app.tar.gz.sig"], False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_manifest(root, "macos-arm64", ["holdmap_0.4.0_aarch64.dmg"])

    def test_signed_mac_archive_and_signature_use_published_names(self):
        for matrix, architecture in [("macos-arm64", "aarch64"), ("macos-x64", "x64")]:
            with self.subTest(matrix=matrix):
                root, result = self.collect(matrix, [f"dmg/holdmap_0.4.0_{architecture}.dmg",
                                            "macos/holdmap.app.tar.gz", "macos/holdmap.app.tar.gz.sig"], True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assert_manifest(root, matrix, [f"holdmap_0.4.0_{architecture}.dmg",
                                     f"holdmap_0.4.0_{architecture}.app.tar.gz",
                                     f"holdmap_0.4.0_{architecture}.app.tar.gz.sig"])

    def test_windows_and_linux_updater_files_are_all_covered(self):
        fixtures = {"windows-x64": ["msi/holdmap_0.4.0_x64_en-US.msi", "msi/holdmap_0.4.0_x64_en-US.msi.sig",
                    "nsis/holdmap_0.4.0_x64-setup.exe", "nsis/holdmap_0.4.0_x64-setup.exe.sig",
                    "nsis/holdmap_0.4.0_x64-setup.nsis.zip", "nsis/holdmap_0.4.0_x64-setup.nsis.zip.sig"],
                    "linux-x64": ["deb/holdmap_0.4.0_amd64.deb", "rpm/holdmap-0.4.0-1.x86_64.rpm",
                    "appimage/holdmap_0.4.0_amd64.AppImage", "appimage/holdmap_0.4.0_amd64.AppImage.sig",
                    "appimage/holdmap_0.4.0_amd64.AppImage.tar.gz", "appimage/holdmap_0.4.0_amd64.AppImage.tar.gz.sig"]}
        for matrix, names in fixtures.items():
            with self.subTest(matrix=matrix):
                root, result = self.collect(matrix, names, True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assert_manifest(root, matrix, [Path(name).name for name in names])

    def test_unsigned_updater_archive_fails_before_manifest(self):
        root, result = self.collect("macos-arm64", ["dmg/holdmap_0.4.0_aarch64.dmg",
                                    "macos/holdmap.app.tar.gz"], True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((root / "holdmap-desktop-macos-arm64.sha256").exists())

    def test_outside_and_symlink_inputs_cannot_create_manifest(self):
        def outside(root, paths):
            path = root / "outside.dmg"
            path.write_bytes(b"outside")
            paths.append(str(path))

        def linked_file(root, paths):
            path = root / "target/link.dmg"
            path.symlink_to(paths[0])
            paths.append(str(path))

        def linked_parent(root, paths):
            directory = root / "outside"
            directory.mkdir()
            (directory / "escape.dmg").write_bytes(b"outside")
            (root / "target/link").symlink_to(directory, target_is_directory=True)
            paths.append(str(root / "target/link/escape.dmg"))

        for mutate in [outside, linked_file, linked_parent]:
            with self.subTest(case=mutate.__name__):
                root, result = self.collect("macos-arm64", ["dmg/holdmap_0.4.0_aarch64.dmg"], False, mutate)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((root / "holdmap-desktop-macos-arm64.sha256").exists())

    def test_normalized_name_collisions_cannot_create_manifest(self):
        root, result = self.collect("macos-arm64", ["dmg/holdmap 0.4.0.dmg", "dmg/holdmap.0.4.0.dmg"], False)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((root / "holdmap-desktop-macos-arm64.sha256").exists())

    def test_staging_does_not_reuse_restored_symlink(self):
        def old_staging(root, paths):
            outside = root / "outside"
            outside.mkdir()
            (root / "target/desktop-release-assets-macos-arm64").symlink_to(outside, target_is_directory=True)

        root, result = self.collect("macos-arm64", ["dmg/holdmap_0.4.0_aarch64.dmg"], False, old_staging)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_manifest(root, "macos-arm64", ["holdmap_0.4.0_aarch64.dmg"])
        self.assertEqual(list((root / "outside").iterdir()), [])


if __name__ == "__main__":
    unittest.main()
