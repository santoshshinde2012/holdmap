#!/usr/bin/env python3
"""Exercise release context handling without calling dist or publishing artifacts."""
import json
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parent.parent


class ReleaseSecurityTests(unittest.TestCase):
    def test_distribution_action_pins_match_the_release_workflow(self):
        config = tomllib.loads((ROOT / "dist-workspace.toml").read_text())
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        for action, expected in config["dist"]["github-action-commits"].items():
            with self.subTest(action=action):
                pins = re.findall(r"(?m)^\s*-?\s*uses:\s*" + re.escape(action)
                                  + r"@([0-9a-f]{40})(?:\s|$)", workflow)
                self.assertTrue(pins, f"Release workflow must use configured action {action}")
                self.assertEqual(set(pins), {expected})

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


class ReleaseNotesTests(unittest.TestCase):
    before = "## Release Notes\n\nFix literal `$()` and preserve [links](https://example.test/).\n\n"
    npm = "### Install prebuilt binaries into your npm project\n\n```sh\nnpm install holdmap@0.5.0\n```\n"
    after = "\n## Download holdmap 0.5.0\n\n| File | Platform |\n| --- | --- |\n| holdmap.zip | Windows |\n"

    def prepare(self, notes, tag="v0.5.0", repository="santoshshinde2012/holdmap", published=False):
        args = [sys.executable, str(ROOT / "scripts/prepare-release-notes.py"),
                "--tag", tag, "--repo", repository]
        if published:
            args.append("--npm-registry-published")
        return subprocess.run(args, input=notes.encode("utf-8") if isinstance(notes, str) else notes,
                              capture_output=True, check=False)

    def test_realistic_dist_block_becomes_pinned_attached_package_download(self):
        result = self.prepare(self.before + self.npm + self.after)
        self.assertEqual(result.returncode, 0, result.stderr)
        replacement = (
            "### Download the npm package\n\n"
            "The npm package is attached to this release; it is not published to the npm registry.\n\n"
            "[Download holdmap-npm-package.tar.gz](https://github.com/santoshshinde2012/holdmap/"
            "releases/download/v0.5.0/holdmap-npm-package.tar.gz)\n"
        )
        self.assertEqual(result.stdout.decode("utf-8"), self.before + replacement + self.after)
        self.assertNotIn(b"npm install holdmap@", result.stdout)

    def test_notes_without_the_generated_section_remain_byte_identical(self):
        for notes in [self.before + self.after, "## Notes\r\n\r\nUnicode → ✓\r\n", "No final newline"]:
            with self.subTest(notes=notes):
                result = self.prepare(notes)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, notes.encode("utf-8"))

    def test_prerelease_and_crlf_preserve_surrounding_notes(self):
        notes = (self.before + self.npm + self.after).replace("0.5.0", "0.5.0-rc.1").replace("\n", "\r\n")
        result = self.prepare(notes, tag="v0.5.0-rc.1", repository="example-org/holdmap.test")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(result.stdout.startswith(self.before.replace("\n", "\r\n").encode()))
        self.assertTrue(result.stdout.endswith(self.after.replace("0.5.0", "0.5.0-rc.1").replace("\n", "\r\n").encode()))
        self.assertIn(b"https://github.com/example-org/holdmap.test/releases/download/v0.5.0-rc.1/holdmap-npm-package.tar.gz", result.stdout)
        self.assertNotIn(b"\n", result.stdout.replace(b"\r\n", b""))

    def test_malformed_or_hostile_tags_emit_no_notes(self):
        for tag in ["", "0.5.0", "v0.5", "v0.5.0$(touch${IFS}marker)", "v0.5.0\ncontents=write", "--help"]:
            with self.subTest(tag=tag):
                result = self.prepare(self.before + self.npm, tag=tag)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")
                self.assertIn(b"error:", result.stderr)

    def test_invalid_repositories_cannot_enter_the_download_url(self):
        for repository in ["", "holdmap", "../holdmap", "example/..", "example/.",
                           "example/holdmap/extra", "example/holdmap?query=1", "example/holdmap\nnext", "example/$(false)"]:
            with self.subTest(repository=repository):
                result = self.prepare(self.npm, repository=repository)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")
                self.assertIn(b"repository", result.stderr)

    def test_changed_generated_sections_fail_instead_of_retaining_a_false_claim(self):
        for notes in [self.npm.replace("0.5.0", "0.4.0"), self.npm.replace("```sh", "```bash"),
                      self.npm.replace("npm install holdmap@0.5.0", "npm install holdmap@0.5.0 --global"),
                      self.npm + "\n" + self.npm]:
            with self.subTest(notes=notes):
                result = self.prepare(notes)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")

    def test_malformed_note_bytes_fail_without_partial_output(self):
        for notes in [b"invalid UTF-8 \xff", b"invalid NUL \x00"]:
            with self.subTest(notes=notes):
                result = self.prepare(notes)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")
                self.assertIn(b"error:", result.stderr)

    def test_future_registry_publication_requires_an_explicit_opt_in(self):
        notes = self.before + self.npm + self.after
        result = self.prepare(notes, published=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, notes.encode("utf-8"))


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

    def test_built_version_must_match_the_release_tag_before_collecting(self):
        prior = os.environ.get("TAG")
        try:
            os.environ["TAG"] = "v0.3.0"
            root, result = self.collect("macos-arm64", ["dmg/holdmap_0.4.0_aarch64.dmg"], False)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / "holdmap-desktop-macos-arm64.sha256").exists())
        finally:
            if prior is None:
                os.environ.pop("TAG", None)
            else:
                os.environ["TAG"] = prior

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


