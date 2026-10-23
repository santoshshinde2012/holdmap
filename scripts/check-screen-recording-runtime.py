#!/usr/bin/env python3
"""Encode a synthetic canvas in an owned WKWebView using production modules and CSP.

No display picker, user-screen capture, native save chooser, user store or OS permission
is used. The fixture-only binary IPC sink writes inside its private root; production
save-dialog validation and persistence have separate native unit coverage.
Use --compare-capabilities to inspect build and webview differences without capturing.
Use --app-ui to verify recorder gates and export only the owned App through its PNG UI.
App UI mode brings only its owned scratch window to the foreground before interacting.
"""

import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import tempfile
import time


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
  url: location.href,
  secureContext: globalThis.isSecureContext,
  originalAPIs: {
    mediaDevices: typeof navigator.mediaDevices,
    getDisplayMedia: typeof navigator.mediaDevices?.getDisplayMedia,
    getUserMedia: typeof navigator.mediaDevices?.getUserMedia,
    canvasCaptureStream: typeof HTMLCanvasElement.prototype.captureStream,
  },
  screenPickerInvoked: false,
  files: {},
  violations,
};
// Defense in depth: an accidental future picker call cannot capture the user's display.
for (const name of ["getDisplayMedia", "getUserMedia"]) {
  if (typeof navigator.mediaDevices?.[name] === "function") {
    navigator.mediaDevices[name] = () => {
      report.screenPickerInvoked = true;
      return Promise.reject(new Error("The synthetic fixture refuses real device capture."));
    };
  }
}
let stream;
let session;
let timer;
try {
  // Load the production capture modules only after the display-picker tripwire is installed.
  const { captureSupport } = await import(__CAPTURE__);
  report.support = captureSupport();
  report.nativeCapabilities = await invoke("screen_recording_capabilities");
  if (!__CAPABILITIES_ONLY__) {
  const [{ ScreenRecording }, { saveRecording, saveSnapshot }] = await Promise.all([
    import(__ENCODER__), import(__SAVE__),
  ]);
  const canvas = document.querySelector("canvas");
  const context = canvas.getContext("2d");
  let frame = 0;
  let lastPaintAt = performance.now();
  let maxPaintGapMs = 0;
  const paint = () => {
    const now = performance.now();
    maxPaintGapMs = Math.max(maxPaintGapMs, now - lastPaintAt);
    lastPaintAt = now;
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
  const startRequestedAt = performance.now();
  session = await ScreenRecording.start(stream, { onTick() {}, onEnded() {} });
  const waitStartedAt = performance.now();
  const firstPaint = frame;
  const visibilityAtStart = document.visibilityState;
  lastPaintAt = waitStartedAt;
  maxPaintGapMs = 0;
  await new Promise(resolve => setTimeout(resolve, 2600));
  const waitEndedAt = performance.now();
  report.syntheticTiming = {
    startupMs: Math.round(waitStartedAt - startRequestedAt),
    requestedWaitMs: 2600,
    actualWaitMs: Math.round(waitEndedAt - waitStartedAt),
    paintedFrames: frame - firstPaint,
    maxPaintGapMs: Math.round(Math.max(maxPaintGapMs, waitEndedAt - lastPaintAt)),
    visibilityAtStart,
    visibilityAtStop: document.visibilityState,
  };
  const snapshot = await session.captureSnapshot(new AbortController().signal);
  report.snapshotKeptVideoCapturing = stream.getTracks().every(track => track.readyState === "live");
  if (!report.snapshotKeptVideoCapturing) throw new Error("Taking a PNG snapshot stopped the recording source");
  const result = await session.stop();
  report.durationMs = result.durationMs;
  report.width = result.width;
  report.height = result.height;
  report.warnings = result.warnings;
  report.tracksEnded = stream.getTracks().every(track => track.readyState === "ended");
  report.hiddenVideosRemoved = document.querySelectorAll("video").length === 0;
  for (const format of ["mp4", "gif", "png"]) {
    const file = format === "png" ? snapshot.png : result[format];
    if (!file) throw new Error(`${format.toUpperCase()} output unavailable: ${result.warnings.join("; ")}`);
    const bytes = await file.blob.arrayBuffer();
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    const sha256 = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("");
    const named = { ...file, filename: `holdmap-runtime.${format}` };
    const saved = await (format === "png" ? saveSnapshot(named) : saveRecording(named));
    if (!saved.saved || !saved.path) throw new Error("Native binary IPC did not report a completed fixture save");
    report.files[format] = { bytes: bytes.byteLength, sha256, type: file.blob.type };
  }
  const discardStream = canvas.captureStream(20);
  const discarded = await ScreenRecording.start(discardStream, { onTick() {}, onEnded() {} });
  discarded.discard();
  report.discardTracksEnded = discardStream.getTracks().every(track => track.readyState === "ended");
  report.discardVideosRemoved = document.querySelectorAll("video").length === 0;
  }
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


def run(
    *, custom_protocol=True, incognito=False, capabilities_only=False,
    release_profile=False, hardened=True, devtools=None, linked_sdk=None, app_ui=False,
):
    capabilities_only = capabilities_only or app_ui
    if platform.system() != "Darwin":
        raise SystemExit("This owned synthetic WKWebView check requires macOS.")
    tools = ("cargo", "node") if capabilities_only and not app_ui else ("cargo", "node", "ffprobe", "ffmpeg")
    for tool in tools:
        if shutil.which(tool) is None:
            raise SystemExit(f"Missing prerequisite: {tool}")
    vite = DESKTOP / "node_modules/vite/bin/vite.js"
    if not vite.is_file():
        raise SystemExit("Run npm ci --prefix apps/desktop first.")
    metadata = json.loads(command(["cargo", "metadata", "--no-deps", "--format-version", "1"]).stdout)
    build = ["cargo", "build", "-p", "holdmap-desktop", "--example", "screen_recording_runtime", "--locked"]
    if custom_protocol:
        build.extend(["--features", "tauri/custom-protocol"])
    if release_profile:
        build.append("--release")
    command(build, timeout=600)
    harness = Path(metadata["target_directory"]) / ("release" if release_profile else "debug") / "examples/screen_recording_runtime"
    csp = json.loads((DESKTOP / "src-tauri/tauri.conf.json").read_text())["app"]["security"]["csp"]
    assert "script-src 'self'" in csp and "worker-src 'self' blob:" in csp
    previous_umask = os.umask(0o077)
    identifier = None
    try:
        with tempfile.TemporaryDirectory(prefix="holdmap-screen-runtime-") as temporary:
            root = Path(temporary).resolve()
            os.chmod(root, 0o700)
            (root / "owned-fixture").write_text("holdmap synthetic screen fixture")
            frontend = root / "frontend"
            frontend.mkdir()
            source = (REPO / "scripts/screen-recording-app-fixture.mjs").read_text() if app_ui else FRONTEND
            source = source.replace("__CAPABILITIES_ONLY__", json.dumps(capabilities_only))
            for token, filename in {
                "__ENCODER__": DESKTOP / "src/lib/screen-record-encode.ts",
                "__CAPTURE__": DESKTOP / "src/lib/screen-record.ts",
                "__SAVE__": DESKTOP / "src/lib/screen-record-save.ts",
                "__IPC__": DESKTOP / "node_modules/@tauri-apps/api/core.js",
                "__APP__": DESKTOP / "src/App.svelte",
                "__SVELTE__": DESKTOP / "node_modules/svelte/src/index-client.js",
                "__APP_CSS__": DESKTOP / "src/app.css",
                "__FONTS_CSS__": DESKTOP / "src/fonts.css",
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
            vite_config = {
                "root": str(frontend),
                "base": "/",
                "build": {"outDir": str(assets), "emptyOutDir": True, "target": "safari15", "assetsInlineLimit": 0},
                "worker": {"format": "es"},
            }
            if app_ui:
                vite_config["resolve"] = {"conditions": ["browser", "production"]}
                vite_config["publicDir"] = str(DESKTOP / "public")
                plugin = json.dumps(str(DESKTOP / "node_modules/@sveltejs/vite-plugin-svelte/src/index.js"))
                config.write_text(f"import {{ svelte }} from {plugin};\nconst config = " + json.dumps(vite_config)
                                  + ";\nconfig.plugins = [svelte({compilerOptions: {dev: false}})];\nexport default config;\n")
            else:
                config.write_text("export default " + json.dumps(vite_config))
            command(["node", vite, "build", "--config", config], timeout=60)
            app = root / "Scratch.app"
            (app / "Contents/MacOS").mkdir(parents=True)
            executable = app / "Contents/MacOS/screen_recording_runtime"
            shutil.copy2(harness, executable)
            if linked_sdk:
                patched = executable.with_suffix(".sdk-probe")
                command(["/usr/bin/xcrun", "vtool", "-set-build-version", "macos", "11.0", linked_sdk,
                         "-replace", "-output", patched, executable], timeout=30)
                patched.replace(executable)
            build_version = command(["/usr/bin/xcrun", "vtool", "-show-build", executable]).stdout.decode()
            dependencies = command(["/usr/bin/otool", "-L", executable]).stdout.decode()
            capture_framework = [line for line in dependencies.splitlines() if "ScreenCaptureKit.framework/" in line]
            assert len(capture_framework) == 1 and "weak" in capture_framework[0], "native fallback must preserve launch on older macOS"
            assert "libswift_Concurrency" not in dependencies, "native fallback unexpectedly requires Swift concurrency"
            identifier = f"dev.holdmap.recordingfixture.{os.getpid()}{time.time_ns()}"
            app_plist = plistlib.loads((DESKTOP / "src-tauri/Info.plist").read_bytes())
            app_plist.update({
                "CFBundleExecutable": "screen_recording_runtime",
                "CFBundleIdentifier": identifier,
                "CFBundleName": "Owned Holdmap synthetic recording fixture",
                "CFBundlePackageType": "APPL",
                "CFBundleVersion": "1",
                "CFBundleShortVersionString": "1.0.0",
                "NSHighResolutionCapable": True,
                "LSUIElement": True,
            })
            (app / "Contents/Info.plist").write_bytes(plistlib.dumps(app_plist))
            signing = ["/usr/bin/codesign", "--force", "--deep", "--sign", "-"]
            if hardened:
                signing.extend(["--options", "runtime", "--entitlements", DESKTOP / "src-tauri/Entitlements.plist"])
                if linked_sdk:
                    signing.extend(["--runtime-version", f"{linked_sdk}.0"])
            command([*signing, app], timeout=30)
            signature = command(["/usr/bin/codesign", "--display", "--verbose=4", app]).stderr.decode()
            code_directory = next(line for line in signature.splitlines() if line.startswith("CodeDirectory "))
            assert ("runtime" in code_directory) == hardened, "fixture signature did not match requested hardening"
            fixture = root / "fixture.json"
            fixture.write_text(json.dumps({
                "root": str(root), "app": str(app), "assets": str(assets),
                "identifier": identifier, "incognito": incognito, "devtools": devtools,
                "synthetic_media": not capabilities_only,
                "app_ui": app_ui,
            }))
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
            result["signature"] = {"hardened_runtime": hardened, "production_entitlements": hardened}
            result["webview_options"] = {"devtools": devtools}
            result["linked_build_version"] = build_version.strip()
            print(json.dumps(result, indent=2), flush=True)
            browser = result["browser"]
            if app_ui and browser["status"] != "passed" and (root / "holdmap-runtime.png").is_file():
                with tempfile.NamedTemporaryFile(prefix="holdmap-owned-app-png-diagnostic-", suffix=".png", delete=False) as diagnostic:
                    diagnostic.write((root / "holdmap-runtime.png").read_bytes())
                    print(f"Owned App PNG diagnostic (private): {diagnostic.name}", flush=True)
            assert browser["status"] == "passed", browser.get("error")
            assert result["native_start_requests"] == 0, "fixture attempted native screen capture"
            assert result["ui_focus_requests"] == (3 if app_ui else 0), "only the App UI mode should activate its owned window"
            assert not browser["screenPickerInvoked"] and not browser["violations"]
            assert browser["secureContext"] is True, "fixture did not load a secure app context"
            assert browser["url"].rstrip("/") == "tauri://localhost", "fixture did not load the trusted app origin"
            assert result["native_context"] == {
                "is_dev": not custom_protocol, "incognito": incognito,
                "debug_assertions": not release_profile,
                "background_throttling": None if capabilities_only else "disabled",
            }
            assert not (root / "unused-store").exists(), "fixture unexpectedly accessed a user store"
            assert browser["support"]["display"] == (browser["originalAPIs"]["getDisplayMedia"] == "function"), "capture helper disagrees with the original browser API"
            if not app_ui and int(platform.mac_ver()[0].split(".")[0]) >= 14:
                assert browser["nativeCapabilities"]["available"] is True, "native capture fallback is missing on supported macOS"
            if app_ui:
                assert browser["tauri"] is True, "production App did not run in a native webview"
                assert len(browser["foregroundChecks"]) == 3
                assert all(check["visibility"] == "visible" and check["focused"] for check in browser["foregroundChecks"]), "App UI interactions did not run in a foreground window"
                assert not browser["errors"], "production App emitted runtime errors"
                assert browser["closedWithoutCapture"] is True
                assert browser["fallbackClosedWithoutCapture"] is True
                assert browser["forcedAPIs"]["getDisplayMedia"] == "undefined"
                if int(platform.mac_ver()[0].split(".")[0]) >= 14:
                    assert browser["nativeFallbackSkipped"] is False
                if browser["support"]["display"] or browser["originalSupportedViaNative"]:
                    assert browser["ui"]["chooseScreenVisible"] is True
                    assert browser["ui"]["chooseScreenDisabled"] is False
                    assert browser["ui"]["unavailableMessage"] is False
                if not browser["nativeFallbackSkipped"]:
                    assert browser["fallbackUI"]["chooseScreenVisible"] is True
                    assert browser["fallbackUI"]["chooseScreenDisabled"] is False
                    assert browser["fallbackUI"]["unavailableMessage"] is False
                else:
                    print("SKIP native fallback on macOS older than 14; unsupported explanation verified", flush=True)
                snapshot = browser["appSnapshot"]
                assert snapshot["closedAfterSave"] and snapshot["missingDisplayAPI"]
                file = root / "holdmap-runtime.png"
                payload = file.read_bytes()
                observed = {"bytes": len(payload), "sha256": hashlib.sha256(payload).hexdigest()}
                assert set(result["native_files"]) == {file.name} and set(browser["files"]) == {"png"}
                assert observed == result["native_files"][file.name]
                assert observed == {key: browser["files"]["png"][key] for key in observed}
                assert 0 < len(payload) <= 32 * 1024 * 1024 and payload.startswith(b"\x89PNG\r\n\x1a\n")
                assert file.stat().st_mode & 0o777 == 0o600
                width, height = snapshot["width"], snapshot["height"]
                assert 0 < width <= 3840 and 0 < height <= 2160
                probe = json.loads(command([
                    "ffprobe", "-v", "error", "-show_entries", "stream=codec_name,width,height", "-of", "json", file,
                ], timeout=20).stdout)
                assert probe["streams"] == [{"codec_name": "png", "width": width, "height": height}], probe
                rgba = command(["ffmpeg", "-v", "error", "-i", file, "-f", "rawvideo", "-pix_fmt", "rgba", "-"], timeout=20).stdout
                assert len(rgba) == width * height * 4
                assert rgba[3::4].count(255) / (width * height) > .95
                # Decode the actual saved bytes independently; connected-image canvas
                # readback in an unfocused WKWebView can transiently return all zeros.
                brightness = lambda: map(sum, zip(rgba[0::4], rgba[1::4], rgba[2::4]))
                assert max(brightness()) - min(brightness()) > 75, "App PNG is blank"
                assert {region["name"] for region in snapshot["regions"]} == {"bundled icon", "bundled font"}
                checked = []
                for region in snapshot["regions"]:
                    scale_x, scale_y = width / snapshot["cssBounds"]["width"], height / snapshot["cssBounds"]["height"]
                    left, top = int(region["x"] * scale_x), int(region["y"] * scale_y)
                    region_width = max(1, int(region["width"] * scale_x))
                    region_height = max(1, int(region["height"] * scale_y))
                    assert 0 <= left < left + region_width <= width and 0 <= top < top + region_height <= height
                    pixels = [tuple(rgba[offset:offset + 4])
                              for y in range(top, top + region_height)
                              for x in range(left, left + region_width)
                              for offset in [(y * width + x) * 4]]
                    contrast = (max(sum(pixel[:3]) for pixel in pixels) - min(sum(pixel[:3]) for pixel in pixels)) / 3
                    opaque = sum(pixel[3] == 255 for pixel in pixels) / len(pixels)
                    colorful = sum(max(pixel[:3]) - min(pixel[:3]) > 40 for pixel in pixels) / len(pixels)
                    assert contrast > 25 and opaque > .95, (region["name"], contrast, opaque)
                    if region["name"] == "bundled icon":
                        assert colorful > .1, "App PNG lost the bundled icon colors"
                    checked.append({"name": region["name"], "contrast": contrast, "opaque": opaque, "colorful": colorful})
                print("PASS independently decoded App PNG regions: " + json.dumps(checked), flush=True)
                print("PASS production App PNG UI in WKWebView: bundled font/icon pixels, bounded decode, exact binary IPC save; no source picker", flush=True)
                return result
            if capabilities_only:
                assert not result["native_files"] and not browser["files"]
                print("PASS capability-only probe; no display/device request, canvas capture or file export", flush=True)
                return result
            for check in ("tracksEnded", "hiddenVideosRemoved", "discardTracksEnded", "discardVideosRemoved", "snapshotKeptVideoCapturing"):
                assert browser[check], check
            for format in ("mp4", "gif", "png"):
                file = root / f"holdmap-runtime.{format}"
                payload = file.read_bytes()
                observed = {"bytes": len(payload), "sha256": hashlib.sha256(payload).hexdigest()}
                assert observed == result["native_files"][file.name]
                assert observed == {key: browser["files"][format][key] for key in observed}
                assert file.stat().st_mode & 0o777 == 0o600
                if format == "mp4":
                    assert payload[4:8] == b"ftyp"
                elif format == "gif":
                    assert payload[:6] in (b"GIF87a", b"GIF89a")
                else:
                    assert payload.startswith(b"\x89PNG\r\n\x1a\n")
                probe = json.loads(command([
                    "ffprobe", "-v", "error", "-count_frames", "-show_entries",
                    "stream=codec_name,codec_type,width,height,nb_read_frames:format=duration", "-of", "json", file,
                ], timeout=20).stdout)
                assert len(probe["streams"]) == 1, probe
                video = probe["streams"][0]
                assert video["codec_type"] == "video"
                assert video["codec_name"] == {"mp4": "h264", "gif": "gif", "png": "png"}[format]
                assert (video["width"], video["height"]) == (320, 180)
                if format == "png":
                    assert int(video["nb_read_frames"]) == 1
                    command(["ffmpeg", "-v", "error", "-i", file, "-f", "null", "-"], timeout=20)
                    print("PASS PNG: exact binary IPC digest, dimensions, decode and private save; video continued", flush=True)
                    continue
                assert int(video["nb_read_frames"]) >= 3
                assert 1 <= float(probe["format"]["duration"]) <= 8, probe
                decoded = command(["ffmpeg", "-v", "error", "-i", file, "-f", "framemd5", "-"], timeout=20).stdout.decode()
                hashes = {line.rsplit(",", 1)[-1].strip() for line in decoded.splitlines() if line and not line.startswith("#")}
                assert len(hashes) >= 2, f"{format}: encoded canvas never changed"
                print(f"PASS {format.upper()}: {json.dumps(probe, separators=(',', ':'))}; exact binary IPC digest; animated frames", flush=True)
            print("PASS production encoder and save adapter in WKWebView with strict CSP; stop/discard release tracks; owned fixture cleaned up")
    finally:
        if identifier:
            # Persistent comparison views must leave none of their uniquely named
            # WebKit stores behind. Never reuse or remove the real app's identifier.
            for directory in ("WebKit", "Caches", "HTTPStorages"):
                path = Path.home() / "Library" / directory / identifier
                if path.is_symlink():
                    path.unlink()
                elif path.exists():
                    shutil.rmtree(path)
        os.umask(previous_umask)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--compare-capabilities", action="store_true", help="Probe dev/production protocol and private/persistent data stores without capturing anything")
    modes.add_argument("--app-ui", action="store_true", help="Verify production recorder gates and save only the owned App's PNG")
    args = parser.parse_args()
    if args.compare_capabilities:
        reports = [run(custom_protocol=protocol, incognito=private, capabilities_only=True,
                       release_profile=release, hardened=hardened)
                   for protocol, release, hardened in (
                       (False, False, False), (True, False, False), (True, True, False),
                       (True, False, True), (True, True, True),
                   ) for private in (True, False)]
        reports.append(run(capabilities_only=True, devtools=False))
        print(json.dumps({"capability_comparison": reports}, indent=2), flush=True)
    else:
        # An unfocused WKWebView on a busy runner can hand back a blank canvas for the very first
        # capture (see the comment on the brightness check). That one symptom, and only that one, is
        # worth a single fresh attempt; any other failure, or a second blank, fails the check.
        for attempt in (1, 2):
            try:
                run(app_ui=args.app_ui, release_profile=args.app_ui)
                break
            except AssertionError as error:
                blank = re.search(r"blank|bundled (icon|font)", str(error))
                if not (args.app_ui and blank) or attempt == 2:
                    raise
                print(f"RETRY once: the App PNG came back blank ({error}); capturing again with a fresh window", flush=True)
