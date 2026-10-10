// Preserve IDs embedded in tauri-action's latest.json: signed assets may be added, never replaced.
import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { basename } from "node:path";
import { promisify } from "node:util";

const execute = promisify(execFile);
const repository = process.env.GITHUB_REPOSITORY;
const tag = process.env.TAG;
if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository ?? "") || !/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag ?? "")) {
  throw new Error("Invalid release repository or tag");
}
const manifest = JSON.parse(await readFile(process.env.RELEASE_ASSET_MANIFEST, "utf8"));
if (!Array.isArray(manifest) || manifest.length === 0 || manifest.some((asset) => typeof asset.name !== "string" || typeof asset.path !== "string" || basename(asset.path) !== asset.name || !/^[0-9a-f]{64}$/.test(asset.digest ?? "")) || new Set(manifest.map((asset) => asset.name)).size !== manifest.length) {
  throw new Error("Invalid staged release asset manifest");
}
const gh = async (args) => (await execute("gh", args, { timeout: 120_000, maxBuffer: 10 * 1024 * 1024 })).stdout;
const release = JSON.parse(await gh(["api", `repos/${repository}/releases/tags/${tag}`]));
if (!Number.isSafeInteger(release.id)) throw new Error("Invalid GitHub release ID");
const list = async () => JSON.parse(await gh(["api", "--paginate", "--slurp", `repos/${repository}/releases/${release.id}/assets?per_page=100`])).flat();
const matching = (assets, asset) => {
  const existing = assets.filter((candidate) => candidate.name === asset.name);
  if (existing.length > 1) throw new Error(`Duplicate published asset: ${asset.name}`);
  if (existing[0] && (!Number.isSafeInteger(existing[0].id) || existing[0].digest !== `sha256:${asset.digest}`)) {
    throw new Error(`Published signed asset digest differs or is unavailable: ${asset.name}`);
  }
  return existing[0];
};
// Validate every existing asset before publishing any missing files.
const existing = await list();
const retained = new Map();
for (const asset of manifest) {
  const match = matching(existing, asset);
  if (match) retained.set(asset.name, match.id);
}
for (const asset of manifest) {
  if (retained.has(asset.name)) continue;
  let uploaded = false;
  for (let attempt = 1; attempt <= 5; attempt++) {
    const match = matching(await list(), asset);
    if (match) { uploaded = true; break; }
    try {
      // Never use --clobber: deleting an asset invalidates latest.json's API URL.
      await gh(["release", "upload", tag, asset.path, "--repo", repository]);
    } catch (error) {
      const accepted = matching(await list(), asset);
      if (accepted) { uploaded = true; break; }
      if (attempt === 5) throw error;
      await new Promise((resolve) => setTimeout(resolve, attempt * 10_000));
      continue;
    }
    if (!matching(await list(), asset)) throw new Error(`Uploaded release asset was not found: ${asset.name}`);
    uploaded = true;
    break;
  }
  if (!uploaded) throw new Error(`Could not upload release asset: ${asset.name}`);
}
const final = await list();
for (const asset of manifest) {
  const match = matching(final, asset);
  if (!match || (retained.has(asset.name) && match.id !== retained.get(asset.name))) {
    throw new Error(`Published signed asset ID changed: ${asset.name}`);
  }
}
console.log(`Verified ${manifest.length} signed release assets; preserved ${retained.size} existing asset IDs`);
