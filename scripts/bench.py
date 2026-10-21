#!/usr/bin/env python3
"""Reproducible benchmarks for holdmap.

Measures, with the binary you point it at:
  * CLI startup: `holdmap --version`;
  * scan latency and peak memory of `holdmap list --json` with N extra listening ports;
  * peak memory of `holdmap agents --json`;
  * analyzer run overhead: `holdmap analyzers run` against running the same scanner on the
    same skill folders directly (needs bandit and/or trivy on PATH);
  * download sizes of the latest release (CLI archives and desktop installers), from the
    public GitHub API, and the size of a local desktop bundle folder if you pass one.

Everything holdmap stores goes to a throwaway HOME. Results are printed as Markdown tables
and, with --json, written with the exact environment. Unix only: peak memory comes from
wait4, through scripts/peak-rss.c (built with cc). The numbers depend on the machine, so
compare runs on the same one.

    cargo build --release --locked -p holdmap
    python3 scripts/bench.py --holdmap target/release/holdmap --json bench-results.json
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import socket
import statistics
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

def _load_fixtures():
    import importlib.util

    spec = importlib.util.spec_from_file_location("eval_analyzers", Path(__file__).resolve().parent / "eval-analyzers.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.fixtures


fixtures = _load_fixtures()

REPO = "santoshshinde2012/holdmap"


PEAK_RSS: str | None = None  # path of the compiled scripts/peak-rss.c


def build_peak_rss(dir: Path) -> str | None:
    cc = shutil.which("cc")
    if not cc:
        return None
    out = dir / "peak-rss"
    src = Path(__file__).resolve().parent / "peak-rss.c"
    ok = subprocess.run([cc, "-O2", "-o", str(out), str(src)], capture_output=True).returncode == 0
    return str(out) if ok else None


def timed(cmd: list[str], env: dict) -> float:
    """Wall time of one run in ms."""
    t0 = time.perf_counter()
    p = subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    ms = (time.perf_counter() - t0) * 1000
    if p.returncode != 0:
        sys.exit(f"failed ({p.returncode}): {' '.join(cmd)}")
    return ms


def peak_rss_mib(cmd: list[str], env: dict, runs: int = 3) -> float | None:
    """Largest peak RSS of any single process in the run (holdmap, or a scanner it starts),
    read by the small C helper so the Python parent's own size can't leak into it."""
    if not PEAK_RSS:
        return None
    peaks = []
    for _ in range(runs):
        p = subprocess.run([PEAK_RSS, *cmd], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        line = [l for l in p.stderr.splitlines() if l.startswith("peak_rss_kib=")]
        if p.returncode != 0 or not line:
            sys.exit(f"failed ({p.returncode}): {' '.join(cmd)}")
        peaks.append(int(line[-1].split("=")[1]))
    return round(max(peaks) / 1024, 1)


def series(cmd: list[str], env: dict, runs: int, warmup: int = 2) -> dict:
    for _ in range(warmup):
        timed(cmd, env)
    t = [timed(cmd, env) for _ in range(runs)]
    q = statistics.quantiles(t, n=20) if len(t) >= 2 else [t[0]] * 19
    return {"runs": runs, "median_ms": round(statistics.median(t), 1), "p95_ms": round(q[18], 1),
            "min_ms": round(min(t), 1), "peak_rss_mib": peak_rss_mib(cmd, env)}


def listeners(n: int) -> list[socket.socket]:
    import resource

    soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
    want = n + 256
    if soft < want:
        cap = want if hard == resource.RLIM_INFINITY else min(want, hard)
        resource.setrlimit(resource.RLIMIT_NOFILE, (cap, hard))
        if cap < want:
            sys.exit(f"--ports {n} needs {want} open files; the hard limit is {hard}")
    socks = []
    for _ in range(n):
        s = socket.socket()
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.bind(("127.0.0.1", 0))
        s.listen(1)
        socks.append(s)
    return socks


def listing(holdmap: str, env: dict, runs: int = 5) -> tuple[int, float]:
    """Listeners holdmap reports, and the median of its own `scan_ms` (the scan alone)."""
    seen, scans = 0, []
    for _ in range(runs):
        out = subprocess.run([holdmap, "list", "--json", "--no-docker"], env=env, capture_output=True, text=True, check=True)
        d = json.loads(out.stdout)
        seen = len(d["entries"])
        scans.append(d.get("scan_ms", 0))
    return seen, statistics.median(scans)


def version_of(cmd: list[str]) -> str:
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
        return ((p.stdout or p.stderr).strip().splitlines() or ["?"])[0][:100]
    except Exception:
        return "not available"


def environment(holdmap: str) -> dict:
    env = {"holdmap": version_of([holdmap, "--version"]), "os": platform.platform(),
           "kernel": platform.release(), "machine": platform.machine(), "python": platform.python_version(),
           "cpu_count": os.cpu_count()}
    try:
        cpu = next(l for l in Path("/proc/cpuinfo").read_text().splitlines() if l.startswith("model name"))
        env["cpu"] = cpu.split(":", 1)[1].strip()
        mem = next(l for l in Path("/proc/meminfo").read_text().splitlines() if l.startswith("MemTotal"))
        env["memory_gib"] = round(int(mem.split()[1]) / 1024 / 1024, 1)
    except (OSError, StopIteration):
        env["cpu"] = version_of(["sysctl", "-n", "machdep.cpu.brand_string"])
        try:
            env["memory_gib"] = round(int(version_of(["sysctl", "-n", "hw.memsize"])) / 1024 ** 3, 1)
        except ValueError:
            pass
    env["rustc"] = version_of(["rustc", "--version"])
    try:
        env["git"] = subprocess.run(["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    except OSError:
        pass
    return env


def analyzer_overhead(holdmap: str, env: dict, home: Path, runs: int) -> list[dict]:
    root = home / ".claude" / "skills"
    for name, (_, files) in fixtures().items():
        for rel, text in files.items():
            p = root / name / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text, encoding="utf-8")
    skills = sorted(p for p in root.iterdir() if p.is_dir())
    out = []
    tmp = Path(tempfile.mkdtemp(prefix="holdmap-bench-direct-"))
    direct = {
        "pycqa.bandit": ("bandit", lambda d, o: ["bandit", "--recursive", str(d), "--quiet", "--format", "sarif", "--output", str(o)]),
        "aquasecurity.trivy-secrets": ("trivy", lambda d, o: ["trivy", "fs", "--scanners", "secret", "--skip-version-check",
                                                              "--disable-telemetry", "--quiet", "--format", "sarif", "--output", str(o), str(d)]),
    }
    for aid, (prog, argv) in direct.items():
        if not shutil.which(prog):
            continue
        subprocess.run([holdmap, "analyzers", "add", "--preset", aid], env=env, check=True, capture_output=True)
        subprocess.run([holdmap, "analyzers", "approve", aid, "--yes"], env=env, check=True, capture_output=True)
        via = series([holdmap, "analyzers", "run", aid, "--json"], env, runs, warmup=1)
        times = []
        for _ in range(runs + 1):
            t0 = time.perf_counter()
            for d in skills:
                # Exit codes say "found something"; only the time matters here.
                subprocess.run(argv(d, tmp / "r.sarif"), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env={**os.environ, "HOME": str(tmp)})
            times.append((time.perf_counter() - t0) * 1000)
        d_ms = statistics.median(times[1:])
        out.append({"analyzer": aid, "skills": len(skills), "holdmap_ms": via["median_ms"], "direct_ms": round(d_ms, 1),
                    "overhead_ms_per_skill": round((via["median_ms"] - d_ms) / len(skills), 1),
                    "overhead_pct": round((via["median_ms"] - d_ms) / d_ms * 100, 1), "holdmap_peak_rss_mib": via["peak_rss_mib"]})
    shutil.rmtree(tmp, ignore_errors=True)
    return out


def release_sizes() -> dict:
    try:
        with urllib.request.urlopen(f"https://api.github.com/repos/{REPO}/releases/latest", timeout=20) as r:
            d = json.load(r)
    except Exception as e:  # offline is fine
        return {"error": str(e)}
    keep = (".dmg", ".msi", "-setup.exe", ".deb", ".AppImage", ".rpm", ".tar.xz", ".zip")
    return {"tag": d["tag_name"], "assets": {a["name"]: a["size"] for a in d["assets"] if a["name"].endswith(keep)}}


def folder_size(p: Path) -> dict:
    files = [f for f in p.rglob("*") if f.is_file()] if p.is_dir() else [p]
    return {f.name: f.stat().st_size for f in files if f.suffix in {".dmg", ".msi", ".exe", ".deb", ".AppImage", ".rpm", ".app", ".zip", ".gz"} or p.is_file()}


def mib(b: int) -> str:
    return f"{b / 1024 / 1024:.1f}"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--holdmap", default="target/release/holdmap")
    ap.add_argument("--ports", default="0,100,1000", help="extra listening ports to open, comma-separated")
    ap.add_argument("--runs", type=int, default=15)
    ap.add_argument("--desktop-bundle", help="a built desktop bundle folder or file to size")
    ap.add_argument("--skip", nargs="*", default=[], choices=["startup", "scan", "agents", "analyzers", "release"])
    ap.add_argument("--json")
    a = ap.parse_args()
    holdmap = str(Path(a.holdmap).resolve())
    home = Path(tempfile.mkdtemp(prefix="holdmap-bench-"))
    env = {k: v for k, v in os.environ.items() if not k.startswith("HOLDMAP_")}
    env.update(HOME=str(home), HOLDMAP_HOME=str(home / ".holdmap"), NO_COLOR="1")
    global PEAK_RSS
    PEAK_RSS = build_peak_rss(home)
    res: dict = {"environment": environment(holdmap), "binary_bytes": Path(holdmap).stat().st_size}
    e = res["environment"]
    print(f"holdmap {e['holdmap']} · {e['os']} · {e.get('cpu')} ×{e['cpu_count']} · {e.get('memory_gib')} GiB · {e['rustc']}\n")
    print(f"Release binary: {mib(res['binary_bytes'])} MiB\n")

    if "startup" not in a.skip:
        res["startup"] = series([holdmap, "--version"], env, a.runs * 2)
        s = res["startup"]
        print(f"| CLI startup (`--version`) | median {s['median_ms']} ms | p95 {s['p95_ms']} ms | peak RSS {s['peak_rss_mib']} MiB |\n| --- | --: | --: | --: |\n")

    if "scan" not in a.skip:
        res["scan"] = []
        base, _ = listing(holdmap, env, 1)
        print("| Extra ports | Listeners reported | `list --json` median | p95 | Scan alone (`scan_ms`) | Peak RSS |\n| --: | --: | --: | --: | --: | --: |")
        for n in (int(x) for x in a.ports.split(",")):
            socks = listeners(n)
            try:
                seen, scan_ms = listing(holdmap, env)
                s = series([holdmap, "list", "--json", "--no-docker"], env, a.runs)
            finally:
                for x in socks:
                    x.close()
            s.update(extra_ports=n, listeners_reported=seen, baseline_listeners=base, scan_ms_median=scan_ms)
            res["scan"].append(s)
            print(f"| {n} | {seen} | {s['median_ms']} ms | {s['p95_ms']} ms | {scan_ms:g} ms | {s['peak_rss_mib']} MiB |")
        print()

    if "agents" not in a.skip:
        res["agents"] = series([holdmap, "agents", "--json", "--no-docker"], env, max(5, a.runs // 3))
        s = res["agents"]
        print(f"| `agents --json` | median {s['median_ms']} ms | p95 {s['p95_ms']} ms | peak RSS {s['peak_rss_mib']} MiB |\n| --- | --: | --: | --: |\n")

    if "analyzers" not in a.skip:
        res["analyzers"] = analyzer_overhead(holdmap, env, home, max(3, a.runs // 3))
        print("| Analyzer | Skills | Through holdmap | Scanner alone | Overhead per skill | Peak RSS, largest process |\n| --- | --: | --: | --: | --: | --: |")
        for r in res["analyzers"]:
            print(f"| `{r['analyzer']}` | {r['skills']} | {r['holdmap_ms']} ms | {r['direct_ms']} ms | {r['overhead_ms_per_skill']} ms ({r['overhead_pct']}%) | {r['holdmap_peak_rss_mib']} MiB |")
        print()

    if "release" not in a.skip:
        res["release"] = release_sizes()
        if "assets" in res["release"]:
            print(f"| {res['release']['tag']} download | MiB |\n| --- | --: |")
            for k, v in sorted(res["release"]["assets"].items()):
                print(f"| {k} | {mib(v)} |")
            print()
    if a.desktop_bundle:
        res["desktop_bundle"] = folder_size(Path(a.desktop_bundle))
        for k, v in res["desktop_bundle"].items():
            print(f"local bundle {k}: {mib(v)} MiB")
    if a.json:
        Path(a.json).write_text(json.dumps(res, indent=2) + "\n")
    shutil.rmtree(home, ignore_errors=True)


if __name__ == "__main__":
    main()
