// Site-wide facts: links, the latest release and its download files. Resolved once at build
// time; the Pages workflow rebuilds after every release, so links always match the latest one.
import versionTxt from "../../../version.txt?raw";

export const REPO = "santoshshinde2012/portwise";
export const repoUrl = `https://github.com/${REPO}`;
export const siteUrl = "https://santoshshinde2012.github.io/portwise/";
export const releasesUrl = `${repoUrl}/releases`;
export const latestUrl = `${repoUrl}/releases/latest`;
export const changelogUrl = `${repoUrl}/blob/main/CHANGELOG.md`;
export const licenseUrl = `${repoUrl}#license`;
export const securityUrl = `${repoUrl}/blob/main/SECURITY.md`;
export const contributingUrl = `${repoUrl}/blob/main/CONTRIBUTING.md`;
export const issuesUrl = `${repoUrl}/issues`;

export const tagline =
  "See which ports, agents and tools are running — and stop the right thing safely.";
export const description =
  "portwise maps your agents, tools and apps to the ports they hold, explains why a port is busy, and stops only what it should. A CLI, TUI, desktop app and MCP server on one Rust core. macOS, Linux and Windows.";

/** Stars are only shown once the number says something. */
const STARS_SHOWN_FROM = 25;

type Repo = { stargazers_count?: number };
type Release = { tag_name?: string };

async function github<T>(path: string): Promise<T | null> {
  if (process.env.PORTWISE_SITE_OFFLINE) return null;
  const headers: Record<string, string> = { accept: "application/vnd.github+json", "user-agent": "portwise-site" };
  if (process.env.GITHUB_TOKEN) headers.authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  try {
    const r = await fetch(`https://api.github.com/repos/${REPO}${path}`, { headers, signal: AbortSignal.timeout(5000) });
    return r.ok ? ((await r.json()) as T) : null;
  } catch {
    return null;
  }
}

const fileVersion = versionTxt.trim();
const [release, repo] = await Promise.all([github<Release>("/releases/latest"), github<Repo>("")]);

/** The latest published release (falls back to version.txt when GitHub can't be reached). */
export const version = release?.tag_name?.replace(/^v/, "") || fileVersion;
export const stars = repo?.stargazers_count && repo.stargazers_count >= STARS_SHOWN_FROM ? repo.stargazers_count : null;

const dl = (file: string) => `${repoUrl}/releases/latest/download/${file}`;

export const install = {
  sh: `curl -LsSf ${dl("portwise-installer.sh")} | sh`,
  ps: `powershell -ExecutionPolicy Bypass -c "irm ${dl("portwise-installer.ps1")} | iex"`,
  cargo: `cargo install --locked --git ${repoUrl} portwise`,
};

export type Os = "macos" | "windows" | "linux";
export type Download = { os: Os; label: string; detail: string; file: string; url: string; checksum: string; primary?: boolean };

const d = (os: Os, label: string, detail: string, file: string, checksum: string, primary = false): Download => ({
  os, label, detail, file, url: dl(file), checksum, primary,
});

export const downloads: Download[] = [
  d("macos", "macOS · Apple silicon", ".dmg", `portwise_${version}_aarch64.dmg`, "portwise-desktop-macos-arm64.sha256", true),
  d("macos", "macOS · Intel", ".dmg", `portwise_${version}_x64.dmg`, "portwise-desktop-macos-x64.sha256"),
  d("windows", "Windows · installer", ".exe", `portwise_${version}_x64-setup.exe`, "portwise-desktop-windows-x64.sha256", true),
  d("windows", "Windows · MSI", ".msi", `portwise_${version}_x64_en-US.msi`, "portwise-desktop-windows-x64.sha256"),
  d("linux", "Linux · AppImage", ".AppImage", `portwise_${version}_amd64.AppImage`, "portwise-desktop-linux-x64.sha256", true),
  d("linux", "Linux · Debian, Ubuntu", ".deb", `portwise_${version}_amd64.deb`, "portwise-desktop-linux-x64.sha256"),
  d("linux", "Linux · Fedora, RHEL", ".rpm", `portwise-${version}-1.x86_64.rpm`, "portwise-desktop-linux-x64.sha256"),
];

export const checksumUrl = (file: string) => dl(file);

/** `href` under the site's base path, e.g. `url("docs/")` → `/portwise/docs/`. */
export const url = (path = "") => `${import.meta.env.BASE_URL.replace(/\/$/, "")}/${path.replace(/^\//, "")}`;
