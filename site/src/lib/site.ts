// Site-wide facts: links, the latest release and its download files. Resolved once at build
// time; the Pages workflow rebuilds after every release, so links always match the latest one.
import versionTxt from "../../../version.txt?raw";
import { publishedReleaseVersion } from "./published-release.mjs";

export const REPO = "santoshshinde2012/holdmap";
export const repoUrl = `https://github.com/${REPO}`;
export const siteUrl = "https://santoshshinde2012.github.io/holdmap/";
export const releasesUrl = `${repoUrl}/releases`;
export const latestUrl = `${repoUrl}/releases/latest`;
export const changelogUrl = `${repoUrl}/blob/main/CHANGELOG.md`;
export const licenseUrl = `${repoUrl}#license`;
export const securityUrl = `${repoUrl}/blob/main/SECURITY.md`;
export const contributingUrl = `${repoUrl}/blob/main/CONTRIBUTING.md`;
export const issuesUrl = `${repoUrl}/issues`;

export const tagline =
  "See which ports, agents and tools are running — and stop the right thing safely.";

/** Stars are only shown once the number says something. */
const STARS_SHOWN_FROM = 25;

type Repo = { stargazers_count?: number };
const offline = process.env.HOLDMAP_SITE_OFFLINE === "1";
const requireRelease = process.env.HOLDMAP_SITE_REQUIRE_RELEASE === "1";

async function github<T>(path: string): Promise<T | null> {
  if (offline) return null;
  const headers: Record<string, string> = { accept: "application/vnd.github+json", "user-agent": "holdmap-site" };
  if (process.env.GITHUB_TOKEN) headers.authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  try {
    const r = await fetch(`https://api.github.com/repos/${REPO}${path}`, { headers, signal: AbortSignal.timeout(5000) });
    return r.ok ? ((await r.json()) as T) : null;
  } catch {
    return null;
  }
}

const fileVersion = versionTxt.trim();
const [release, repo] = await Promise.all([github<unknown>("/releases/latest"), github<Repo>("")]);

/** Publishing requires a validated stable release; local builds can fall back to version.txt. */
export const version = publishedReleaseVersion({ sourceVersion: fileVersion, release, repositoryUrl: repoUrl, offline, requireRelease });
/** The version on main (version.txt). It can be ahead of the latest published release. */
export const sourceVersion = fileVersion;
const parts = (v: string) => v.split(".").map(Number);
/** Whether published version `have` is at least `want` (both `x.y.z`). */
export function atLeast(have: string, want: string): boolean {
  const [a, b] = [parts(have), parts(want)];
  for (let i = 0; i < 3; i++) if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  return true;
}
/** The release highlights describe the source version; true once installers for it are published. */
export const releaseHighlightsPublished = atLeast(version, sourceVersion);
/** The release that first ships Home (Cards / Map), the richer per-agent inventory and stopping ports from Home.
 *  Until it is the published release, the site keeps its "not in a published release yet" note. */
export const homeRelease: string | null = "0.9.0";
export const homePublished = homeRelease !== null && atLeast(version, homeRelease);
export const description = releaseHighlightsPublished
  ? "holdmap shows your agents, MCP servers, skills, tools, apps and ports, and stops services safely after a reviewed plan. macOS, Linux and Windows."
  : `holdmap shows your agents, MCP servers, skills, tools, apps and ports, and stops services safely. Latest release v${version}; ${sourceVersion} is in preparation.`;
export const stars = repo?.stargazers_count && repo.stargazers_count >= STARS_SHOWN_FROM ? repo.stargazers_count : null;

// Keep the chosen release label, filenames and checksums together while a new release builds.
const dl = (file: string) => `${repoUrl}/releases/download/v${version}/${file}`;

export const install = {
  sh: `curl --proto '=https' --tlsv1.2 -LsSf ${dl("holdmap-installer.sh")} | sh`,
  ps: `powershell -ExecutionPolicy Bypass -c "irm ${dl("holdmap-installer.ps1")} | iex"`,
  cargo: `cargo install --locked --git ${repoUrl} --tag v${version} holdmap`,
};

export type Os = "macos" | "windows" | "linux";
export type Download = { os: Os; label: string; detail: string; file: string; url: string; checksum: string; primary?: boolean };

const d = (os: Os, label: string, detail: string, file: string, checksum: string, primary = false): Download => ({
  os, label, detail, file, url: dl(file), checksum, primary,
});

export const downloads: Download[] = [
  d("macos", "macOS · Apple silicon", ".dmg", `holdmap_${version}_aarch64.dmg`, "holdmap-desktop-macos-arm64.sha256", true),
  d("macos", "macOS · Intel", ".dmg", `holdmap_${version}_x64.dmg`, "holdmap-desktop-macos-x64.sha256"),
  d("windows", "Windows · installer", ".exe", `holdmap_${version}_x64-setup.exe`, "holdmap-desktop-windows-x64.sha256", true),
  d("windows", "Windows · MSI", ".msi", `holdmap_${version}_x64_en-US.msi`, "holdmap-desktop-windows-x64.sha256"),
  d("linux", "Linux · AppImage", ".AppImage", `holdmap_${version}_amd64.AppImage`, "holdmap-desktop-linux-x64.sha256", true),
  d("linux", "Linux · Debian, Ubuntu", ".deb", `holdmap_${version}_amd64.deb`, "holdmap-desktop-linux-x64.sha256"),
  d("linux", "Linux · Fedora, RHEL", ".rpm", `holdmap-${version}-1.x86_64.rpm`, "holdmap-desktop-linux-x64.sha256"),
];

export const checksumUrl = (file: string) => dl(file);

/** `href` under the site's base path, e.g. `url("docs/")` → `/holdmap/docs/`. */
export const url = (path = "") => `${import.meta.env.BASE_URL.replace(/\/$/, "")}/${path.replace(/^\//, "")}`;
