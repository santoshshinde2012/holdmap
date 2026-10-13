// Publish one complete updater manifest after every platform's signature checks and uploads pass.
// Build jobs produce these manifests only after minisign verification by verify_updater.rs.
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { promisify } from "node:util";

const repository = process.env.GITHUB_REPOSITORY;
const tag = process.env.TAG;
if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository ?? "") || !/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag ?? "")) {
  throw new Error("Invalid updater release repository or tag");
}
const expected = {
  "macos-arm64": ["darwin-aarch64", ["app"]],
  "macos-x64": ["darwin-x86_64", ["app"]],
  "linux-x64": ["linux-x86_64", ["appimage", "deb", "rpm"]],
  "windows-x64": ["windows-x86_64", ["nsis", "msi"]],
};
const suffixes = { app: [".app.tar.gz"], appimage: [".AppImage", ".AppImage.tar.gz"], nsis: [".exe", ".nsis.zip"], msi: [".msi", ".msi.zip"], deb: [".deb"], rpm: [".rpm"] };
const directory = process.env.UPDATER_MANIFEST_DIR;
const files = (await readdir(directory)).filter((name) => name.endsWith(".updater.json"));
if (files.length !== Object.keys(expected).length) throw new Error("Updater requires every platform manifest");
const platforms = new Map();
const matrices = new Set();
for (const file of files) {
  const bytes = await readFile(join(directory, file));
  if (bytes.length > 1024 * 1024) throw new Error("Updater platform manifest is too large");
  const manifest = JSON.parse(bytes);
  if (!Object.hasOwn(expected, manifest.matrix) || matrices.has(manifest.matrix) || manifest.version !== tag.slice(1)) {
    throw new Error("Updater platform or version is inconsistent with the release");
  }
  matrices.add(manifest.matrix);
  const [target, installers] = expected[manifest.matrix];
  if (!manifest.platforms || typeof manifest.platforms !== "object" || Array.isArray(manifest.platforms) || !Object.hasOwn(manifest.platforms, target)) {
    throw new Error("Updater platform manifest has no primary installer");
  }
  const allowed = new Set([target, ...installers.map((installer) => `${target}-${installer}`)]);
  const entries = Object.entries(manifest.platforms);
  if (entries.length < 2 || entries.length > allowed.size) throw new Error("Invalid updater installer count");
  for (const [platform, asset] of entries) {
    if (!allowed.has(platform) || platforms.has(platform) || !asset || !/^[A-Za-z0-9_.-]+$/.test(asset.name ?? "") || !/^[0-9a-f]{64}$/.test(asset.digest ?? "") || typeof asset.signature !== "string" || asset.signature.length > 16 * 1024 || !/^[A-Za-z0-9+/]+={0,2}$/.test(asset.signature)) {
      throw new Error("Invalid verified updater asset metadata");
    }
    const kinds = platform === target ? installers : [platform.slice(target.length + 1)];
    if (!kinds.some((kind) => suffixes[kind].some((suffix) => asset.name.endsWith(suffix)))) {
      throw new Error("Updater artifact has an unexpected installer format");
    }
    platforms.set(platform, asset);
  }
  const primary = manifest.platforms[target];
  if (!entries.some(([platform, asset]) => platform !== target && asset.name === primary.name && asset.digest === primary.digest && asset.signature === primary.signature)) {
    throw new Error("Updater primary installer does not match a verified installer target");
  }
}

const execute = promisify(execFile);
const gh = async (args) => (await execute("gh", args, { timeout: 120_000, maxBuffer: 10 * 1024 * 1024 })).stdout;
const release = JSON.parse(await gh(["api", `repos/${repository}/releases/tags/${tag}`]));
if (!Number.isSafeInteger(release.id) || release.id <= 0 || release.tag_name !== tag) throw new Error("Invalid GitHub release");
const list = async () => JSON.parse(await gh(["api", "--paginate", "--slurp", `repos/${repository}/releases/${release.id}/assets?per_page=100`])).flat();
const assets = await list();
const published = {};
for (const [platform, artifact] of platforms) {
  const matches = assets.filter((asset) => asset.name === artifact.name);
  if (matches.length !== 1 || !Number.isSafeInteger(matches[0].id) || matches[0].id <= 0 || matches[0].digest !== `sha256:${artifact.digest}`) {
    throw new Error(`Published updater asset differs from verified artifact: ${artifact.name}`);
  }
  published[platform] = {
    signature: artifact.signature,
    url: `https://api.github.com/repos/${repository}/releases/assets/${matches[0].id}`,
  };
}
const pubDate = release.published_at ?? release.created_at;
if (typeof pubDate !== "string" || !Number.isFinite(Date.parse(pubDate))) throw new Error("Invalid release publication date");
const payload = JSON.stringify({ version: tag.slice(1), notes: release.body ?? "", pub_date: pubDate, platforms: published }, null, 2) + "\n";
const path = join(directory, "latest.json");
await writeFile(path, payload);
const digest = `sha256:${createHash("sha256").update(payload).digest("hex")}`;
const matchesPublished = async () => {
  const current = (await list()).filter((asset) => asset.name === "latest.json");
  if (current.length > 1) throw new Error("Duplicate latest.json assets");
  return current[0]?.digest === digest;
};
for (let attempt = 1; attempt <= 5; attempt++) {
  if (await matchesPublished()) break;
  try {
    // Only the index may be replaced; signed artifact IDs above remain stable.
    await gh(["release", "upload", tag, path, "--clobber", "--repo", repository]);
  } catch (error) {
    if (await matchesPublished()) break;
    if (attempt === 5) throw error;
    await new Promise((resolve) => setTimeout(resolve, attempt * 10_000));
    continue;
  }
  if (!(await matchesPublished())) throw new Error("Published latest.json digest differs from the verified index");
  break;
}
console.log(`Published complete signed updater index for ${platforms.size} platform/installer targets`);
