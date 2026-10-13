#!/usr/bin/env python3
"""macOS: genuine signed updater install/relaunch in an owned scratch .app.

Run after npm ci --prefix apps/desktop. No installed application, login settings,
permissions, or release service is changed. Keys and artifacts live only in a
private temporary directory. The example imports production update.rs unchanged.
"""

import datetime
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import threading
import time


REPO = Path(__file__).resolve().parents[1]
TAURI = REPO / "apps/desktop/node_modules/.bin/tauri"
MARKER_SOURCE = r"""
use std::{env, fs, path::PathBuf};
fn main() {
    let marker = PathBuf::from(env::var_os("HOLDMAP_UPDATER_E2E_MARKER").unwrap());
    let expected = PathBuf::from(env::var_os("HOLDMAP_UPDATER_E2E_EXECUTABLE").unwrap());
    let actual = env::current_exe().unwrap().canonicalize().unwrap();
    assert_eq!(actual, expected.canonicalize().unwrap());
    fs::write(marker, format!("{}\n{}\n", std::process::id(), actual.display())).unwrap();
}
"""


def command(arguments, env=None, timeout=180, cwd=REPO, sensitive=False):
    result = subprocess.run(
        [str(argument) for argument in arguments],
        cwd=cwd,
        env=env,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    if result.returncode:
        # Key-generation output can contain private material; never print it.
        if not sensitive:
            print(result.stderr.decode(errors="replace")[:8000])
        raise RuntimeError(
            f"{Path(str(arguments[0])).name} failed with status {result.returncode}"
        )
    return result


def fingerprint(directory):
    return [
        (
            str(path.relative_to(directory)),
            path.stat().st_mode,
            hashlib.sha256(path.read_bytes()).hexdigest(),
        )
        for path in sorted(directory.rglob("*"))
        if path.is_file()
    ]


def run():
    if platform.system() != "Darwin":
        raise SystemExit("This scratch application runtime proof requires macOS.")
    if not TAURI.exists():
        raise SystemExit("Run npm ci --prefix apps/desktop before this proof.")

    metadata = json.loads(
        command(["cargo", "metadata", "--no-deps", "--format-version", "1"]).stdout
    )
    current = next(
        package["version"]
        for package in metadata["packages"]
        if package["name"] == "holdmap-desktop"
    )
    major, minor, patch = map(int, current.split("-")[0].split("+")[0].split("."))
    future = f"{major}.{minor}.{patch + 1}"
    command(["cargo", "build", "-p", "holdmap-desktop", "--example", "updater_runtime", "--locked"])
    harness = Path(metadata["target_directory"]) / "debug/examples/updater_runtime"

    # This changes only this test process's umask; all children and private keys inherit it.
    previous_umask = os.umask(0o077)
    try:
        with (
            tempfile.TemporaryDirectory(prefix="holdmap-updater-runtime-") as temporary,
            tempfile.TemporaryDirectory(prefix="holdmap-updater-scope-") as outside,
        ):
            root = Path(temporary).resolve()
            os.chmod(root, 0o700)
            (root / "owned-fixture").write_text("holdmap updater fixture")
            outside_sentinel = Path(outside) / "unchanged.txt"
            outside_sentinel.write_text("owned outside sentinel")
            marker_source = root / "marker.rs"
            marker_source.write_text(MARKER_SOURCE)
            marker_binary = root / "marker"
            command(["rustc", marker_source, "--crate-name", "holdmap_updater_marker", "-o", marker_binary])

            signer_env = os.environ.copy()
            for name in (
                "TAURI_SIGNING_PRIVATE_KEY",
                "TAURI_SIGNING_PRIVATE_KEY_PATH",
                "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
            ):
                signer_env.pop(name, None)
            signer_env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = ""
            keys = []
            for name in ("a", "b"):
                key = root / f"{name}.key"
                key.touch(mode=0o600)
                command(
                    [TAURI, "signer", "generate", "--write-keys", key, "--password", "", "--force", "--ci"],
                    env=signer_env,
                    timeout=30,
                    sensitive=True,
                )
                os.chmod(key, 0o600)
                assert key.stat().st_mode & 0o777 == 0o600
                keys.append((key, key.with_suffix(".key.pub").read_text().strip()))

            payload = root / "payload/Updated.app"
            (payload / "Contents/MacOS").mkdir(parents=True)
            shutil.copy2(marker_binary, payload / "Contents/MacOS/updated-marker")
            plist = {
                "CFBundleExecutable": "updated-marker",
                "CFBundleIdentifier": "dev.holdmap.updaterfixture",
                "CFBundleName": "Owned updater fixture",
                "CFBundleVersion": future,
                "CFBundleShortVersionString": future,
                "LSUIElement": True,
            }
            (payload / "Contents/Info.plist").write_bytes(plistlib.dumps(plist))
            (payload / "Contents/fixture-version").write_text(future)
            archive = root / "update.app.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                bundle.add(payload, arcname="Updated.app")

            signatures = {}
            for version in (future, current):
                command(
                    [TAURI, "signer", "sign", "--private-key-path", keys[0][0], "--app-version", version, archive],
                    env=signer_env,
                    timeout=30,
                    sensitive=True,
                )
                signatures[version] = Path(f"{archive}.sig").read_text().strip()
            artifact = archive.read_bytes()
            tampered = bytearray(artifact)
            tampered[len(tampered) // 2] ^= 1
            routes = {}
            requests = []

            class Handler(http.server.BaseHTTPRequestHandler):
                def do_GET(self):
                    self.connection.settimeout(5)
                    requests.append(self.path)
                    body = routes.get(self.path)
                    self.send_response(200 if body is not None else 404)
                    self.send_header("Content-Length", str(len(body or b"")))
                    self.end_headers()
                    if body is not None:
                        self.wfile.write(body)

                def log_message(self, *_):
                    pass

            server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            server.daemon_threads = True
            worker = threading.Thread(target=server.serve_forever, daemon=True)
            worker.start()
            port = server.server_address[1]
            target = "darwin-aarch64" if platform.machine() == "arm64" else "darwin-x86_64"
            errors = {
                "wrong-key": "different key",
                "tampered": "signature verification failed",
                "wrong-version": "was signed for version",
            }
            try:
                for case in ("escaped-result", *errors, "success"):
                    directory = root / case
                    app = directory / "Scratch.app"
                    (app / "Contents/MacOS").mkdir(parents=True)
                    shutil.copy2(harness, app / "Contents/MacOS/updater_runtime")
                    original_plist = {
                        **plist,
                        "CFBundleExecutable": "updater_runtime",
                        "CFBundleVersion": current,
                        "CFBundleShortVersionString": current,
                    }
                    (app / "Contents/Info.plist").write_bytes(plistlib.dumps(original_plist))
                    (app / "Contents/fixture-version").write_text(current)
                    before = fingerprint(app)
                    download = f"/artifact/{case}"
                    manifest = f"/manifest/{case}"
                    routes[download] = bytes(tampered) if case == "tampered" else artifact
                    entry = {
                        "url": f"http://127.0.0.1:{port}{download}",
                        "signature": signatures[current if case == "wrong-version" else future],
                    }
                    routes[manifest] = json.dumps({
                        "version": future,
                        "notes": "owned runtime fixture",
                        "pub_date": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                        "platforms": {target: entry, f"{target}-app": entry},
                    }).encode()
                    result_file = directory / "result.json"
                    if case == "escaped-result":
                        (directory / "result-link").symlink_to(outside, target_is_directory=True)
                        result_file = directory / "result-link/unchanged.txt"
                    marker = directory / "restarted.marker"
                    fixture_file = directory / "fixture.json"
                    fixture_file.write_text(json.dumps({
                        "root": str(root),
                        "app": str(app),
                        "endpoint": f"http://127.0.0.1:{port}{manifest}",
                        "public_key": keys[1 if case == "wrong-key" else 0][1],
                        "result": str(result_file),
                    }))
                    environment = os.environ.copy()
                    # Only the owned subprocess changes its proxy environment. Native
                    # updater code stays unchanged and the fixture connects directly.
                    for name in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
                        environment.pop(name, None)
                    environment["HOLDMAP_UPDATER_E2E_MARKER"] = str(marker)
                    environment["HOLDMAP_UPDATER_E2E_EXECUTABLE"] = str(app / "Contents/MacOS/updated-marker")
                    child = subprocess.Popen(
                        [str(app / "Contents/MacOS/updater_runtime"), "--fixture", str(fixture_file)],
                        cwd=directory,
                        env=environment,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                    )
                    try:
                        try:
                            stdout, stderr = child.communicate(timeout=45)
                        except subprocess.TimeoutExpired:
                            raise RuntimeError(f"{case}: owned updater exceeded its deadline") from None
                    finally:
                        # Also covers interruption and unexpected capture failures; only
                        # the process represented by our owned Popen is ever signalled.
                        if child.poll() is None:
                            child.kill()
                        child.wait(timeout=5)
                    if case == "escaped-result":
                        assert child.returncode != 0
                        assert b"result directory escaped" in stderr, (stdout, stderr)
                        assert manifest not in requests and download not in requests
                        assert outside_sentinel.read_text() == "owned outside sentinel"
                        assert fingerprint(app) == before
                        assert not marker.exists()
                        print("PASS result symlink escape refused before network or mutation")
                        continue
                    assert child.returncode == 0, (case, child.returncode, stdout, stderr)
                    assert manifest in requests and download in requests, (case, requests)
                    if case == "success":
                        deadline = time.monotonic() + 5
                        while not marker.exists() and time.monotonic() < deadline:
                            time.sleep(0.01)
                        assert marker.exists(), "Tauri restart did not launch the replacement binary"
                        lines = marker.read_text().splitlines()
                        assert int(lines[0]) != child.pid
                        assert Path(lines[1]).resolve() == (app / "Contents/MacOS/updated-marker").resolve()
                        assert (app / "Contents/fixture-version").read_text() == future
                        assert not (app / "Contents/MacOS/updater_runtime").exists()
                        assert (app / "Contents/MacOS/updated-marker").read_bytes() == marker_binary.read_bytes()
                        assert not result_file.exists()
                        print("PASS genuine signed install and Tauri restart into replacement marker")
                    else:
                        result = json.loads(result_file.read_text())
                        assert errors[case] in result["error"], (case, result)
                        assert not result["installing"], (case, "install latch remained held")
                        assert fingerprint(app) == before, (case, "scratch installation changed")
                        assert not marker.exists()
                        print(f"PASS {case} refused before installation")
            finally:
                server.shutdown()
                server.server_close()
                worker.join(timeout=5)
    finally:
        os.umask(previous_umask)


if __name__ == "__main__":
    run()