class ReleaseLookupTests(unittest.TestCase):
    def setUp(self):
        sandbox = tempfile.TemporaryDirectory(prefix="holdmap-release-lookup-test-")
        self.addCleanup(sandbox.cleanup)
        self.root = Path(sandbox.name)
        self.state = self.root / "github.json"
        self.calls = self.root / "calls.jsonl"
        stub = self.root / "gh"
        stub.write_text('''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
with Path(os.environ["HOLDMAP_TEST_CALLS"]).open("a") as log:
    log.write(json.dumps(args) + "\\n")
if args[:3] != ["api", "--method", "GET"]:
    sys.exit("only explicit GET requests are permitted")
state = json.loads(Path(os.environ["HOLDMAP_TEST_GITHUB"]).read_text())
endpoint = args[-1]
if "/releases/tags/" in endpoint:
    sys.exit("Not Found (HTTP 404): this route exposes published releases only")
elif endpoint.startswith("repos/example/holdmap/releases?per_page=100&page="):
    page = int(endpoint.rsplit("=", 1)[1])
    print(json.dumps(state["pages"][page - 1] if page <= len(state["pages"]) else []))
elif endpoint == "repos/example/holdmap/releases/42":
    print(json.dumps(state["release"]))
else:
    sys.exit("unexpected GitHub operation")
''')
        stub.chmod(0o700)

    def fixture(self, draft=True):
        release = {"id": 42, "tag_name": "v0.7.0", "draft": draft, "prerelease": False}
        return {"pages": [[release.copy()]], "release": release}

    def lookup(self, data, tag="v0.7.0", repository="example/holdmap"):
        self.state.write_text(json.dumps(data))
        self.calls.unlink(missing_ok=True)
        environment = os.environ.copy()
        environment.update({"PATH": str(self.root) + os.pathsep + environment["PATH"],
                            "GITHUB_REPOSITORY": repository, "TAG": tag,
                            "HOLDMAP_TEST_GITHUB": str(self.state), "HOLDMAP_TEST_CALLS": str(self.calls)})
        result = subprocess.run(["node", "--input-type=module", "-e",
                                 "const {lookupRelease} = await import(process.argv[1]); "
                                 "console.log(JSON.stringify(await lookupRelease(process.env.GITHUB_REPOSITORY, process.env.TAG)));",
                                 (ROOT / "scripts/release-api.mjs").as_uri()],
                                env=environment, capture_output=True, text=True, check=False, timeout=30)
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []
        for call in calls:
            self.assertEqual(call[:3], ["api", "--method", "GET"])
            self.assertNotIn("/releases/tags/", call[-1])
        return result, calls

    def test_drafts_and_published_releases_use_listing_then_pinned_numeric_id(self):
        for draft in [True, False]:
            with self.subTest(draft=draft):
                data = self.fixture(draft)
                result, calls = self.lookup(data)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), data["release"])
                self.assertEqual([call[-1] for call in calls], [
                    "repos/example/holdmap/releases?per_page=100&page=1",
                    "repos/example/holdmap/releases/42"])

    def test_later_page_lookup_and_duplicate_tags_across_pages(self):
        data = self.fixture()
        others = [{"id": 1000 + index, "tag_name": "other-" + str(index)} for index in range(100)]
        data["pages"] = [others, data["pages"][0]]
        result, calls = self.lookup(data)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any(call[-1].endswith("page=2") for call in calls))
        data["pages"][0][0] = data["release"].copy()
        result, calls = self.lookup(data)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Duplicate release tag", result.stderr)
        self.assertFalse(any(call[-1].endswith("/releases/42") for call in calls))

    def test_missing_or_malformed_listed_release_identity_fails_before_id_lookup(self):
        cases = [[], {}, None, [{"tag_name": "v0.7.0"}]]
        cases.extend([{"id": identifier, "tag_name": "v0.7.0"}]
                     for identifier in [0, -1, 42.5, 2 ** 53, "42", None])
        for page in cases:
            with self.subTest(page=page):
                data = self.fixture()
                data["pages"] = [page]
                result, calls = self.lookup(data)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(len(calls), 1)

    def test_numeric_id_response_must_preserve_tag_id_draft_and_prerelease_identity(self):
        cases = [None, {}]
        for field, value in [("id", 43), ("tag_name", "v0.6.0"), ("draft", None),
                             ("prerelease", True), ("id", None)]:
            release = self.fixture()["release"]
            release[field] = value
            cases.append(release)
        for release in cases:
            with self.subTest(release=release):
                data = self.fixture()
                data["release"] = release
                result, calls = self.lookup(data)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(len(calls), 2)

    def test_release_listing_is_bounded_even_if_a_matching_tag_was_found(self):
        data = self.fixture()
        data["pages"] = [[{"id": 1000 + page * 100 + index, "tag_name": f"other-{page}-{index}"}
                          for index in range(100)] for page in range(11)]
        data["pages"][0][0] = data["release"].copy()
        result, calls = self.lookup(data)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unexpectedly large release list", result.stderr)
        self.assertEqual(len(calls), 11)

    def test_noncanonical_tags_and_repositories_never_reach_github(self):
        for tag in ["v01.2.3", "v0.7.0-01", "v0.7.0\n", "v0.7.0$(false)"]:
            with self.subTest(tag=tag):
                result, calls = self.lookup(self.fixture(), tag=tag)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])
        for repository in ["example/..", "example/holdmap/extra", "example/holdmap\n"]:
            with self.subTest(repository=repository):
                result, calls = self.lookup(self.fixture(), repository=repository)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])


