#!/usr/bin/env python3
"""Verify the built tray in an owned, ephemeral native macOS WKWebView.

Uses synthetic IPC, no real app state, scans or permissions. Captures only its own webview.
Run npm run build in apps/desktop first. Optional --output keeps the report and screenshot.
"""
import argparse
import json
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import time
import urllib.request

REPO = Path(__file__).resolve().parents[1]
DESKTOP = REPO / 'apps/desktop'


def run(output):
    if platform.system() != 'Darwin':
        raise SystemExit('Requires macOS WKWebView')
    with tempfile.TemporaryDirectory(prefix='holdmap-tray-runtime-') as directory:
        root = Path(directory)
        result = root / 'report.json'
        data = subprocess.check_output(['node', '--experimental-strip-types', '--input-type=module', '-e',
            "import {demoPanel} from './apps/desktop/src/lib/tray-panel.ts'; console.log(JSON.stringify(demoPanel()))"], cwd=REPO)
        (root / 'data.json').write_bytes(data)
        executable = root / 'tray-fixture'
        subprocess.run(['xcrun', 'swiftc', str(REPO / 'scripts/tray-panel-runtime.swift'), '-o', str(executable)], check=True, timeout=90)
        server = subprocess.Popen(['node', 'node_modules/vite/bin/vite.js', 'preview', '--host', '127.0.0.1', '--port', '4176', '--strictPort'], cwd=DESKTOP, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            for _ in range(100):
                if server.poll() is not None:
                    raise RuntimeError(server.stderr.read().decode())
                try:
                    urllib.request.urlopen('http://127.0.0.1:4176/tray.html', timeout=1).close()
                    break
                except OSError:
                    time.sleep(.05)
            subprocess.run([str(executable), 'http://127.0.0.1:4176/tray.html', str(root / 'data.json'), str(REPO / 'scripts/tray-panel-fixture.mjs'), str(result)], check=True, timeout=55)
            report = json.loads(result.read_text())
            if output:
                output.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(result, output)
                if result.with_suffix('.json.png').exists():
                    shutil.copy2(result.with_suffix('.json.png'), output.with_suffix('.png'))
            assert report.get('status') == 'passed', report
            print(f"PASS native WKWebView: {len(report['reports'])} layout checks; section navigation, polling, settings and keyboard keep header/footer fixed")
        finally:
            server.terminate()
            server.wait(timeout=5)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path)
    run(parser.parse_args().output)
