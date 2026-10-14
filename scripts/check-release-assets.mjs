// Read-only release completeness gate. Publishing remains a separate workflow step.
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { promisify } from "node:util";
import { lookupRelease } from "./release-api.mjs";

const execute = promisify(execFile);
const repository = process.env.GITHUB_REPOSITORY;
const tag = process.env.TAG;
const exact = (pattern, value) => typeof value === "string" && pattern.exec(value)?.[0] === value;
const versionPattern = /^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
if (!exact(/^[A-Za-z0-9][A-Za-z0-9-]*\/[A-Za-z0-9_.-]+$/, repository)
  || repository.length > 140 || [".", ".."].includes(repository.split("/")[1])) {
  throw new Error("Invalid release repository; expected a literal owner/repository");
}
if (!exact(versionPattern, tag) || tag.length > 100
  || tag.split("-").slice(1).join("-").split(".").some((part) => /^0\d+$/.test(part))) {
  throw new Error("Invalid release tag; expected a literal vX.Y.Z version or prerelease");
}
if (![undefined, "true", "false"].includes(process.env.CHECK_RELEASE_DRAFT)) {
  throw new Error("CHECK_RELEASE_DRAFT must be true or false");
}
const version = tag.slice(1);
const prerelease = version.includes("-");
const api = async (path, limit = 2 * 1024 * 1024, binary = false) => {
  const args = ["api", "--method", "GET", "-H", "X-GitHub-Api-Version: 2022-11-28"];
  if (binary) args.push("-H", "Accept: application/octet-stream");
  const result = await execute("gh", [...args, path], { timeout: 30_000, maxBuffer: limit, encoding: "buffer" });
  return result.stdout;
};
const release = await lookupRelease(repository, tag);
if (!Number.isSafeInteger(release.id) || release.id <= 0 || release.tag_name !== tag
  || release.prerelease !== prerelease || typeof release.draft !== "boolean") {
  throw new Error("Release identity or prerelease status differs from TAG");
}
if (process.env.CHECK_RELEASE_DRAFT === "true" && !release.draft) {
  throw new Error("Release must remain a draft until verification succeeds");
}

// Explicit pagination keeps the embedded first page in release metadata from
// accidentally hiding a missing asset, duplicate, or unfinished upload.
const assets = [];
for (let page = 1; ; page += 1) {
  const batch = JSON.parse((await api(`repos/${repository}/releases/${release.id}/assets?per_page=100&page=${page}`)).toString("utf8"));
  if (!Array.isArray(batch) || batch.length > 100 || assets.length + batch.length > 256) {
    throw new Error("Invalid or unexpectedly large release asset list");
  }
  assets.push(...batch);
  if (batch.length < 100) break;
}
const byName = new Map();
const identifiers = new Set();
for (const asset of assets) {
  if (typeof asset?.name !== "string" || byName.has(asset.name)) throw new Error(`Duplicate or invalid release asset: ${asset?.name}`);
  if (!Number.isSafeInteger(asset.id) || asset.id <= 0 || identifiers.has(asset.id)) throw new Error(`Invalid or reused release asset ID: ${asset.name}`);
  byName.set(asset.name, asset);
  identifiers.add(asset.id);
}
function requireAsset(name) {
  const asset = byName.get(name);
  if (!asset) throw new Error(`Missing required release asset: ${name}`);
  if (asset.state !== "uploaded" || !Number.isSafeInteger(asset.size) || asset.size <= 0
    || !exact(/^sha256:[0-9a-f]{64}$/, asset.digest)) {
    throw new Error(`Release asset is not completely uploaded with a SHA-256 digest: ${name}`);
  }
  return asset;
}

