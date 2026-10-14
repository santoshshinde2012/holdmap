#!/usr/bin/env python3
"""Encode a synthetic canvas in an owned WKWebView using production modules and CSP.

No display picker, user-screen capture, native save chooser, user store or OS permission
is used. The fixture-only binary IPC sink writes inside its private root; production
save-dialog validation and persistence have separate native unit coverage.
"""

import hashlib
import html
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tempfile


REPO = Path(__file__).resolve().parents[1]
DESKTOP = REPO / "apps/desktop"
FRONTEND = r"""
import { invoke } from __IPC__;

const violations = [];
document.addEventListener("securitypolicyviolation", event => {
  violations.push({ directive: event.violatedDirective, blocked: event.blockedURI });
});
const report = {
  support: null,
  videoEncoder: typeof VideoEncoder === "function",
  mediaRecorder: typeof MediaRecorder === "function",
  userAgent: navigator.userAgent,
  screenPickerInvoked: false,
  files: {},
  violations,
};
// Defense in depth: an accidental future picker call cannot capture the user's display.
if (typeof navigator.mediaDevices?.getDisplayMedia === "function") {
  navigator.mediaDevices.getDisplayMedia = () => {
    report.screenPickerInvoked = true;
    return Promise.reject(new Error("The synthetic fixture refuses real display capture."));
  };
}
let stream;
let session;
let timer;
try {
  // Load the production capture modules only after the display-picker tripwire is installed.
  const [{ ScreenRecording }, { captureSupport }, { saveRecording }] = await Promise.all([
    import(__ENCODER__), import(__CAPTURE__), import(__SAVE__),
  ]);
  report.support = captureSupport();
  const canvas = document.querySelector("canvas");
  const context = canvas.getContext("2d");
  let frame = 0;
  const paint = () => {
    context.fillStyle = frame % 2 ? "#172554" : "#052e16";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.fillStyle = "#facc15";
    context.fillRect((frame * 7) % (canvas.width - 40), 45, 40, 60);
    context.fillStyle = "white";
    context.font = "20px sans-serif";
    context.fillText(`Synthetic frame ${frame++}`, 16, 26);
  };
  paint();
  timer = setInterval(paint, 50);
  // A generated canvas is the only capture source. Never call getDisplayMedia.
  stream = canvas.captureStream(20);
  session = await ScreenRecording.start(stream, { onTick() {}, onEnded() {} });
  await new Promise(resolve => setTimeout(resolve, 2600));
  const result = await session.stop();
  report.durationMs = result.durationMs;
  report.width = result.width;
  report.height = result.height;
  report.warnings = result.warnings;
  report.tracksEnded = stream.getTracks().every(track => track.readyState === "ended");
  report.hiddenVideosRemoved = document.querySelectorAll("video").length === 0;
  for (const format of ["mp4", "gif"]) {
    const file = result[format];
    if (!file) throw new Error(`${format.toUpperCase()} output unavailable: ${result.warnings.join("; ")}`);
    const bytes = await file.blob.arrayBuffer();
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    const sha256 = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("");
    const saved = await saveRecording({ ...file, filename: `holdmap-runtime.${format}` });
    if (!saved.saved || !saved.path) throw new Error("Native binary IPC did not report a completed fixture save");
    report.files[format] = { bytes: bytes.byteLength, sha256, type: file.blob.type };
  }
  const discardStream = canvas.captureStream(20);
  const discarded = await ScreenRecording.start(discardStream, { onTick() {}, onEnded() {} });
  discarded.discard();
  report.discardTracksEnded = discardStream.getTracks().every(track => track.readyState === "ended");
  report.discardVideosRemoved = document.querySelectorAll("video").length === 0;
  report.status = "passed";
} catch (cause) {
  report.status = "failed";
  report.error = String(cause?.stack || cause).slice(0, 4000);
} finally {
  clearInterval(timer);
  session?.discard();
  stream?.getTracks().forEach(track => track.stop());
  await invoke("finish_fixture", { report });
}
"""


