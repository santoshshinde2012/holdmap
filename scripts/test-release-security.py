#!/usr/bin/env python3
"""Exercise release context handling without calling dist or publishing artifacts."""
import json
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


if __name__ == "__main__":
    unittest.main()
