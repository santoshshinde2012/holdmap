#!/usr/bin/env python3
"""Check native fullscreen close and retained window identity in a private owned .app.

No installed Holdmap app, user store, scans, shortcuts, recording, power controls,
Accessibility or Screen Recording permissions are used. Only the owned process is
ever signalled. The fixture attempts window restoration on completion and errors;
the runner force-stops only its owned child if the outer deadline expires.
macOS cooperative activation may prevent the foreground precondition; that physical
case is explicitly skipped, while ordinary secondary/no-tray destruction still runs.
"""

import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tempfile
import time


REPO = Path(__file__).resolve().parents[1]


def command(arguments, timeout=180):
    result = subprocess.run(arguments, cwd=REPO, capture_output=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace")[:8000])
    return result


def run():
    if platform.system() != "Darwin":
        raise SystemExit("This owned window lifecycle runtime proof requires macOS.")
    metadata = json.loads(command(["cargo", "metadata", "--no-deps", "--format-version", "1"]).stdout)
    command(["cargo", "build", "-p", "holdmap-desktop", "--example", "window_lifecycle_runtime", "--locked"])
    harness = Path(metadata["target_directory"]) / "debug/examples/window_lifecycle_runtime"
    previous_umask = os.umask(0o077)
    try:
        with tempfile.TemporaryDirectory(prefix="holdmap-window-runtime-") as temporary:
            root = Path(temporary).resolve()
            os.chmod(root, 0o700)
            (root / "owned-fixture").write_text("holdmap window fixture")
            app = root / "Scratch.app"
            (app / "Contents/MacOS").mkdir(parents=True)
            executable = app / "Contents/MacOS/window_lifecycle_runtime"
            shutil.copy2(harness, executable)
            identifier = f"dev.holdmap.windowfixture.{os.getpid()}"
            (app / "Contents/Info.plist").write_bytes(plistlib.dumps({
                "CFBundleExecutable": "window_lifecycle_runtime",
                "CFBundleIdentifier": identifier,
                "CFBundlePackageType": "APPL",
                "CFBundleInfoDictionaryVersion": "6.0",
                "CFBundleName": "Owned Holdmap window fixture",
                "CFBundleVersion": "1",
                "CFBundleShortVersionString": "1.0.0",
                "NSHighResolutionCapable": True,
            }))
            command(["/usr/bin/codesign", "--force", "--deep", "--sign", "-", str(app)], timeout=30)
            result_file = root / "result.json"
            fixture = root / "fixture.json"
            fixture.write_text(json.dumps({"root": str(root), "app": str(app), "result": str(result_file), "identifier": identifier}))
            environment = os.environ.copy()
            # Defense in depth if an example accidentally starts using core state later.
            environment["HOLDMAP_HOME"] = str(root / "unused-store")
            child = subprocess.Popen(
                [str(executable), "--fixture", str(fixture)],
                cwd=root,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            try:
                deadline = time.monotonic() + 60
                opened_requests = set()
                while True:
                    for request in ("bootstrap-open",):
                        if request in opened_requests or not (root / request).exists():
                            continue
                        opened = subprocess.run(
                            ["/usr/bin/open", "-a", str(app)],
                            cwd=root,
                            capture_output=True,
                            timeout=5,
                        )
                        assert opened.returncode == 0, opened.stderr
                        opened_requests.add(request)
                    if time.monotonic() >= deadline:
                        raise RuntimeError("owned window lifecycle fixture exceeded its deadline")
                    try:
                        stdout, stderr = child.communicate(timeout=0.1)
                        break
                    except subprocess.TimeoutExpired:
                        pass
            finally:
                if child.poll() is None:
                    child.kill()
                child.wait(timeout=5)
            assert child.returncode == 0, (child.returncode, stdout, stderr)
            assert result_file.exists(), (stdout, stderr)
            result = json.loads(result_file.read_text())
            print(json.dumps(result, indent=2), flush=True)
            assert result["error"] is None, result["error"]
            assert result["cleanup_error"] is None, result["cleanup_error"]
            assert result["normal_closes_error"] is None, result["normal_closes_error"]
            assert result["normal_closes"] == {"secondary_destroyed": True, "main_destroyed_without_tray": True}
            assert "bootstrap-open" in opened_requests
            assert result["dock_reopen_count"] >= 1
            assert not (root / "unused-store").exists(), "fixture unexpectedly accessed a user store"
            states = {stage["name"]: stage["state"] for stage in result["stages"]}
            for identity in ("native_window", "native_view", "native_webview"):
                assert len({state[identity] for state in states.values()}) == 1
                assert all(state[identity] != 0 for state in states.values())
            assert not states["restored"]["fullscreen"]
            print("PASS secondary and no-tray windows close normally; owned fixture cleaned up")
            if result["activation_unavailable"]:
                print("SKIP physical fullscreen close: macOS cooperative background activation unavailable")
            else:
                closed = states["fullscreen_closed"]
                assert closed["fullscreen"]
                assert not closed["application_active"]
                assert not closed["on_active_space"]
                assert states["reopen_observation"]["visible"]
                print("PASS native fullscreen close leaves the Space inactive and retains the same window/webview")
    finally:
        os.umask(previous_umask)


if __name__ == "__main__":
    run()
