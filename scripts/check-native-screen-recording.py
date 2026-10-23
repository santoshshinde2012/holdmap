#!/usr/bin/env python3
"""Validate native MP4/GIF encoding with owned pixels; never open a screen picker."""

from __future__ import annotations

import json
from pathlib import Path
import platform
import shutil
import stat
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
NATIVE = ROOT / "apps/desktop/src-tauri/native"
MAX_BYTES = 128 * 1024 * 1024


def run(args: list[str], *, timeout: int = 120) -> subprocess.CompletedProcess[bytes]:
    result = subprocess.run(args, capture_output=True, timeout=timeout, check=False)
    if result.returncode:
        raise RuntimeError(
            f"Native recording fixture command failed ({result.returncode}): "
            + result.stderr.decode(errors="replace")
            + result.stdout.decode(errors="replace")
        )
    return result


def probe(path: Path) -> dict:
    return json.loads(run([
        "ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)
    ]).stdout)


def validate(path: Path, duration: float, frame_count: int | None = None) -> None:
    metadata = probe(path)
    streams = metadata["streams"]
    assert len(streams) == 1 and streams[0]["codec_type"] == "video", "Unexpected audio or extra stream"
    video = streams[0]
    assert video["codec_name"] == ("gif" if path.suffix == ".gif" else "h264"), "Unexpected codec"
    assert 0 < video["width"] <= 1920 and 0 < video["height"] <= 1080, "Native dimensions exceeded limit"
    assert abs(float(metadata["format"]["duration"]) - duration) < 0.02, "Native recording lost its timeline"
    assert 0 < path.stat().st_size <= MAX_BYTES, "Native recording exceeded byte limit"
    assert stat.S_IMODE(path.stat().st_mode) == 0o600, "Native recording is not private"
    run(["ffmpeg", "-v", "error", "-i", str(path), "-f", "null", "-"], timeout=120)
    if frame_count is not None:
        assert int(video["nb_frames"]) == frame_count, "GIF frame bound/sampling changed"
        selected = [0, frame_count // 2, frame_count - 1]
        expression = "+".join(f"eq(n\\,{index})" for index in selected)
        pixels = run([
            "ffmpeg", "-v", "error", "-i", str(path), "-vf", f"select={expression},scale=1:1",
            "-fps_mode", "passthrough", "-frames:v", "3", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"
        ]).stdout
        assert len(pixels) == 9, "Could not decode GIF first/middle/last frames"
        for offset, channel in enumerate([0, 2, 1]):  # owned red, blue, green frames
            color = pixels[offset * 3:offset * 3 + 3]
            assert all(color[channel] > value + 80 for index, value in enumerate(color) if index != channel), \
                "GIF lost its first, middle, or final scene"


def validate_png(path: Path) -> None:
    metadata = probe(path)
    assert len(metadata["streams"]) == 1, "PNG snapshot has unexpected streams"
    image = metadata["streams"][0]
    assert image["codec_name"] == "png", "Snapshot is not a PNG"
    assert (image["width"], image["height"]) == (640, 360), "Snapshot dimensions changed"
    assert 0 < path.stat().st_size <= 32 * 1024 * 1024, "Snapshot exceeded byte limit"
    assert stat.S_IMODE(path.stat().st_mode) == 0o600, "Snapshot is not private"
    pixel = run(["ffmpeg", "-v", "error", "-i", str(path), "-vf", "scale=1:1", "-frames:v", "1",
                 "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"]).stdout
    assert len(pixel) == 3 and pixel[0] > pixel[1] + 150 and pixel[0] > pixel[2] + 150, \
        "PNG snapshot lost its owned red pixels"


def main() -> None:
    if sys.platform != "darwin" or int(platform.mac_ver()[0].split(".")[0]) < 14:
        print("SKIP native recording fixture requires macOS 14 or newer")
        return
    for executable in ("xcrun", "otool", "ffprobe", "ffmpeg"):
        if shutil.which(executable) is None:
            raise RuntimeError(f"Native recording fixture requires {executable}")
    architecture = platform.machine()
    if architecture not in ("arm64", "x86_64"):
        raise RuntimeError("Unsupported native fixture architecture")
    with tempfile.TemporaryDirectory(prefix="holdmap-native-recording-") as temporary:
        private = Path(temporary)
        source = private / "fixture.swift"
        # Only the fixture is appended; the real writer, converter, C boundary and
        # lifecycle state implementation are compiled without substitution.
        source.write_text((NATIVE / "screen_capture.swift").read_text() + "\n"
                          + (NATIVE / "screen_capture_fixture.swift").read_text())
        binary = private / "fixture"
        output = private / "outputs"
        output.mkdir(mode=0o700)
        run([
            "xcrun", "swiftc", "-swift-version", "5", "-parse-as-library", "-O",
            "-target", f"{architecture}-apple-macos11.0", str(source),
            "-Xlinker", "-weak_framework", "-Xlinker", "ScreenCaptureKit", "-o", str(binary)
        ])
        dependencies = run(["otool", "-L", str(binary)]).stdout.decode()
        framework = [line for line in dependencies.splitlines() if "ScreenCaptureKit.framework/" in line]
        assert len(framework) == 1 and "weak" in framework[0], "ScreenCaptureKit must be weak linked"
        assert "libswift_Concurrency" not in dependencies, "Fixture requires a newer Swift concurrency runtime"
        result = run([str(binary), str(output)], timeout=240)
        assert json.loads(result.stdout)["capture_invoked"] is False
        validate(output / "short.mp4", 3)
        validate(output / "short.gif", 3, 24)
        validate(output / "rounded.mp4", 20)
        validate(output / "rounded.gif", 20, 160)
        validate(output / "capped.mp4", 120)
        # 120 s at 8 fps wants 960 frames; the 256 MiB ImageIO memory budget at the default
        # 640 px edge keeps 252 of them (it was a fixed 240 before recordings had options).
        validate(output / "capped.gif", 120, 252)
        validate_png(output / "snapshot.png")
    print("Native recording fixture passed: real H.264/GIF and PNG, full timeline, bounds, privacy and cancellation; no screen capture")


if __name__ == "__main__":
    main()