class PagesSourceTests(unittest.TestCase):
    workflow_revision = "1" * 40
    source_revision = "a" * 40
    annotation = "b" * 40
    ref_path = "repos/example/holdmap/git/ref/tags/v0.7.0"

    def setUp(self):
        sandbox = tempfile.TemporaryDirectory(prefix="holdmap-pages-source-test-")
        self.addCleanup(sandbox.cleanup)
        self.root = Path(sandbox.name)
        self.responses = self.root / "responses.json"
        self.calls = self.root / "calls.jsonl"
        self.output = self.root / "output"
        stub = self.root / "gh"
        stub.write_text('''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
with Path(os.environ["HOLDMAP_TEST_CALLS"]).open("a") as log:
    log.write(json.dumps(args) + "\\n")
if len(args) != 6 or args[:5] != ["api", "--method", "GET", "-H", "X-GitHub-Api-Version: 2022-11-28"]:
    sys.exit("only fixed GET requests are permitted")
responses = json.loads(Path(os.environ["HOLDMAP_TEST_RESPONSES"]).read_text())
if args[-1] not in responses:
    sys.exit("unexpected GitHub API route")
print(json.dumps(responses[args[-1]]))
''')
        stub.chmod(0o700)

    def fixture(self):
        release = {"id": 42, "tag_name": "v0.7.0", "draft": False, "prerelease": False}
        return {
            "repos/example/holdmap/releases?per_page=100&page=1": [release.copy()],
            "repos/example/holdmap/releases/42": release,
            self.ref_path: {"ref": "refs/tags/v0.7.0", "object": {"type": "tag", "sha": self.annotation}},
            "repos/example/holdmap/git/tags/" + self.annotation:
                {"sha": self.annotation, "object": {"type": "commit", "sha": self.source_revision}},
        }

    def resolve(self, data=None, event="workflow_dispatch", ref="refs/heads/main", tag="v0.7.0",
                repository="example/holdmap"):
        self.responses.write_text(json.dumps(data if data is not None else self.fixture()))
        self.calls.unlink(missing_ok=True)
        self.output.unlink(missing_ok=True)
        environment = os.environ.copy()
        environment.update({"PATH": str(self.root) + os.pathsep + environment["PATH"],
                            "GITHUB_EVENT_NAME": event, "GITHUB_REF": ref,
                            "GITHUB_REPOSITORY": repository, "GITHUB_SHA": self.workflow_revision,
                            "SOURCE_TAG": tag, "GITHUB_OUTPUT": str(self.output),
                            "HOLDMAP_TEST_CALLS": str(self.calls), "HOLDMAP_TEST_RESPONSES": str(self.responses)})
        result = subprocess.run(["node", str(ROOT / "scripts/resolve-pages-source.mjs")],
                                env=environment, capture_output=True, text=True, check=False, timeout=30)
        calls = [json.loads(line)[-1] for line in self.calls.read_text().splitlines()] if self.calls.exists() else []
        if result.returncode != 0:
            self.assertFalse(self.output.exists(), "failed source resolution must emit no checkout authority")
        return result, calls

    def test_main_and_pull_request_builds_keep_the_event_revision_without_api_calls(self):
        for event, ref in [("push", "refs/heads/main"), ("pull_request", "refs/pull/51/merge")]:
            with self.subTest(event=event):
                result, calls = self.resolve(event=event, ref=ref, tag="")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(calls, [])
                self.assertEqual(self.output.read_text(), f"revision={self.workflow_revision}\n")

    def test_dispatch_peels_annotated_and_lightweight_tags_to_the_actual_source_commit(self):
        for annotated in [False, True]:
            with self.subTest(annotated=annotated):
                data = self.fixture()
                if not annotated:
                    data[self.ref_path]["object"] = {"type": "commit", "sha": self.source_revision}
                result, calls = self.resolve(data)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.output.read_text(), f"revision={self.source_revision}\n")
                self.assertIn(self.ref_path, calls)
                self.assertNotIn("/releases/tags/", " ".join(calls))

    def test_non_main_dispatch_and_invalid_or_prerelease_tags_fail_before_github(self):
        cases = [("workflow_dispatch", "refs/tags/v0.7.0", "v0.7.0"),
                 ("workflow_dispatch", "refs/heads/other", "v0.7.0"),
                 ("push", "refs/heads/main", "v0.7.0"),
                 ("workflow_run", "refs/heads/main", "")]
        cases.extend(("workflow_dispatch", "refs/heads/main", tag) for tag in [
            "", "v01.2.3", "v0.7.0-rc.1", "refs/tags/v0.7.0", "v0.7.0\n", "v0.7.0$(false)"])
        for event, ref, tag in cases:
            with self.subTest(event=event, ref=ref, tag=tag):
                result, calls = self.resolve(event=event, ref=ref, tag=tag)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])
        for repository in ["../holdmap", "example/..", "example/holdmap?query=1", "example/holdmap\n"]:
            with self.subTest(repository=repository):
                result, calls = self.resolve(repository=repository)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])

    def test_draft_missing_or_mismatched_releases_never_reach_tag_resolution(self):
        for case in ["draft", "missing", "prerelease", "wrong_tag"]:
            with self.subTest(case=case):
                data = self.fixture()
                release = data["repos/example/holdmap/releases/42"]
                if case == "draft":
                    release["draft"] = True
                elif case == "prerelease":
                    release["prerelease"] = True
                elif case == "wrong_tag":
                    release["tag_name"] = "v0.6.0"
                else:
                    data["repos/example/holdmap/releases?per_page=100&page=1"] = []
                result, calls = self.resolve(data)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn(self.ref_path, calls)

    def test_ambiguous_refs_malformed_objects_and_tag_cycles_fail_closed(self):
        for case in ["branch", "short_sha", "blob", "annotation_mismatch", "cycle"]:
            with self.subTest(case=case):
                data = self.fixture()
                if case == "branch":
                    data[self.ref_path]["ref"] = "refs/heads/v0.7.0"
                elif case == "short_sha":
                    data[self.ref_path]["object"]["sha"] = "a" * 39
                elif case == "blob":
                    data[self.ref_path]["object"]["type"] = "blob"
                elif case == "annotation_mismatch":
                    data["repos/example/holdmap/git/tags/" + self.annotation]["sha"] = "c" * 40
                else:
                    data["repos/example/holdmap/git/tags/" + self.annotation]["object"] = {
                        "type": "tag", "sha": self.annotation}
                result, _ = self.resolve(data)
                self.assertNotEqual(result.returncode, 0)

    def test_annotated_tag_chain_has_a_finite_limit(self):
        data = self.fixture()
        digests = [f"{index:040x}" for index in range(200, 210)]
        data[self.ref_path]["object"] = {"type": "tag", "sha": digests[0]}
        for left, right in zip(digests, digests[1:]):
            data["repos/example/holdmap/git/tags/" + left] = {"sha": left, "object": {"type": "tag", "sha": right}}
        result, calls = self.resolve(data)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exceeds its limit", result.stderr)
        self.assertEqual(len([path for path in calls if "/git/tags/" in path]), 8)

    def test_workflow_verifies_pinned_checkout_before_install_and_records_actual_head(self):
        workflow = (ROOT / ".github/workflows/pages.yml").read_text()
        self.assertLess(workflow.index("run: node scripts/resolve-pages-source.mjs"),
                        workflow.index("ref: ${{ steps.source.outputs.revision }}"))
        check = 'test "$(git rev-parse HEAD)" = "$WEBSITE_SOURCE_REVISION"'
        self.assertLess(workflow.index(check), workflow.index("Install (desktop app"))
        self.assertIn("revision: execFileSync('git', ['rev-parse', 'HEAD']", workflow)
        git = self.root / "git"
        git.write_text('#!/bin/sh\nprintf "%s\\n" "$HOLDMAP_TEST_CHECKOUT_SHA"\n')
        git.chmod(0o700)
        for actual in [self.source_revision, self.workflow_revision]:
            environment = {**os.environ, "PATH": str(self.root) + os.pathsep + os.environ["PATH"],
                           "WEBSITE_SOURCE_REVISION": self.source_revision, "HOLDMAP_TEST_CHECKOUT_SHA": actual}
            result = subprocess.run(["bash", "-c", check], env=environment, capture_output=True, check=False)
            self.assertEqual(result.returncode, 0 if actual == self.source_revision else 1)

    def test_release_dispatch_runs_on_main_with_the_literal_source_tag(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        command = re.search(r"(?m)^\s*run: (gh workflow run pages\.yml[^\n]+)$", workflow).group(1)
        stub = self.root / "gh"
        stub.write_text('#!/usr/bin/env python3\nimport json, sys\nprint(json.dumps(sys.argv[1:]))\n')
        stub.chmod(0o700)
        environment = {**os.environ, "PATH": str(self.root) + os.pathsep + os.environ["PATH"],
                       "TAG": "v0.7.0", "GITHUB_REPOSITORY": "example/holdmap"}
        result = subprocess.run(["bash", "-c", command], env=environment,
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), ["workflow", "run", "pages.yml", "--repo", "example/holdmap",
                                                   "--ref", "main", "--raw-field", "source_tag=v0.7.0"])


