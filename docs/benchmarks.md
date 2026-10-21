# Benchmarks and evaluation

Every number here was measured with the scripts in this repository, on the machine named
below. They're Linux numbers: a Mac (and Windows) differs in how holdmap reads sockets and
processes, in the sandbox analyzers run in, and in hardware, so run the scripts again on the
machine you care about before comparing. Nothing on this page was measured on macOS or Windows.

## Environment

| | |
| --- | --- |
| Machine | Cloud VM: Intel Xeon, 8 vCPUs, 15.6 GiB RAM |
| OS | Debian GNU/Linux 13 (trixie), kernel 6.12.94, x86_64 |
| holdmap | `main` at `9713a58` plus this change (source version 0.9.0), `cargo build --release --locked -p holdmap` |
| Toolchain | rustc 1.99.1, Python 3.13.5 |
| Analyzer sandbox | bubblewrap 0.12.0 |
| Scanners | Bandit 1.9.4, Trivy 0.76.0, OSV-Scanner 2.7.0, Cisco Skill Scanner 2.3.0, SkillSpector 2.13.0 (`--no-llm`) |
| Measured | 21 October 2026 |

The VM runs other services, so it already has 27 listening ports before the benchmark adds any.

## Performance

`python3 scripts/bench.py --runs 30 --ports 0,100,1000,5000`. Times are wall clock for the whole
command, process start to exit, after two warm-up runs. Peak RSS is the largest single process
in the run.

| Measurement | Median | p95 | Peak RSS |
| --- | --: | --: | --: |
| CLI startup (`holdmap --version`, 60 runs) | 1.2 ms | 1.7 ms | 4.7 MiB |
| `holdmap agents --json` (10 runs) | 55.9 ms | 64.7 ms | 8.0 MiB |

Scan latency with N extra listening ports (all held by one process):

| Extra ports | Listeners reported | `holdmap list --json` median | p95 | Scan alone (`scan_ms`) | Peak RSS |
| --: | --: | --: | --: | --: | --: |
| 0 | 27 | 60.1 ms | 91.6 ms | 79 ms | 7.6 MiB |
| 100 | 127 | 47.1 ms | 61.7 ms | 36 ms | 8.1 MiB |
| 1,000 | 1,027 | 45.0 ms | 58.0 ms | 23 ms | 13.3 MiB |
| 5,000 | 5,027 | 188.5 ms | 262.4 ms | 86 ms | 36.5 MiB |

`scan_ms` is the scan's own time as reported in the JSON (median of 5 runs); the rest is
startup, enrichment and JSON output. Up to 1,000 ports the time is set by the processes
behind the ports, not their number: on this VM the run with no extra ports was repeatably
about 15 ms slower than with 100, in either order. Memory grows with the number of sockets.

Core library, `cargo bench -p holdmap-core` (Criterion, warm, in-process):

| Benchmark | Time (95% interval) |
| --- | --- |
| Scan: listeners and PID map | 10.50–10.66 ms |
| Topology build | 134.1–135.6 µs |
| Explain one port | 9.06–9.27 µs |
| Parse 2,000 `ss` lines (SSH inspection) | 643.5–648.7 µs |

### Analyzer overhead

