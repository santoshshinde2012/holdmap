#!/usr/bin/env python3
"""Read-only proof that a tag's original release build completed before recovery."""

import json
import os
import re
import selectors
import subprocess
import sys
import time

MAX_RESPONSE_BYTES = 2 * 1024 * 1024
MAX_JOBS = 1000
MAX_TAG_DEPTH = 8
WORKFLOW_PATH = ".github/workflows/release.yml"
SUCCESS_JOBS = {
    "plan", "build-global-artifacts", "host", "announce",
    "custom-desktop-release / plan", "custom-desktop-release / notes",
    *{
        f"build-local-artifacts ({target})" for target in (
            "aarch64-apple-darwin", "x86_64-apple-darwin",
            "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu",
            "x86_64-unknown-linux-musl", "x86_64-pc-windows-msvc",
        )
    },
    *{
        f"custom-desktop-release / build ({platform})" for platform in (
            "macos-arm64", "macos-x64", "linux-x64", "windows-x64",
        )
    },
}
UPDATER_JOB = "custom-desktop-release / updater"
PUBLICATION_JOB = "publish-complete-release"
EXPECTED_JOBS = SUCCESS_JOBS | {UPDATER_JOB, PUBLICATION_JOB}


class RecoveryError(Exception):
    """A fixed-message failure that does not expose API bodies or credentials."""


def positive_integer(value):
    return type(value) is int and value > 0