class CompleteReleaseAssetTests(unittest.TestCase):
    def setUp(self):
        sandbox = tempfile.TemporaryDirectory(prefix="holdmap-release-completeness-test-")
        self.addCleanup(sandbox.cleanup)
        self.root = Path(sandbox.name)
        self.state = self.root / "github.json"
        self.calls = self.root / "calls.jsonl"
        stub = self.root / "gh"
        stub.write_text('''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
with Path(os.environ["HOLDMAP_TEST_CALLS"]).open("a") as log:
    log.write(json.dumps(args) + "\\n")
if args[:3] != ["api", "--method", "GET"]:
    sys.exit("only explicit GET API requests are permitted")
state = json.loads(Path(os.environ["HOLDMAP_TEST_GITHUB"]).read_text())
endpoint = args[-1]
if endpoint.startswith("repos/example/holdmap/releases?per_page=100&page="):
    page = int(endpoint.rsplit("=", 1)[1])
    releases = state.get("releases", [state["release"]])
    print(json.dumps(releases[(page - 1) * 100:page * 100]))
elif endpoint == "repos/example/holdmap/releases/42":
    print(json.dumps(state["release"]))
elif endpoint.startswith("repos/example/holdmap/releases/42/assets?per_page=100&page="):
    page = int(endpoint.rsplit("=", 1)[1])
    print(json.dumps(state["assets"][(page - 1) * 100:page * 100]))
elif endpoint.startswith("repos/example/holdmap/releases/assets/"):
    identifier = endpoint.rsplit("/", 1)[1]
    if "Accept: application/octet-stream" not in args:
        sys.exit("metadata downloads must request raw asset bytes")
    if identifier not in state["payloads"]:
        sys.exit("binary downloads are forbidden")
    sys.stdout.buffer.write(state["payloads"][identifier].encode())
else:
    sys.exit("unexpected GitHub operation")
''')
        stub.chmod(0o700)

    def fixture(self, version="0.6.0", draft=True):
        cli = ["holdmap-" + suffix for suffix in [
            "aarch64-apple-darwin.tar.xz", "x86_64-apple-darwin.tar.xz",
            "aarch64-unknown-linux-gnu.tar.xz", "x86_64-unknown-linux-gnu.tar.xz",
            "x86_64-unknown-linux-musl.tar.xz", "x86_64-pc-windows-msvc.zip",
        ]]
        desktop = {
            "macos-arm64": [f"holdmap_{version}_aarch64.dmg"],
            "macos-x64": [f"holdmap_{version}_x64.dmg"],
            "windows-x64": [f"holdmap_{version}_x64_en-US.msi", f"holdmap_{version}_x64-setup.exe"],
            "linux-x64": [f"holdmap_{version}_amd64.deb", f"holdmap-{version}-1.x86_64.rpm", f"holdmap_{version}_amd64.AppImage"],
        }
        data = {"release": {"id": 42, "tag_name": "v" + version,
                            "draft": draft, "prerelease": "-" in version},
                "assets": [], "payloads": {}}

        def add(name, body=None):
            raw = (body if body is not None else "binary:" + name).encode()
            asset = {"id": 700 + len(data["assets"]), "name": name, "size": len(raw),
                     "digest": "sha256:" + hashlib.sha256(raw).hexdigest(), "state": "uploaded"}
            data["assets"].append(asset)
            if body is not None:
                data["payloads"][str(asset["id"])] = body

        for name in cli + [name for names in desktop.values() for name in names] + [
            "holdmap-installer.sh", "holdmap-installer.ps1", "holdmap-npm-package.tar.gz", "source.tar.gz",
        ] + [name + ".cdx.xml" for name in ["holdmap", "holdmap-core", "holdmap-activity", "holdmap-mcp", "holdmap-desktop"]]:
            add(name)

        def checksums(names, binary=False):
            by_name = {asset["name"]: asset for asset in data["assets"]}
            return "".join(by_name[name]["digest"][7:] + (" *" if binary else "  ") + name + "\n" for name in names)

        for name in cli + ["source.tar.gz"]:
            add(name + ".sha256", checksums([name], binary=True))
        for platform, names in desktop.items():
            add("holdmap-desktop-" + platform + ".sha256", checksums(names))
        add("sha256.sum", checksums(cli + ["holdmap-npm-package.tar.gz", "source.tar.gz"], binary=True))
        add("dist-manifest.json", json.dumps({"announcement_tag": "v" + version,
            "announcement_is_prerelease": "-" in version,
            "releases": [{"app_name": "holdmap", "app_version": version}]}))
        return data

    def replace_payload(self, data, name, body, update_digest=True):
        asset = next(asset for asset in data["assets"] if asset["name"] == name)
        data["payloads"][str(asset["id"])] = body
        if update_digest:
            asset["size"] = len(body.encode())
            asset["digest"] = "sha256:" + hashlib.sha256(body.encode()).hexdigest()

    def verify(self, data, tag=None, repository="example/holdmap", draft="true"):
        self.state.write_text(json.dumps(data))
        self.calls.unlink(missing_ok=True)
        environment = os.environ.copy()
        environment.update({"PATH": str(self.root) + os.pathsep + environment["PATH"],
                            "GITHUB_REPOSITORY": repository,
                            "TAG": data["release"]["tag_name"] if tag is None else tag,
                            "CHECK_RELEASE_DRAFT": draft,
                            "HOLDMAP_TEST_GITHUB": str(self.state), "HOLDMAP_TEST_CALLS": str(self.calls)})
        result = subprocess.run(["node", str(ROOT / "scripts/check-release-assets.mjs")],
                                env=environment, capture_output=True, text=True, check=False, timeout=30)
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []
        for call in calls:
            self.assertEqual(call[:3], ["api", "--method", "GET"], "verifier must never publish or mutate")
            self.assertNotIn("/releases/tags/", call[-1], "drafts are absent from the published-only tag route")
        self.assertEqual(json.loads(self.state.read_text()), data, "verifier must not modify release state")
        return result, calls

    def test_complete_stable_and_prerelease_assets_pass_without_binary_downloads(self):
        for version, draft in [("0.6.0", "true"), ("1.2.3-rc.1", "true"), ("0.6.0", "false")]:
            with self.subTest(version=version, draft=draft):
                data = self.fixture(version, draft=draft == "true")
                result, calls = self.verify(data, draft=draft)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("no release changes made", result.stdout)
                self.assertEqual(len([call for call in calls if "/releases/assets/" in call[-1]]), 13)

    def test_missing_platform_or_duplicate_asset_fails(self):
        for name in ["holdmap_0.6.0_aarch64.dmg", "holdmap_0.6.0_x64.dmg",
                     "holdmap_0.6.0_amd64.AppImage", "holdmap_0.6.0_x64-setup.exe",
                     "holdmap-desktop-windows-x64.sha256", "holdmap-installer.sh", "holdmap-core.cdx.xml",
                     "holdmap-aarch64-unknown-linux-gnu.tar.xz.sha256"]:
            with self.subTest(missing=name):
                data = self.fixture()
                data["assets"] = [asset for asset in data["assets"] if asset["name"] != name]
                result, _ = self.verify(data)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(name, result.stderr)
        data = self.fixture()
        duplicate = data["assets"][0].copy()
        duplicate["id"] = 9999
        data["assets"].append(duplicate)
        result, _ = self.verify(data)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Duplicate", result.stderr)

    def test_assets_on_later_pages_and_duplicates_are_not_hidden(self):
        data = self.fixture()
        extra = [{"id": 10_000 + i, "name": f"optional-{i}.sig", "size": 1,
                  "state": "uploaded", "digest": "sha256:" + "0" * 64} for i in range(80)]
        data["assets"] = extra + data["assets"]
        result, calls = self.verify(data)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any(call[-1].endswith("page=2") for call in calls))
        duplicate = data["assets"][80].copy()
        duplicate["id"] = 20_000
        data["assets"].append(duplicate)
        result, _ = self.verify(data)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Duplicate", result.stderr)

    def test_cli_checksums_and_metadata_size_limits_are_enforced(self):
        for name in ["holdmap-aarch64-apple-darwin.tar.xz.sha256", "sha256.sum"]:
            with self.subTest(name=name):
                data = self.fixture()
                asset = next(asset for asset in data["assets"] if asset["name"] == name)
                body = data["payloads"][str(asset["id"])]
                self.replace_payload(data, name, "0" * 64 + body[64:])
                result, _ = self.verify(data)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Checksum differs", result.stderr)
        data = self.fixture()
        asset = next(asset for asset in data["assets"] if asset["name"] == "sha256.sum")
        asset["size"] = 64 * 1024 + 1
        result, calls = self.verify(data)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("size limit", result.stderr)
        self.assertFalse(any(call[-1].endswith("/" + str(asset["id"])) for call in calls))

    def test_unfinished_zero_byte_or_unverifiable_asset_fails(self):
        for field, value in [("state", "starter"), ("size", 0), ("digest", None), ("digest", "sha256:bad")]:
            with self.subTest(field=field, value=value):
                data = self.fixture()
                data["assets"][0][field] = value
                result, _ = self.verify(data)
                self.assertNotEqual(result.returncode, 0)

    def test_checksum_content_must_match_uploaded_bytes_and_cover_every_platform(self):
        for case in ["binary_digest", "metadata_digest", "missing_entry", "duplicate_entry", "path_entry"]:
            with self.subTest(case=case):
                data = self.fixture()
                name = "holdmap-desktop-windows-x64.sha256"
                asset = next(asset for asset in data["assets"] if asset["name"] == name)
                original = data["payloads"][str(asset["id"])]
                if case == "binary_digest":
                    body = "0" * 64 + original[64:]
                elif case == "metadata_digest":
                    body = original.replace("  holdmap_", " *holdmap_", 1)
                elif case == "missing_entry":
                    body = original.splitlines(keepends=True)[0]
                elif case == "duplicate_entry":
                    body = original + original.splitlines(keepends=True)[0]
                else:
                    body = original.replace("  holdmap_", "  ../holdmap_", 1)
                self.replace_payload(data, name, body, update_digest=case != "metadata_digest")
                result, _ = self.verify(data)
                self.assertNotEqual(result.returncode, 0)

    def test_release_and_distribution_identity_must_agree(self):
        for case in ["tag", "prerelease", "published", "manifest"]:
            with self.subTest(case=case):
                data = self.fixture()
                if case == "tag":
                    data["release"]["tag_name"] = "v0.5.0"
                    tag = "v0.6.0"
                else:
                    tag = None
                if case == "prerelease":
                    data["release"]["prerelease"] = True
                elif case == "published":
                    data["release"]["draft"] = False
                elif case == "manifest":
                    self.replace_payload(data, "dist-manifest.json", json.dumps({
                        "announcement_tag": "v0.5.0", "announcement_is_prerelease": False,
                        "releases": [{"app_name": "holdmap", "app_version": "0.5.0"}]}))
                result, _ = self.verify(data, tag=tag)
                self.assertNotEqual(result.returncode, 0)

    def test_invalid_tag_and_repository_never_reach_github(self):
        for tag in ["", "v0.6", "v0.6.0\n", "v0.6.0-..", "v0.6.0-rc/1", "v0.6.0$(touch marker)"]:
            with self.subTest(tag=tag):
                result, calls = self.verify(self.fixture(), tag=tag)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])
        for repository in ["", "../holdmap", "example/..", "example/.", "example/holdmap/extra", "example/holdmap\n"]:
            with self.subTest(repository=repository):
                result, calls = self.verify(self.fixture(), repository=repository)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])


