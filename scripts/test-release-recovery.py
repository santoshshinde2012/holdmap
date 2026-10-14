#!/usr/bin/env python3
"""Isolated release recovery regressions; the gh fixture permits GET requests only."""

import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "example/holdmap"
TAG = "v0.7.0"
RUN_ID = 123456
COMMIT = "a" * 40
ANNOTATION = "b" * 40
RUN_PATH = f"repos/{REPOSITORY}/actions/runs/{RUN_ID}"
REF_PATH = f"repos/{REPOSITORY}/git/ref/tags/{TAG}"
JOBS_PATH = RUN_PATH + "/jobs?per_page=100&page=1&filter=all"


def fixture():
    names = [
        "plan", "build-global-artifacts", "host", "announce",
        "build-local-artifacts (aarch64-apple-darwin)",
        "build-local-artifacts (x86_64-apple-darwin)",
        "build-local-artifacts (aarch64-unknown-linux-gnu)",
        "build-local-artifacts (x86_64-unknown-linux-gnu)",
        "build-local-artifacts (x86_64-unknown-linux-musl)",
        "build-local-artifacts (x86_64-pc-windows-msvc)",
        "custom-desktop-release / plan", "custom-desktop-release / notes",
        "custom-desktop-release / build (macos-arm64)",
        "custom-desktop-release / build (macos-x64)",
        "custom-desktop-release / build (linux-x64)",
        "custom-desktop-release / build (windows-x64)",
        "custom-desktop-release / updater", "publish-complete-release",
    ]
    jobs = [{"id": index + 1, "run_id": RUN_ID, "run_attempt": 1, "head_sha": COMMIT,
             "name": name, "status": "completed", "conclusion": "success"}
            for index, name in enumerate(names)]
    jobs[-1]["conclusion"] = "failure"
    return {
        REF_PATH: {"ref": f"refs/tags/{TAG}", "object": {"type": "tag", "sha": ANNOTATION}},
        f"repos/{REPOSITORY}/git/tags/{ANNOTATION}":
            {"sha": ANNOTATION, "object": {"type": "commit", "sha": COMMIT}},
        RUN_PATH: {"id": RUN_ID, "run_attempt": 1, "path": ".github/workflows/release.yml",
                   "event": "push", "head_branch": TAG, "head_sha": COMMIT,
                   "status": "completed", "conclusion": "failure",
                   "repository": {"full_name": REPOSITORY},
                   "head_repository": {"full_name": REPOSITORY}},
        JOBS_PATH: {"total_count": len(jobs), "jobs": jobs},
    }