The same 17 fixture skills (see [Evaluation](#evaluation)), scanned through
`holdmap analyzers run` and by running the scanner directly on each skill folder, one after
another. holdmap's part is the inventory, a private copy of each folder, the sandbox, the
cleared environment and parsing the SARIF report.

| Analyzer | Through holdmap | Scanner alone | Overhead per skill | Peak RSS, largest process |
| --- | --: | --: | --: | --: |
| `pycqa.bandit` | 3,258 ms | 2,570 ms | 40.5 ms (+27%) | 29.4 MiB |
| `aquasecurity.trivy-secrets` | 1,586 ms | 976 ms | 35.9 ms (+63%) | 107.3 MiB |

The largest process is the scanner (Trivy uses about 107 MiB on its own), not holdmap.

### Download size

From the v0.9.0 release assets (`scripts/bench.py` reads them from the GitHub API):

| Download | Size |
| --- | --: |
| CLI, macOS Apple silicon (`.tar.xz`) | 1.6 MiB |
| CLI, macOS Intel (`.tar.xz`) | 1.8 MiB |
| CLI, Linux x64 (`.tar.xz`, glibc / musl) | 1.9 / 2.0 MiB |
| CLI, Linux ARM64 (`.tar.xz`) | 1.6 MiB |
| CLI, Windows x64 (`.zip`) | 2.8 MiB |
| Desktop, macOS Apple silicon / Intel (`.dmg`) | 5.9 / 6.4 MiB |
| Desktop, Windows x64 (`.msi` / setup `.exe`) | 6.8 / 5.0 MiB |
| Desktop, Linux x64 (`.deb` / `.rpm` / `.AppImage`) | 6.9 / 6.9 / 81.6 MiB |

The AppImage bundles its own GTK and WebKitGTK libraries; the others use the system's web view.
The uncompressed release CLI binary built on the VM is 14.5 MiB. Pass `--desktop-bundle DIR`
to size a desktop bundle you built yourself.

## Evaluation

`python3 scripts/eval-analyzers.py` writes the 17 small skills in
`scripts/eval-fixtures.json`, each with known problems, to a throwaway home. It adds and approves
each installed analyzer there, runs it through holdmap and scores the findings. The
credential-shaped values in the fixtures are a fixed prefix plus a `{{fill:LENGTH:CHARSET}}`
placeholder (a key file is `{{pem:LINES}}`), filled from a fixed seed when the files are written. No real or
credential-shaped string is committed, and every run writes the same files.

| Skills | Labels |
| --- | --- |
| `clean-formatter`, `clean-notes`, `clean-changelog`, `clean-web` | none (clean) |
| `secret-aws`, `secret-github`, `secret-private-key` | hard-coded secret |
| `py-shell`, `py-eval-pickle`, `py-weak-crypto` | risky Python |
| `dep-pypi`, `dep-npm` | known-vulnerable pinned dependency |
| `inject-override`, `inject-exfil` | prompt injection or exfiltration in the instructions |
| `evade-bandit-config`, `evade-trivyignore` | risky Python / secret, plus scanner settings that hide it |
| `mixed` | secret and risky Python |

A skill counts as flagged when the analyzer reports at least one finding at or above the
chosen severity. Positives are the skills with a label the analyzer covers; negatives are the
four clean skills. Skills with only other labels aren't scored for that analyzer, so a secret
scanner isn't marked down for missing `shell=True`.

At medium severity or above (the default):

| Analyzer | Covers | TP | FP | FN | TN | Precision | Recall | Run |
| --- | --- | --: | --: | --: | --: | --: | --: | --: |
| Trivy secrets 0.76.0 | secrets | 5 | 0 | 0 | 4 | 1.00 | 1.00 | 1.6 s |
| Bandit 1.9.4 | risky Python | 4 | 0 | 1 | 4 | 1.00 | 0.80 | 3.3 s |
| OSV-Scanner 2.7.0 | vulnerable dependencies | 2 | 0 | 0 | 4 | 1.00 | 1.00 | 2.9 s |
| Cisco Skill Scanner 2.3.0 | injection, exfiltration | 2 | 1 | 0 | 3 | 0.67 | 1.00 | 84.5 s |
| SkillSpector 2.13.0 | all four | 12 | 0 | 1 | 4 | 1.00 | 0.92 | 45.9 s |

With `--min low`, Bandit also flags `mixed` (its `shell=True` on a fixed string is low
severity), giving recall 1.00. Cisco Skill Scanner then flags all four clean skills
(precision 0.33), for example with a low `MANIFEST_MISSING_LICENSE` because the fixtures declare
no license. The other rows don't change.

What it missed and why:

- **Bandit on `mixed`:** `subprocess.run("…", shell=True)` with a constant command is rated
  low, so it's below the medium threshold.
- **Cisco Skill Scanner on `clean-changelog`:** a medium `FILE_MAGIC_MISMATCH` on the
  `requirements.txt` that only holds a comment.
- **SkillSpector on `secret-private-key`:** it reported nothing at any severity for a bare PEM
  key file. Trivy and Cisco Skill Scanner both caught it.

**Settings a skill ships.** The `evade-*` skills ship a `.bandit` that skips the shell checks,
and a `.trivyignore` plus `trivy-secret.yaml` that switch off the Slack token rule. Run directly
in the skill folder, Bandit and Trivy report **0** findings for them. Through holdmap, which
leaves those files out of the copy, each reports **1**, and both count as true positives above.
The script prints this comparison.

This is a small hand-made set that shows how the pipeline behaves and catches regressions.
It doesn't measure how well these scanners do on real skills. Rules-only scanners miss what
they have no rule for, so no findings means "nothing matched", not "safe".

## Reproduce

Linux or macOS, with Python 3.10+ and a C compiler (`cc`, for the small peak-memory helper):

```sh
cargo build --release --locked -p holdmap
python3 scripts/bench.py --json bench-results.json   # about 2 minutes; --skip analyzers release to narrow it
cargo bench -p holdmap-core                          # Criterion micro-benchmarks
python3 scripts/eval-analyzers.py --json eval-results.json            # medium and above
python3 scripts/eval-analyzers.py --min low                           # every severity
```

Install the scanners you want evaluated first (the evaluation skips any that aren't on
`PATH`; see [Presets](https://santoshshinde2012.github.io/holdmap/docs/analyzers/#presets)).
OSV-Scanner, and SkillSpector's dependency check, contact `api.osv.dev`. Both scripts keep
everything holdmap stores in a temporary home and delete it afterwards (`--keep` keeps the
evaluation's). `--json` also records the exact environment.