class CompleteDesktopUpdaterTests(unittest.TestCase):
    def setUp(self):
        self.sandbox = tempfile.TemporaryDirectory(prefix="holdmap-updater-index-test-")
        self.addCleanup(self.sandbox.cleanup)
        self.root = Path(self.sandbox.name)
        self.state = self.root / "github.json"
        self.assets = []
        for identifier, (matrix, target, kind, suffix) in enumerate([
            ("macos-arm64", "darwin-aarch64", "app", ".app.tar.gz"),
            ("macos-x64", "darwin-x86_64", "app", ".app.tar.gz"),
            ("linux-x64", "linux-x86_64", "appimage", ".AppImage"),
            ("windows-x64", "windows-x86_64", "nsis", ".exe"),
        ], 701):
            name = "holdmap-" + matrix + suffix
            digest = hashlib.sha256(name.encode()).hexdigest()
            asset = {"name": name, "digest": digest, "signature": "dGVzdA=="}
            manifest = {"matrix": matrix, "version": "0.5.0",
                        "platforms": {target: asset, target + "-" + kind: asset}}
            (self.root / (matrix + ".updater.json")).write_text(json.dumps(manifest))
            self.assets.append({"name": name, "id": identifier, "digest": "sha256:" + digest})
        self.initial = {"assets": self.assets, "uploads": [], "next_id": 900,
                        "release": {"id": 42, "tag_name": "v0.5.0", "draft": True,
                                    "prerelease": False, "body": "Release notes",
                                    "published_at": None, "created_at": "2026-10-01T12:00:00Z"}}
        self.state.write_text(json.dumps(self.initial))
        stub = self.root / "gh"
        stub.write_text('''#!/usr/bin/env python3
import hashlib, json, os, sys
from pathlib import Path
path = Path(os.environ["HOLDMAP_TEST_GITHUB"])
state = json.loads(path.read_text())
args = sys.argv[1:]
if args[0] == "api":
    endpoint = args[-1]
    if endpoint == "repos/example/holdmap/releases?per_page=100&page=1":
        print(json.dumps([state["release"]]))
    elif endpoint == "repos/example/holdmap/releases/42":
        print(json.dumps(state["release"]))
    elif endpoint == "repos/example/holdmap/releases/42/assets?per_page=100":
        print(json.dumps([state["assets"]]))
    else:
        sys.exit("unexpected GitHub API endpoint; tag routes do not expose drafts")
elif args[:2] == ["release", "upload"]:
    payload = Path(args[3])
    if payload.name != "latest.json" or "--clobber" not in args:
        sys.exit("only the updater index may be replaced")
    state["latest_json"] = json.loads(payload.read_text())
    state["assets"] = [asset for asset in state["assets"] if asset["name"] != "latest.json"]
    state["assets"].append({"name":"latest.json","id":state["next_id"],
                            "digest":"sha256:"+hashlib.sha256(payload.read_bytes()).hexdigest()})
    state["next_id"] += 1
    state["uploads"].append("latest.json")
    path.write_text(json.dumps(state))
else:
    sys.exit("unexpected GitHub operation")
''')
        stub.chmod(0o700)

    def publish(self):
        environment = os.environ.copy()
        environment.update({"PATH": str(self.root) + os.pathsep + environment["PATH"],
                            "GITHUB_REPOSITORY": "example/holdmap", "TAG": "v0.5.0",
                            "UPDATER_MANIFEST_DIR": str(self.root), "HOLDMAP_TEST_GITHUB": str(self.state)})
        return subprocess.run(["node", str(ROOT / "scripts/publish-desktop-updater.mjs")],
                              env=environment, capture_output=True, text=True, check=False)

    def test_complete_index_uses_verified_stable_asset_ids_and_is_idempotent(self):
        for _ in range(2):
            result = self.publish()
            self.assertEqual(result.returncode, 0, result.stderr)
        state = json.loads(self.state.read_text())
        self.assertEqual(state["assets"][:4], self.assets)
        self.assertEqual(state["uploads"], ["latest.json"])
        index = state["latest_json"]
        self.assertEqual(index["version"], "0.5.0")
        self.assertEqual(len(index["platforms"]), 8)
        for matrix, target, identifier in [("macos-arm64", "darwin-aarch64", 701),
                                           ("macos-x64", "darwin-x86_64", 702),
                                           ("linux-x64", "linux-x86_64", 703),
                                           ("windows-x64", "windows-x86_64", 704)]:
            with self.subTest(matrix=matrix):
                self.assertTrue(index["platforms"][target]["url"].endswith("/" + str(identifier)))

    def test_missing_or_inconsistent_platforms_cannot_publish(self):
        path = self.root / "windows-x64.updater.json"
        original = path.read_text()
        for case in ["missing", "wrong_version", "duplicate_matrix", "unexpected_platform", "wrong_format"]:
            with self.subTest(case=case):
                manifest = json.loads(original)
                if case == "missing":
                    path.unlink()
                elif case == "wrong_version":
                    manifest["version"] = "0.4.0"
                elif case == "duplicate_matrix":
                    manifest["matrix"] = "linux-x64"
                elif case == "unexpected_platform":
                    manifest["platforms"]["linux-aarch64"] = manifest["platforms"]["windows-x86_64"]
                else:
                    manifest["platforms"]["windows-x86_64"]["name"] = "unexpected.dmg"
                if case != "missing":
                    path.write_text(json.dumps(manifest))
                result = self.publish()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(json.loads(self.state.read_text())["uploads"], [])
                path.write_text(original)

    def test_replaced_missing_or_duplicate_published_assets_cannot_publish(self):
        for case in ["changed", "missing", "duplicate", "unknown_digest"]:
            with self.subTest(case=case):
                state = json.loads(json.dumps(self.initial))
                if case == "changed":
                    state["assets"][0]["digest"] = "sha256:" + "0" * 64
                elif case == "missing":
                    state["assets"].pop(0)
                elif case == "duplicate":
                    state["assets"].append(state["assets"][0].copy())
                else:
                    state["assets"][0]["digest"] = None
                self.state.write_text(json.dumps(state))
                result = self.publish()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(json.loads(self.state.read_text())["uploads"], [])


