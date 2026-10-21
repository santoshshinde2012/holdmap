#!/usr/bin/env python3
"""Detection evaluation for holdmap's analyzers and presets.

Builds a small labelled set of agent skills in a throwaway HOME (nothing is read from or
written to your real home), registers and approves each analyzer there, runs it through
holdmap (staging copy, sandbox, report parsing) and scores the findings per skill.

The skills live in eval-fixtures.json. Credential-shaped values there are placeholders filled
(`{{fill:LENGTH:CHARSET}}`, `{{pem:LINES}}`) with seeded filler characters when the files are written, so none is committed.

    python3 scripts/eval-analyzers.py --holdmap target/release/holdmap [--min medium] [--json out.json]

Scoring, per analyzer and per skill: a skill counts as flagged when the analyzer reports at
least one finding at or above --min. Positives are the skills carrying a label the analyzer
claims to cover (SCOPE); negatives are the clean skills. Skills whose labels are all outside
an analyzer's scope are not scored for it, so a secret scanner isn't marked down for missing
shell=True.
"""
from __future__ import annotations

import argparse
import json
import os
import random
import re
import shutil
import string
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SEV = ["info", "low", "medium", "high", "critical"]

# Labels: secret, pyrisk (risky Python), vulndep (known-vulnerable pinned dependency),
# inject (prompt injection or exfiltration instructions in the skill text).
SCOPE = {
    "aquasecurity.trivy-secrets": {"secret"},
    "pycqa.bandit": {"pyrisk"},
    "google.osv-scanner": {"vulndep"},
    "cisco.skill-scanner": {"inject"},
    "nvidia.skillspector": {"secret", "pyrisk", "vulndep", "inject"},
}
PRESETS = {"aquasecurity.trivy-secrets", "pycqa.bandit", "google.osv-scanner", "cisco.skill-scanner"}
PROGRAM = {
    "aquasecurity.trivy-secrets": "trivy",
    "pycqa.bandit": "bandit",
    "google.osv-scanner": "osv-scanner",
    "cisco.skill-scanner": "skill-scanner",
    "nvidia.skillspector": "skillspector",
}


FIXTURES = Path(__file__).resolve().parent / "eval-fixtures.json"
PEM = re.compile(r"\{\{pem:(\d+)\}\}")
FILL = re.compile(r"\{\{fill:(\d+):(\w+)\}\}")
CHARSETS = {
    "alnum": string.ascii_letters + string.digits,
    "base32": string.ascii_uppercase + "234567",
    "base64": string.ascii_letters + string.digits + "+/",
    "digits": string.digits,
}


def fixtures() -> dict[str, tuple[set[str], dict[str, str]]]:
    """The labelled skills from eval-fixtures.json. Credential-shaped fixtures are stored as
    a fixed prefix plus `{{fill:LENGTH:CHARSET}}`, which is replaced with filler characters
    from a fixed seed, so the repository holds no credential-shaped strings and every run
    writes the same files."""
    filler = random.Random(20261021)
    spec = json.loads(FIXTURES.read_text(encoding="utf-8"))
    fill = lambda n, charset: "".join(filler.choice(CHARSETS[charset]) for _ in range(n))

    def expand(text: str) -> str:
        # `{{pem:LINES}}` is a PEM block of filler lines; its armour is split so secret
        # scanners don't flag this script.
        pem = PEM.fullmatch(text)
        if pem:
            label = "RSA " + "PRIVATE KEY"
            lines = "\n".join(fill(64, "base64") for _ in range(int(pem.group(1))))
            return f"-----BEGIN {label}-----\n{lines}\n-----END {label}-----\n"
        return FILL.sub(lambda m: fill(int(m.group(1)), m.group(2)), text)

    return {name: (set(item["labels"]), {rel: expand(text) for rel, text in item["files"].items()})
            for name, item in spec.items()}


def run(cmd: list[str], env: dict, check: bool = True, timeout: int = 900) -> subprocess.CompletedProcess:
    p = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=timeout)
    if check and p.returncode != 0:
        sys.exit(f"command failed ({p.returncode}): {' '.join(cmd)}\n{p.stdout}\n{p.stderr}")
    return p