class GitHub:
    def __init__(self):
        self.deadline = time.monotonic() + 90

    def get(self, path):
        # Never invoke a shell or use a response's URL. Bound both retained bytes and
        # total elapsed time; a broken CLI cannot hold the publication gate indefinitely.
        deadline = min(self.deadline, time.monotonic() + 15)
        if deadline <= time.monotonic():
            raise RecoveryError("Release build verification exceeded its deadline")
        process = subprocess.Popen(
            ["gh", "api", "--method", "GET", "-H",
             "X-GitHub-Api-Version: 2022-11-28", path],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
        body = bytearray()
        try:
            with selectors.DefaultSelector() as selector, process.stdout:
                selector.register(process.stdout, selectors.EVENT_READ)
                while True:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not selector.select(remaining):
                        raise RecoveryError("Release build API request timed out")
                    chunk = os.read(process.stdout.fileno(), 64 * 1024)
                    if not chunk:
                        break
                    body.extend(chunk)
                    if len(body) > MAX_RESPONSE_BYTES:
                        raise RecoveryError("Release build API response exceeded its limit")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise RecoveryError("Release build API request timed out")
            if process.wait(timeout=remaining) != 0:
                raise RecoveryError("Could not read release build metadata")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
        try:
            return json.loads(body)
        except (ValueError, RecursionError) as error:
            raise RecoveryError("Invalid release build API response") from error


def validate_inputs(environment):
    repository = environment.get("GITHUB_REPOSITORY", "")
    tag = environment.get("TAG", "")
    run_id = environment.get("RELEASE_BUILD_RUN_ID", "")
    if (len(repository) > 140
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+", repository)
            or repository.split("/")[1] in {".", ".."}):
        raise RecoveryError("Invalid release repository")
    if (len(tag) > 100
            or not re.fullmatch(
                r"v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
                r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?", tag)
            or any(re.fullmatch(r"0[0-9]+", part)
                   for part in tag.partition("-")[2].split("."))):
        raise RecoveryError("Invalid canonical release tag")
    if not re.fullmatch(r"[1-9][0-9]{0,19}", run_id):
        raise RecoveryError("Invalid release build run ID")
    return repository, tag, int(run_id)


def tag_commit(api, repository, tag):
    reference = api.get(f"repos/{repository}/git/ref/tags/{tag}")
    if not isinstance(reference, dict) or reference.get("ref") != f"refs/tags/{tag}":
        raise RecoveryError("Release tag reference differs from TAG")
    target = reference.get("object")
    visited = set()
    for _ in range(MAX_TAG_DEPTH + 1):
        if (not isinstance(target, dict)
                or not isinstance(target.get("sha"), str)
                or not re.fullmatch(r"[0-9a-f]{40}", target["sha"])):
            raise RecoveryError("Invalid release tag object")
        digest = target["sha"]
        if target.get("type") == "commit":
            return digest
        if target.get("type") != "tag" or digest in visited or len(visited) >= MAX_TAG_DEPTH:
            raise RecoveryError("Release tag chain is invalid or exceeds its limit")
        visited.add(digest)
        annotated = api.get(f"repos/{repository}/git/tags/{digest}")
        if not isinstance(annotated, dict) or annotated.get("sha") != digest:
            raise RecoveryError("Annotated release tag identity differs")
        target = annotated.get("object")
    raise RecoveryError("Release tag chain exceeds its limit")


def validate_run(run, repository, tag, run_id, commit):
    if (not isinstance(run, dict) or type(run.get("id")) is not int or run["id"] != run_id
            or run.get("path") != WORKFLOW_PATH or run.get("event") != "push"
            or run.get("head_branch") != tag or run.get("head_sha") != commit
            or run.get("status") != "completed"
            or run.get("conclusion") not in ("success", "failure")
            or not positive_integer(run.get("run_attempt"))
            or not isinstance(run.get("repository"), dict)
            or run["repository"].get("full_name") != repository
            or not isinstance(run.get("head_repository"), dict)
            or run["head_repository"].get("full_name") != repository):
        raise RecoveryError("Release build must be a completed tag push from this repository and workflow")


def check(environment):
    repository, tag, run_id = validate_inputs(environment)
    api = GitHub()
    commit = tag_commit(api, repository, tag)
    run_path = f"repos/{repository}/actions/runs/{run_id}"
    run = api.get(run_path)
    validate_run(run, repository, tag, run_id, commit)
    jobs = {}
    identifiers = set()
    total = None
    for page in range(1, MAX_JOBS // 100 + 2):
        result = api.get(f"{run_path}/jobs?per_page=100&page={page}&filter=all")
        if (not isinstance(result, dict) or type(result.get("total_count")) is not int
                or not 0 <= result["total_count"] <= MAX_JOBS
                or total is not None and total != result["total_count"]
                or not isinstance(result.get("jobs"), list) or len(result["jobs"]) > 100):
            raise RecoveryError("Invalid or unexpectedly large release build job list")
        total = result["total_count"]
        for job in result["jobs"]:
            if (not isinstance(job, dict) or not positive_integer(job.get("id"))
                    or job["id"] in identifiers or not isinstance(job.get("name"), str)
                    or job["name"] not in EXPECTED_JOBS or job["name"] in jobs
                    or type(job.get("run_id")) is not int or job["run_id"] != run_id
                    or type(job.get("run_attempt")) is not int
                    or job["run_attempt"] != run["run_attempt"]
                    or job.get("head_sha") != commit or job.get("status") != "completed"):
                raise RecoveryError("Release build jobs are incomplete, ambiguous or belong to another run")
            identifiers.add(job["id"])
            jobs[job["name"]] = job
        if len(jobs) > total:
            raise RecoveryError("Release build job count differs from API metadata")
        if len(result["jobs"]) < 100:
            break
    if len(jobs) != total or set(jobs) != EXPECTED_JOBS:
        raise RecoveryError("Release build is missing required jobs")
    if (any(jobs[name].get("conclusion") != "success" for name in SUCCESS_JOBS)
            or jobs[UPDATER_JOB].get("conclusion") not in ("success", "skipped")
            or jobs[PUBLICATION_JOB].get("conclusion") not in ("success", "failure")):
        raise RecoveryError("Every build, upload, notes and enabled updater job must have succeeded")
    # Recheck after pagination so a concurrently restarted run or moved tag cannot be
    # mistaken for the completed build whose job results were just verified.
    current = api.get(run_path)
    validate_run(current, repository, tag, run_id, commit)
    if current["run_attempt"] != run["run_attempt"] or tag_commit(api, repository, tag) != commit:
        raise RecoveryError("Release build attempt or tag changed during verification")
    print(f"Verified original release build {run_id} for {repository} {tag} at {commit}; no changes made")


if __name__ == "__main__":
    try:
        check(os.environ)
    except (RecoveryError, OSError, subprocess.SubprocessError) as error:
        message = str(error) if isinstance(error, RecoveryError) else "Could not verify release build metadata"
        print(f"error: {message}", file=sys.stderr)
        sys.exit(1)