class SignedDesktopUploadTests(unittest.TestCase):
    def setUp(self):
        sandbox = tempfile.TemporaryDirectory(prefix="holdmap-signed-upload-test-")
        self.addCleanup(sandbox.cleanup)
        self.root = Path(sandbox.name)
        self.state = self.root / "github.json"
        self.manifest = self.root / "assets.json"
        assets = []
        published = []
        for identifier, suffix in [(701, ".dmg"), (702, ".app.tar.gz"), (703, ".app.tar.gz.sig")]:
            name = "holdmap_0.4.0_aarch64" + suffix
            path = self.root / name
            path.write_bytes(name.encode())
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            assets.append({"name": name, "path": str(path), "digest": digest})
            published.append({"name": name, "id": identifier, "digest": "sha256:" + digest})
        self.manifest.write_text(json.dumps(assets))
        self.initial = {"assets": published, "uploads": [], "next_id": 900,
                        "release": {"id": 42, "tag_name": "v0.4.0", "draft": True, "prerelease": False},
                        "latest_json": {"platforms": {"darwin-aarch64": {
                            "url": "https://api.github.com/repos/example/holdmap/releases/assets/702"}}}}
        self.state.write_text(json.dumps(self.initial))
        stub = self.root / "gh"
        stub.write_text('''#!/usr/bin/env python3
import hashlib, json, os, sys
from pathlib import Path
path = Path(os.environ["HOLDMAP_TEST_GITHUB"])
state = json.loads(path.read_text())
args = sys.argv[1:]
if args[0] == "api":
    endpoint = args[-1]
    if endpoint == "repos/example/holdmap/releases?per_page=100&page=1":
        print(json.dumps([state["release"]]))
    elif endpoint == "repos/example/holdmap/releases/42":
        print(json.dumps(state["release"]))
    elif endpoint == "repos/example/holdmap/releases/42/assets?per_page=100":
        print(json.dumps([state["assets"]]))
    else:
        sys.exit("unexpected GitHub API endpoint; tag routes do not expose drafts")
elif args[:2] == ["release", "upload"]:
    if "--clobber" in args:
        sys.exit("asset replacement is forbidden")
    payload = Path(args[3])
    if any(asset["name"] == payload.name for asset in state["assets"]):
        sys.exit("existing asset cannot be replaced")
    state["assets"].append({"name": payload.name, "id": state["next_id"],
                            "digest": "sha256:" + hashlib.sha256(payload.read_bytes()).hexdigest()})
    state["next_id"] += 1
    state["uploads"].append(payload.name)
    fail = state.pop("fail_after_upload", False)
    path.write_text(json.dumps(state))
    if fail:
        sys.exit("transfer response lost after acceptance")
else:
    sys.exit("unexpected GitHub operation")
''')
        stub.chmod(0o700)

    def upload(self):
        environment = os.environ.copy()
        environment.update({"PATH": str(self.root) + os.pathsep + environment["PATH"],
                            "GITHUB_REPOSITORY": "example/holdmap", "TAG": "v0.4.0",
                            "RELEASE_ASSET_MANIFEST": str(self.manifest), "HOLDMAP_TEST_GITHUB": str(self.state)})
        return subprocess.run(["node", str(ROOT / "scripts/upload-desktop-release-assets.mjs")],
                              env=environment, capture_output=True, text=True, check=False)

    def test_matching_uploads_preserve_latest_json_asset_ids_on_repeated_runs(self):
        for _ in range(2):
            result = self.upload()
            self.assertEqual(result.returncode, 0, result.stderr)
        state = json.loads(self.state.read_text())
        self.assertEqual(state["assets"], self.initial["assets"])
        self.assertEqual(state["latest_json"], self.initial["latest_json"])
        self.assertEqual(state["uploads"], [])
        referenced = int(state["latest_json"]["platforms"]["darwin-aarch64"]["url"].rsplit("/", 1)[1])
        self.assertTrue(any(asset["id"] == referenced for asset in state["assets"]))

    def test_missing_asset_recovers_an_accepted_upload_without_replacing_ids(self):
        state = json.loads(self.state.read_text())
        state["assets"] = state["assets"][:2]
        state["fail_after_upload"] = True
        self.state.write_text(json.dumps(state))
        result = self.upload()
        self.assertEqual(result.returncode, 0, result.stderr)
        final = json.loads(self.state.read_text())
        self.assertEqual(final["assets"][:2], self.initial["assets"][:2])
        self.assertEqual(final["latest_json"], self.initial["latest_json"])
        self.assertEqual(final["uploads"], ["holdmap_0.4.0_aarch64.app.tar.gz.sig"])

    def test_mismatched_or_unknown_digest_fails_before_any_upload(self):
        for digest in [None, "sha256:" + "0" * 64]:
            with self.subTest(digest=digest):
                state = json.loads(json.dumps(self.initial))
                state["assets"] = state["assets"][1:]
                state["assets"][0]["digest"] = digest
                self.state.write_text(json.dumps(state))
                result = self.upload()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(json.loads(self.state.read_text())["uploads"], [])


if __name__ == "__main__":
    unittest.main()