def evasion_control(skills: Path, ids: list[str]) -> list[dict]:
    """Run Bandit and Trivy straight on the evade-* skills, in their own folder, the way they
    would run without holdmap's staging copy, and count what they report."""
    checks = []
    if "pycqa.bandit" in ids:
        checks.append(("evade-bandit-config", "bandit", ["bandit", "--recursive", ".", "--format", "json", "--quiet"],
                       lambda d: len(d.get("results", []))))
    if "aquasecurity.trivy-secrets" in ids:
        checks.append(("evade-trivyignore", "trivy", ["trivy", "fs", "--scanners", "secret", "--skip-version-check",
                                                     "--disable-telemetry", "--quiet", "--format", "json", "."],
                       lambda d: sum(len(r.get("Secrets") or []) for r in d.get("Results") or [])))
    out = []
    for skill, tool, cmd, count in checks:
        p = subprocess.run(cmd, cwd=skills / skill, capture_output=True, text=True, timeout=300)
        try:
            direct = count(json.loads(p.stdout))
        except json.JSONDecodeError:
            direct = None
        out.append({"skill": skill, "tool": tool, "direct": direct, "holdmap": None})
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--holdmap", default="target/release/holdmap")
    ap.add_argument("--min", default="medium", choices=SEV)
    ap.add_argument("--only", nargs="*", help="analyzer ids to evaluate (default: every one installed)")
    ap.add_argument("--json", help="write the full results here")
    ap.add_argument("--keep", action="store_true", help="keep the throwaway HOME")
    a = ap.parse_args()
    holdmap = str(Path(a.holdmap).resolve())

    home = Path(tempfile.mkdtemp(prefix="holdmap-eval-"))
    labels: dict[str, set[str]] = {}
    for name, (lab, files) in fixtures().items():
        labels[name] = lab
        for rel, text in files.items():
            p = home / ".claude" / "skills" / name / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text, encoding="utf-8")
    env = {k: v for k, v in os.environ.items() if not k.startswith("HOLDMAP_")}
    env.update(HOME=str(home), HOLDMAP_HOME=str(home / ".holdmap"), NO_COLOR="1",
               XDG_CONFIG_HOME=str(home / ".config"), XDG_DATA_HOME=str(home / ".local/share"),
               XDG_CACHE_HOME=str(home / ".cache"), XDG_STATE_HOME=str(home / ".local/state"))
    # The scanners stay on the real PATH; everything holdmap stores goes to the throwaway HOME.
    ids = a.only or [i for i in SCOPE if shutil.which(PROGRAM[i])]
    results = {}
    min_rank = SEV.index(a.min)
    for aid in ids:
        if aid in PRESETS:
            run([holdmap, "analyzers", "add", "--preset", aid], env)
        run([holdmap, "analyzers", "approve", aid, "--yes"], env)
        t0 = time.perf_counter()
        r = run([holdmap, "analyzers", "run", aid, "--json"], env, check=False)
        secs = time.perf_counter() - t0
        ins = json.loads(run([holdmap, "insights", "--json", "--all"], env).stdout)
        res = next((x for x in ins["results"] if x["analyzer"] == aid), None)
        if res is None:
            sys.exit(f"{aid}: no result recorded\n{r.stdout}\n{r.stderr}")
        names = {s["id"]: s["name"] for s in res["subjects"]}
        flagged: dict[str, list[str]] = {}
        every: dict[str, list[str]] = {}
        for f in res["findings"]:
            every.setdefault(names.get(f["subject"], "?"), []).append(f["rule_id"])
            if SEV.index(f["severity"]) >= min_rank:
                flagged.setdefault(names.get(f["subject"], "?"), []).append(f"{f['severity']}:{f['rule_id']}")
        tp = fp = fn = tn = 0
        per_skill = {}
        for name, lab in labels.items():
            hit = name in flagged
            if lab & SCOPE[aid]:
                tp, fn = tp + hit, fn + (not hit)
                per_skill[name] = "TP" if hit else "FN"
            elif not lab:
                fp, tn = fp + hit, tn + (not hit)
                per_skill[name] = "FP" if hit else "TN"
            else:
                per_skill[name] = "-"
        results[aid] = {
            "tool": PROGRAM[aid], "version": res.get("analyzer_version") or "unknown",
            "status": res["status"], "seconds": round(secs, 2), "subjects": len(res["subjects"]),
            "findings": len(res["findings"]), "tp": tp, "fp": fp, "fn": fn, "tn": tn,
            "precision": round(tp / (tp + fp), 2) if tp + fp else None,
            "recall": round(tp / (tp + fn), 2) if tp + fn else None,
            "per_skill": per_skill, "flagged": flagged, "all_findings": every,
        }
    out = {"min_severity": a.min, "skills": {k: sorted(v) for k, v in labels.items()}, "analyzers": results}
    if a.json:
        Path(a.json).write_text(json.dumps(out, indent=2) + "\n")
    print(f"min severity: {a.min}; {len(labels)} skills, {sum(1 for v in labels.values() if not v)} clean\n")
    print("| Analyzer | Version | Covers | TP | FP | FN | TN | Precision | Recall | Run (s) |")
    print("| --- | --- | --- | --: | --: | --: | --: | --: | --: | --: |")
    fmt = lambda x: "n/a" if x is None else f"{x:.2f}"
    for aid, r in results.items():
        print(f"| `{aid}` | {r['version']} | {', '.join(sorted(SCOPE[aid]))} | {r['tp']} | {r['fp']} | {r['fn']} | {r['tn']} | {fmt(r['precision'])} | {fmt(r['recall'])} | {r['seconds']} |")
    print("\nper skill (TP/FP/FN/TN; - = outside the analyzer's scope):")
    print(f"  {'skill':22} {'labels':14} " + " ".join(f"{PROGRAM[a][:6]:>6}" for a in results))
    for name in labels:
        print(f"  {name:22} {','.join(sorted(labels[name])) or 'clean':14} " + " ".join(f"{results[a]['per_skill'][name]:>6}" for a in results))
    control = evasion_control(home / ".claude" / "skills", ids)
    for c in control:
        aid = next(i for i in results if PROGRAM[i] == c["tool"])
        c["holdmap"] = len(results[aid]["all_findings"].get(c["skill"], []))
    if control:
        out["evasion_control"] = control
        if a.json:
            Path(a.json).write_text(json.dumps(out, indent=2) + "\n")
        print("\nthe same scanner run directly on the skill folder, which keeps the settings it ships:")
        for c in control:
            print(f"  {c['skill']:22} {c['tool']:7} findings run directly: {c['direct']}; through holdmap: {c['holdmap']}")
    if a.keep:
        print(f"\nHOME kept at {home}")
    else:
        shutil.rmtree(home, ignore_errors=True)


if __name__ == "__main__":
    main()