const cli = [
  "aarch64-apple-darwin.tar.xz", "x86_64-apple-darwin.tar.xz",
  "aarch64-unknown-linux-gnu.tar.xz", "x86_64-unknown-linux-gnu.tar.xz",
  "x86_64-unknown-linux-musl.tar.xz", "x86_64-pc-windows-msvc.zip",
].map((suffix) => `holdmap-${suffix}`);
const desktop = {
  "macos-arm64": [`holdmap_${version}_aarch64.dmg`],
  "macos-x64": [`holdmap_${version}_x64.dmg`],
  "windows-x64": [`holdmap_${version}_x64_en-US.msi`, `holdmap_${version}_x64-setup.exe`],
  "linux-x64": [`holdmap_${version}_amd64.deb`, `holdmap-${version}-1.x86_64.rpm`, `holdmap_${version}_amd64.AppImage`],
};
const globalArchives = [...cli, "holdmap-npm-package.tar.gz", "source.tar.gz"];
const required = [
  ...cli.flatMap((name) => [name, `${name}.sha256`]),
  ...Object.values(desktop).flat(),
  ...Object.keys(desktop).map((platform) => `holdmap-desktop-${platform}.sha256`),
  "holdmap-installer.sh", "holdmap-installer.ps1", "holdmap-npm-package.tar.gz",
  "source.tar.gz", "source.tar.gz.sha256", "dist-manifest.json", "sha256.sum",
  ...["holdmap", "holdmap-core", "holdmap-activity", "holdmap-mcp", "holdmap-desktop"].map((name) => `${name}.cdx.xml`),
];
required.forEach(requireAsset);

async function readVerified(name, limit = 64 * 1024) {
  const asset = requireAsset(name);
  if (asset.size > limit) throw new Error(`Release metadata asset exceeds its size limit: ${name}`);
  // Fetch by the API's validated numeric ID, never by a supplied download URL.
  const bytes = await api(`repos/${repository}/releases/assets/${asset.id}`, limit, true);
  if (bytes.length !== asset.size || `sha256:${createHash("sha256").update(bytes).digest("hex")}` !== asset.digest) {
    throw new Error(`Downloaded release metadata digest or size differs: ${name}`);
  }
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}

async function verifyChecksums(name, expected) {
  const lines = (await readVerified(name)).split(/\r?\n/).filter((line) => line.length);
  if (!lines.length || lines.length > 256) throw new Error(`Invalid checksum manifest: ${name}`);
  const seen = new Set();
  for (const line of lines) {
    const entry = /^([0-9a-fA-F]{64}) [ *]([A-Za-z0-9][A-Za-z0-9._+-]*)$/.exec(line);
    if (!entry || entry[0] !== line || seen.has(entry[2])) throw new Error(`Invalid or duplicate checksum entry in ${name}`);
    const [, digest, filename] = entry;
    if (requireAsset(filename).digest !== `sha256:${digest.toLowerCase()}`) throw new Error(`Checksum differs from uploaded asset: ${filename}`);
    seen.add(filename);
  }
  for (const filename of expected) {
    if (!seen.has(filename)) throw new Error(`Checksum manifest ${name} does not cover ${filename}`);
  }
}

await Promise.all([
  ...cli.map((name) => verifyChecksums(`${name}.sha256`, [name])),
  verifyChecksums("source.tar.gz.sha256", ["source.tar.gz"]),
  verifyChecksums("sha256.sum", globalArchives),
  ...Object.entries(desktop).map(([platform, names]) => verifyChecksums(`holdmap-desktop-${platform}.sha256`, names)),
]);
const manifest = JSON.parse(await readVerified("dist-manifest.json", 1024 * 1024));
if (manifest.announcement_tag !== tag || manifest.announcement_is_prerelease !== prerelease
  || !Array.isArray(manifest.releases) || manifest.releases.length !== 1
  || manifest.releases[0].app_name !== "holdmap" || manifest.releases[0].app_version !== version) {
  throw new Error("Distribution manifest version or release identity differs from TAG");
}
console.log(`Verified ${required.length} required release assets and their checksum coverage for ${repository} ${tag}; no release changes made`);