class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.sandbox = tempfile.TemporaryDirectory(prefix="holdmap-release-recovery-test-")
        self.addCleanup(self.sandbox.cleanup)
        self.directory = Path(self.sandbox.name)
        self.record = self.directory / "requests.jsonl"
        self.responses = self.directory / "responses.json"
        stub = self.directory / "gh"
        stub.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "from pathlib import Path\n"
            "args = sys.argv[1:]\n"
            "with Path(os.environ['HOLDMAP_TEST_RECORD']).open('a') as record:\n"
            "    record.write(json.dumps(args) + '\\n')\n"
            "if len(args) != 6 or args[:5] != ['api', '--method', 'GET', '-H', "
            "'X-GitHub-Api-Version: 2022-11-28']:\n"
            "    sys.exit('fixture rejects every mutation or unexpected CLI argument')\n"
            "responses = json.loads(Path(os.environ['HOLDMAP_TEST_RESPONSES']).read_text())\n"
            "if args[5] not in responses:\n"
            "    sys.exit('unexpected endpoint')\n"
            "value = responses[args[5]]\n"
            "if isinstance(value, list):\n"
            "    count_path = Path(os.environ['HOLDMAP_TEST_RECORD'] + '.counts')\n"
            "    counts = json.loads(count_path.read_text()) if count_path.exists() else {}\n"
            "    index = counts.get(args[5], 0)\n"
            "    counts[args[5]] = index + 1\n"
            "    count_path.write_text(json.dumps(counts))\n"
            "    value = value[min(index, len(value) - 1)]\n"
            "print(json.dumps(value))\n"
        )
        stub.chmod(0o700)

    def run_guard(self, responses=None, environment=None):
        self.responses.write_text(json.dumps(fixture() if responses is None else responses))
        self.record.unlink(missing_ok=True)
        Path(str(self.record) + ".counts").unlink(missing_ok=True)
        env = os.environ.copy()
        env.update({
            "PATH": str(self.directory) + os.pathsep + env["PATH"],
            "GITHUB_REPOSITORY": REPOSITORY, "TAG": TAG, "RELEASE_BUILD_RUN_ID": str(RUN_ID),
            "HOLDMAP_TEST_RECORD": str(self.record), "HOLDMAP_TEST_RESPONSES": str(self.responses),
        })
        env.update(environment or {})
        return subprocess.run([sys.executable, str(ROOT / "scripts/check-release-recovery.py")],
                              env=env, capture_output=True, text=True, timeout=10, check=False)

    def assert_rejected(self, responses=None, environment=None):
        result = self.run_guard(responses, environment)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("error:", result.stderr)
        self.assertEqual(result.stdout, "")
        return result

    def test_completed_build_accepts_only_get_and_binds_tag_run_and_jobs(self):
        result = self.run_guard()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(str(RUN_ID), result.stdout)
        self.assertIn(COMMIT, result.stdout)
        requests = [json.loads(line) for line in self.record.read_text().splitlines()]
        self.assertTrue(requests)
        self.assertTrue(all(request[:3] == ["api", "--method", "GET"] for request in requests))
        self.assertEqual([request[-1] for request in requests].count(RUN_PATH), 2)
        self.assertEqual([request[-1] for request in requests].count(REF_PATH), 2)

    def test_updater_can_be_successful_or_unconfigured_but_never_failed_or_incomplete(self):
        for conclusion in ["success", "skipped", "failure", "cancelled", None]:
            with self.subTest(conclusion=conclusion):
                responses = fixture()
                responses[JOBS_PATH]["jobs"][-2]["conclusion"] = conclusion
                result = self.run_guard(responses)
                self.assertEqual(result.returncode == 0, conclusion in {"success", "skipped"}, result.stderr)
        responses = fixture()
        responses[JOBS_PATH]["jobs"][-2]["status"] = "in_progress"
        self.assert_rejected(responses)

    def test_every_build_and_notes_job_must_succeed(self):
        base = fixture()
        for index, job in enumerate(base[JOBS_PATH]["jobs"][:-2]):
            for conclusion in ["failure", "skipped", "cancelled", None]:
                with self.subTest(job=job["name"], conclusion=conclusion):
                    responses = copy.deepcopy(base)
                    responses[JOBS_PATH]["jobs"][index]["conclusion"] = conclusion
                    self.assert_rejected(responses)
        responses = fixture()
        responses[JOBS_PATH]["jobs"][0]["status"] = "queued"
        self.assert_rejected(responses)

    def test_only_the_final_publication_job_may_have_failed(self):
        for conclusion in ["success", "failure", "skipped", "cancelled", None]:
            responses = fixture()
            responses[JOBS_PATH]["jobs"][-1]["conclusion"] = conclusion
            result = self.run_guard(responses)
            self.assertEqual(result.returncode == 0, conclusion in {"success", "failure"}, result.stderr)

    def test_wrong_event_ref_workflow_source_commit_and_run_state_are_refused(self):
        for key, value in [
            ("id", RUN_ID + 1), ("event", "pull_request"), ("event", "workflow_dispatch"),
            ("head_branch", "main"), ("head_branch", "v0.6.0"), ("head_sha", "c" * 40),
            ("path", ".github/workflows/other.yml"), ("status", "in_progress"),
            ("conclusion", "cancelled"), ("run_attempt", 0),
            ("repository", {"full_name": "foreign/holdmap"}),
            ("head_repository", {"full_name": "foreign/holdmap"}),
        ]:
            with self.subTest(key=key, value=value):
                responses = fixture()
                responses[RUN_PATH][key] = value
                self.assert_rejected(responses)

    def test_hostile_inputs_never_call_gh_or_a_shell(self):
        marker = self.directory / "executed"
        for key, values in {
            "GITHUB_REPOSITORY": ["", "../holdmap", "example/..", "example/holdmap/extra",
                                  "example/holdmap?query=1", "example/holdmap\nnext", "example/$(false)"],
            "TAG": ["", "v01.2.3", "v0.7.0-01", "v0.7.0\nnext", "--help",
                    f"v0.7.0$(touch${{IFS}}{marker})"],
            "RELEASE_BUILD_RUN_ID": ["", "0", "01", "-1", "1e3", "123\nnext", "--help",
                                     "9" * 21, f"$(touch {marker})"],
        }.items():
            for value in values:
                with self.subTest(key=key, value=value):
                    self.assert_rejected(environment={key: value})
                    self.assertFalse(self.record.exists())
                    self.assertFalse(marker.exists())

    def test_missing_duplicate_unknown_jobs_or_inconsistent_page_counts_are_refused(self):
        for change in ["missing", "duplicate-name", "duplicate-id", "unknown", "count"]:
            with self.subTest(change=change):
                responses = fixture()
                page = responses[JOBS_PATH]
                if change == "missing":
                    page["jobs"].pop(0)
                    page["total_count"] -= 1
                elif change == "duplicate-name":
                    job = copy.deepcopy(page["jobs"][0])
                    job["id"] = 999
                    page["jobs"].append(job)
                    page["total_count"] += 1
                elif change == "duplicate-id":
                    page["jobs"][1]["id"] = page["jobs"][0]["id"]
                elif change == "unknown":
                    page["jobs"][0]["name"] = "unrecognized build"
                else:
                    page["total_count"] += 1
                self.assert_rejected(responses)

    def test_jobs_cannot_be_borrowed_from_other_runs_commits_or_attempts(self):
        for key, value in [("run_id", RUN_ID + 1), ("head_sha", "c" * 40), ("run_attempt", 2)]:
            responses = fixture()
            responses[JOBS_PATH]["jobs"][0][key] = value
            self.assert_rejected(responses)
        responses = fixture()
        responses[RUN_PATH]["run_attempt"] = 2
        responses[JOBS_PATH]["jobs"][0]["conclusion"] = "success"
        self.assert_rejected(responses)
        responses = fixture()
        responses[RUN_PATH]["run_attempt"] = 2
        for job in responses[JOBS_PATH]["jobs"]:
            job["run_attempt"] = 2
        previous = copy.deepcopy(responses[JOBS_PATH]["jobs"][0])
        previous.update({"id": 999, "run_attempt": 1})
        responses[JOBS_PATH]["jobs"][0]["conclusion"] = "failure"
        responses[JOBS_PATH]["jobs"].insert(0, previous)
        responses[JOBS_PATH]["total_count"] += 1
        self.assert_rejected(responses)

    def test_lightweight_and_bounded_nested_annotated_tags_resolve_to_the_commit(self):
        responses = fixture()
        responses[REF_PATH]["object"] = {"type": "commit", "sha": COMMIT}
        self.assertEqual(self.run_guard(responses).returncode, 0)
        responses = fixture()
        nested = "c" * 40
        responses[f"repos/{REPOSITORY}/git/tags/{ANNOTATION}"]["object"] = {"type": "tag", "sha": nested}
        responses[f"repos/{REPOSITORY}/git/tags/{nested}"] = {"sha": nested, "object": {"type": "commit", "sha": COMMIT}}
        self.assertEqual(self.run_guard(responses).returncode, 0)

    def test_invalid_tag_references_annotation_identity_cycles_and_depth_are_refused(self):
        for change in ["ref", "sha", "type", "identity", "cycle", "depth"]:
            responses = fixture()
            annotation = responses[f"repos/{REPOSITORY}/git/tags/{ANNOTATION}"]
            if change == "ref":
                responses[REF_PATH]["ref"] = "refs/heads/main"
            elif change == "sha":
                responses[REF_PATH]["object"]["sha"] = "../unexpected"
            elif change == "type":
                responses[REF_PATH]["object"]["type"] = "tree"
            elif change == "identity":
                annotation["sha"] = "c" * 40
            elif change == "cycle":
                annotation["object"] = {"type": "tag", "sha": ANNOTATION}
            else:
                current = annotation
                for index in range(9):
                    digest = f"{index:040x}"
                    current["object"] = {"type": "tag", "sha": digest}
                    current = {"sha": digest, "object": {"type": "commit", "sha": COMMIT}}
                    responses[f"repos/{REPOSITORY}/git/tags/{digest}"] = current
            with self.subTest(change=change):
                self.assert_rejected(responses)

    def test_restarted_run_or_moved_tag_during_verification_is_refused(self):
        for change in ["attempt", "status", "tag"]:
            responses = fixture()
            if change == "tag":
                moved = copy.deepcopy(responses[REF_PATH])
                moved["object"] = {"type": "commit", "sha": "c" * 40}
                responses[REF_PATH] = [responses[REF_PATH], moved]
            else:
                restarted = copy.deepcopy(responses[RUN_PATH])
                restarted["run_attempt" if change == "attempt" else "status"] = 2 if change == "attempt" else "in_progress"
                responses[RUN_PATH] = [responses[RUN_PATH], restarted]
            with self.subTest(change=change):
                self.assert_rejected(responses)

    def test_invalid_job_list_shapes_and_excess_counts_are_refused(self):
        for page in [None, [], {"total_count": 1001, "jobs": []},
                     {"total_count": True, "jobs": []}, {"total_count": 18, "jobs": None}]:
            responses = fixture()
            responses[JOBS_PATH] = page
            self.assert_rejected(responses)

    def test_malformed_outcomes_and_oversized_api_bodies_fail_with_fixed_errors(self):
        for field in ["run", "updater", "publication"]:
            responses = fixture()
            if field == "run":
                responses[RUN_PATH]["conclusion"] = ["synthetic-private"]
            else:
                responses[JOBS_PATH]["jobs"][-2 if field == "updater" else -1]["conclusion"] = ["synthetic-private"]
            result = self.assert_rejected(responses)
            self.assertNotIn("Traceback", result.stderr)
            self.assertNotIn("synthetic-private", result.stderr)
        responses = fixture()
        responses[RUN_PATH] = {"secret": "synthetic-private" * 150_000}
        result = self.assert_rejected(responses)
        self.assertIn("response exceeded its limit", result.stderr)
        self.assertNotIn("synthetic-private", result.stderr)


if __name__ == "__main__":
    unittest.main()
