// Browser demo installs, loaded only when the demo installs something. Nothing runs.
import type { InstallEvent, InstallMethod, InstallPlan, InstallRecord } from "./analyzer-install";

interface DemoRecipe { method: InstallMethod; package: string; version: string; source: string; verification: string; hosts: string[]; help_url: string; safest: string | null; toolchain: string | null }

const PYPI = ["pypi.org", "files.pythonhosted.org"];
const UV_HELP = "https://docs.astral.sh/uv/getting-started/installation/";
const pinned = (v: string, where: string) => `Pinned to version ${v} from ${where} over HTTPS. holdmap then checks the version it reports and records the program's SHA-256; approval pins that hash.`;

/** The same recipes as install.rs (a vitest keeps them equal). */
export const DEMO_RECIPES: Record<string, DemoRecipe> = {
  "nvidia.skillspector": { method: "uv_tool", package: "git+https://github.com/NVIDIA/skillspector.git@v2.13.0", version: "2.13.0", source: "GitHub: NVIDIA/skillspector, tag v2.13.0 (its Python dependencies from PyPI)", verification: "Pinned to the v2.13.0 tag over HTTPS. holdmap then checks the version it reports and records the program's SHA-256; approval pins that hash.", hosts: ["github.com", ...PYPI], help_url: UV_HELP, safest: null, toolchain: "uv" },
  "cisco.skill-scanner": { method: "uv_tool", package: "cisco-ai-skill-scanner==2.3.0", version: "2.3.0", source: "PyPI: cisco-ai-skill-scanner 2.3.0", verification: pinned("2.3.0", "PyPI"), hosts: PYPI, help_url: UV_HELP, safest: null, toolchain: "uv" },
  "pycqa.bandit": { method: "uv_tool", package: "bandit[sarif]==1.9.4", version: "1.9.4", source: "PyPI: bandit 1.9.4 with its SARIF output extra", verification: pinned("1.9.4", "PyPI"), hosts: PYPI, help_url: UV_HELP, safest: null, toolchain: "uv" },
  "google.osv-scanner": { method: "go_install", package: "github.com/google/osv-scanner/v2/cmd/osv-scanner@v2.7.0", version: "2.7.0", source: "Go module proxy: github.com/google/osv-scanner/v2 v2.7.0, built locally", verification: "Go checks every module against the public checksum database (sum.golang.org). holdmap then checks the version it reports and records the program's SHA-256.", hosts: ["proxy.golang.org", "sum.golang.org"], help_url: "https://go.dev/doc/install", safest: null, toolchain: "go" },
  "aquasecurity.trivy-secrets": { method: "manual", package: "trivy 0.76.0", version: "0.76.0", source: "Aqua Security release binaries or your OS package manager", verification: "Check the release's published checksum yourself; holdmap records the program's SHA-256 when you approve it.", hosts: [], help_url: "https://trivy.dev/latest/getting-started/installation/", safest: "Trivy ships as a prebuilt program, not a Python or Go package, so holdmap can't install it into its own folder. The safest way is your OS package manager (for example `brew install trivy` on macOS) or the official release from GitHub after checking its published checksum.", toolchain: null },
};

/** Demo toolchains: `?demoInstall=no-go` hides Go, `?demoInstall=fail` makes installs fail. */
export function demoInstallMode(): string {
  return new URLSearchParams(globalThis.location?.search ?? "").get("demoInstall") ?? "";
}

export function demoPlan(id: string, name: string, fallback: string | null, reads: string[], runNetwork: string[], installed: InstallRecord | null, mode = demoInstallMode()): InstallPlan {
  const r = DEMO_RECIPES[id];
  const target = `~/Library/Application Support/holdmap/analyzers/tools/${id}`;
  if (!r) {
    return { analyzer: id, name, method: "manual", package: "", version: "", source: "", verification: "", target_dir: target, network: [], reads, run_network: runNetwork, toolchain: null, command: [], fallback, safest: null, help_url: "", can_install: false, blocked: `holdmap has no install recipe for ${name}. Install it yourself, then come back.`, installed };
  }
  const missing = (r.toolchain === "go" && mode.includes("no-go")) || (r.toolchain === "uv" && mode.includes("no-uv"));
  const toolchain = r.toolchain ? { name: r.toolchain, path: missing ? null : `/opt/homebrew/bin/${r.toolchain}`, help_url: r.help_url } : null;
  const command = !toolchain?.path || r.method === "manual" ? [] : r.method === "uv_tool" ? [toolchain.path, "tool", "install", "--force", "--no-config", r.package] : [toolchain.path, "install", r.package];
  const blocked = r.method === "manual" ? r.safest : missing ? `${name} needs ${r.toolchain}, which wasn't found. Install ${r.toolchain} from its official page, then try again.` : null;
  return { analyzer: id, name, method: r.method, package: r.package, version: r.version, source: r.source, verification: r.verification, target_dir: target, network: r.hosts, reads, run_network: runNetwork, toolchain, command, fallback, safest: r.safest, help_url: r.help_url, can_install: !blocked, blocked, installed };
}

/** Simulated install: emits the same updates the app does. Nothing is downloaded or run. */
export async function demoInstall(plan: InstallPlan, on: (e: InstallEvent) => void, cancelled: () => boolean, mode = demoInstallMode(), tick = 250): Promise<InstallRecord> {
  const wait = () => new Promise((r) => setTimeout(r, tick));
  if (!plan.can_install) {
    const reason = plan.blocked ?? "This analyzer can't be installed here.";
    on({ kind: "failed", reason });
    throw new Error(reason);
  }
  on({ kind: "started", command: plan.command });
  const lines = [`Browser demo: nothing is downloaded or installed.`, `Resolving ${plan.package}`, `Downloading from ${plan.network.join(", ")}`, `Installing into ${plan.target_dir}`];
  for (const line of lines) {
    await wait();
    if (cancelled()) { on({ kind: "cancelled" }); throw new Error("cancelled"); }
    on({ kind: "log", line });
  }
  if (mode.includes("fail")) {
    const reason = "The package manager stopped with exit code 2: No solution found when resolving tool dependencies. Check your network connection and try again.";
    on({ kind: "failed", reason });
    throw new Error(reason);
  }
  on({ kind: "verifying" });
  await wait();
  const bin = plan.package.match(/([a-z0-9-]+)(?:\[|==|@|\.git|$)/i)?.[1] ?? plan.analyzer;
  const record: InstallRecord = { analyzer: plan.analyzer, version: plan.version, binary: `${plan.target_dir}/bin/${bin}`, sha256: "demo".padEnd(64, "0"), installed_at_ms: Date.now(), method: plan.method, package: plan.package };
  on({ kind: "done", record });
  return record;
}