def command(arguments, timeout=180, env=None):
    result = subprocess.run(
        [str(argument) for argument in arguments],
        cwd=REPO,
        env=env,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace")[-8000:])
    return result


def run():
    if platform.system() != "Darwin":
        raise SystemExit("This owned synthetic WKWebView check requires macOS.")
    for tool in ("cargo", "node", "ffprobe", "ffmpeg"):
        if shutil.which(tool) is None:
            raise SystemExit(f"Missing prerequisite: {tool}")
    vite = DESKTOP / "node_modules/vite/bin/vite.js"
    if not vite.is_file():
        raise SystemExit("Run npm ci --prefix apps/desktop first.")
    metadata = json.loads(command(["cargo", "metadata", "--no-deps", "--format-version", "1"]).stdout)
    command(["cargo", "build", "-p", "holdmap-desktop", "--example", "screen_recording_runtime", "--locked"])
    harness = Path(metadata["target_directory"]) / "debug/examples/screen_recording_runtime"
    csp = json.loads((DESKTOP / "src-tauri/tauri.conf.json").read_text())["app"]["security"]["csp"]
    assert "script-src 'self'" in csp and "worker-src 'self' blob:" in csp
    previous_umask = os.umask(0o077)
    try:
        with tempfile.TemporaryDirectory(prefix="holdmap-screen-runtime-") as temporary:
            root = Path(temporary).resolve()
            os.chmod(root, 0o700)
            (root / "owned-fixture").write_text("holdmap synthetic screen fixture")
            frontend = root / "frontend"
            frontend.mkdir()
            source = FRONTEND
            for token, filename in {
                "__ENCODER__": DESKTOP / "src/lib/screen-record-encode.ts",
                "__CAPTURE__": DESKTOP / "src/lib/screen-record.ts",
                "__SAVE__": DESKTOP / "src/lib/screen-record-save.ts",
                "__IPC__": DESKTOP / "node_modules/@tauri-apps/api/core.js",
            }.items():
                source = source.replace(token, json.dumps(str(filename)))
            (frontend / "fixture.js").write_text(source)
            (frontend / "index.html").write_text(
                '<!doctype html><html lang="en"><head><meta charset="utf-8">'
                f'<meta http-equiv="Content-Security-Policy" content="{html.escape(csp, quote=True)}">'
                '<title>Owned synthetic recording fixture</title></head><body>'
                '<p>Generated canvas only. No screen is being captured.</p>'
                '<canvas width="320" height="180"></canvas>'
                '<script type="module" src="/fixture.js"></script></body></html>'
            )
            assets = root / "assets"
            config = root / "vite.config.mjs"
            config.write_text("export default " + json.dumps({
                "root": str(frontend),
                "base": "/",
                "build": {"outDir": str(assets), "emptyOutDir": True, "target": "safari15", "assetsInlineLimit": 0},
                "worker": {"format": "es"},
            }))
            command(["node", vite, "build", "--config", config], timeout=60)
            app = root / "Scratch.app"
            (app / "Contents/MacOS").mkdir(parents=True)
            executable = app / "Contents/MacOS/screen_recording_runtime"
            shutil.copy2(harness, executable)
            identifier = f"dev.holdmap.recordingfixture.{os.getpid()}"
            (app / "Contents/Info.plist").write_bytes(plistlib.dumps({
                "CFBundleExecutable": "screen_recording_runtime",
                "CFBundleIdentifier": identifier,
                "CFBundleName": "Owned Holdmap synthetic recording fixture",
                "CFBundlePackageType": "APPL",
                "CFBundleVersion": "1",
                "CFBundleShortVersionString": "1.0.0",
                "NSHighResolutionCapable": True,
                "LSUIElement": True,
            }))
            command(["/usr/bin/codesign", "--force", "--deep", "--sign", "-", app], timeout=30)
            fixture = root / "fixture.json"
            fixture.write_text(json.dumps({"root": str(root), "app": str(app), "assets": str(assets), "identifier": identifier}))
            environment = os.environ.copy()
            environment["HOLDMAP_HOME"] = str(root / "unused-store")
            child = subprocess.Popen(
                [str(executable), "--fixture", str(fixture)], cwd=root, env=environment,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            try:
                stdout, stderr = child.communicate(timeout=60)
            finally:
                if child.poll() is None:
                    child.kill()
                child.wait(timeout=5)
            if child.returncode:
                raise AssertionError((child.returncode, stdout[-4000:], stderr[-4000:]))
            result = json.loads((root / "result.json").read_text())
            print(json.dumps(result, indent=2), flush=True)
            browser = result["browser"]
            assert browser["status"] == "passed", browser.get("error")
            assert not browser["screenPickerInvoked"] and not browser["violations"]
            for check in ("tracksEnded", "hiddenVideosRemoved", "discardTracksEnded", "discardVideosRemoved"):
                assert browser[check], check
            assert not (root / "unused-store").exists(), "fixture unexpectedly accessed a user store"
            for format in ("mp4", "gif"):
                file = root / f"holdmap-runtime.{format}"
                payload = file.read_bytes()
                observed = {"bytes": len(payload), "sha256": hashlib.sha256(payload).hexdigest()}
                assert observed == result["native_files"][file.name]
                assert observed == {key: browser["files"][format][key] for key in observed}
                assert file.stat().st_mode & 0o777 == 0o600
                if format == "mp4":
                    assert payload[4:8] == b"ftyp"
                else:
                    assert payload[:6] in (b"GIF87a", b"GIF89a")
                probe = json.loads(command([
                    "ffprobe", "-v", "error", "-count_frames", "-show_entries",
                    "stream=codec_name,codec_type,width,height,nb_read_frames:format=duration", "-of", "json", file,
                ], timeout=20).stdout)
                assert len(probe["streams"]) == 1, probe
                video = probe["streams"][0]
                assert video["codec_type"] == "video"
                assert video["codec_name"] == ("h264" if format == "mp4" else "gif")
                assert (video["width"], video["height"]) == (320, 180)
                assert int(video["nb_read_frames"]) >= 3
                assert 1 <= float(probe["format"]["duration"]) <= 8, probe
                decoded = command(["ffmpeg", "-v", "error", "-i", file, "-f", "framemd5", "-"], timeout=20).stdout.decode()
                hashes = {line.rsplit(",", 1)[-1].strip() for line in decoded.splitlines() if line and not line.startswith("#")}
                assert len(hashes) >= 2, f"{format}: encoded canvas never changed"
                print(f"PASS {format.upper()}: {json.dumps(probe, separators=(',', ':'))}; exact binary IPC digest; animated frames", flush=True)
            print("PASS production encoder and save adapter in WKWebView with strict CSP; stop/discard release tracks; owned fixture cleaned up")
    finally:
        os.umask(previous_umask)


if __name__ == "__main__":
    run()
