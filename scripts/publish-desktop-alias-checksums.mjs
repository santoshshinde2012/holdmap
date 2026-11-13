// Publish holdmap-desktop-aliases.sha256: the version-free installer names, one line each,
// with the same digest already recorded in the per-platform checksum files.
// v0.10–v0.12 carried this file as a manual upload. Later releases dropped it because no
// job built it. This runs after every platform checksum is on the release.
import { execFile } from "node:child_process";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";

const execute = promisify(execFile);
const tag = process.env.TAG;
const repository = process.env.GITHUB_REPOSITORY;
if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag ?? "")) throw new Error("TAG must be a version tag");
if (!/^[A-Za-z0-9][A-Za-z0-9-]*\/[A-Za-z0-9_.-]+$/.test(repository ?? "")) {
  throw new Error("GITHUB_REPOSITORY must be owner/name");
}

const aliases = [
  "holdmap-linux-x64.AppImage",
  "holdmap-linux-x64.deb",
  "holdmap-linux-x64.rpm",
  "holdmap-macos-arm64.dmg",
  "holdmap-macos-x64.dmg",
  "holdmap-windows-x64-setup.exe",
  "holdmap-windows-x64.msi",
];
const platforms = ["linux-x64", "macos-arm64", "macos-x64", "windows-x64"];

const dir = await mkdtemp(join(tmpdir(), "holdmap-alias-"));
async function download(name) {
  await execute("gh", [
    "release", "download", tag, "--repo", repository, "--pattern", name, "--dir", dir, "--clobber",
  ], { timeout: 60_000 });
  return readFile(join(dir, name), "utf8");
}

const lines = new Map();
for (const platform of platforms) {
  const body = await download(`holdmap-desktop-${platform}.sha256`);
  for (const line of body.split(/\r?\n/)) {
    if (!line) continue;
    const match = /^([0-9a-f]{64}) {2}([A-Za-z0-9][A-Za-z0-9._+-]*)$/.exec(line);
    if (!match) throw new Error(`Unreadable checksum line in holdmap-desktop-${platform}.sha256`);
    if (aliases.includes(match[2])) {
      if (lines.has(match[2]) && lines.get(match[2]) !== match[1]) {
        throw new Error(`Two digests for ${match[2]}`);
      }
      lines.set(match[2], match[1]);
    }
  }
}
for (const name of aliases) {
  if (!lines.has(name)) throw new Error(`Platform checksums do not cover ${name}`);
}
const body = aliases.map((name) => `${lines.get(name)}  ${name}`).join("\n") + "\n";
const { writeFile } = await import("node:fs/promises");
const path = "holdmap-desktop-aliases.sha256";
await writeFile(path, body);
await execute("gh", ["release", "upload", tag, path, "--clobber", "--repo", repository], { timeout: 60_000 });
console.log(`Uploaded ${path} covering ${aliases.length} version-free installers`);
