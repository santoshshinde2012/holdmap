// Stage exactly the files published by the pinned tauri-action before hashing/uploading them.
// Its default macOS archive names differ from disk names; see getAssetName/ghAssetName in
// tauri-apps/tauri-action@1deb371b0cd8bd54025b384f1cd735e725c4060f/src/utils.ts.
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { appendFile, copyFile, lstat, mkdtemp, realpath, writeFile } from "node:fs/promises";
import { basename, join, sep } from "node:path";

const matrix = process.env.RELEASE_MATRIX;
const version = process.env.RELEASE_VERSION;
const updater = process.env.RELEASE_UPDATER === "true";
const architectures = { "macos-arm64": "aarch64", "macos-x64": "x64", "linux-x64": "", "windows-x64": "" };
if (!Object.hasOwn(architectures, matrix) || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version ?? "")) {
  throw new Error("Invalid release matrix or app version");
}
const artifacts = JSON.parse(process.env.TAURI_ARTIFACT_PATHS || "[]");
if (!Array.isArray(artifacts) || artifacts.some((path) => typeof path !== "string")) {
  throw new Error("Invalid tauri-action artifactPaths output");
}
const files = [];
const target = await realpath("target");
if (!target.startsWith(`${await realpath(".")}${sep}`)) throw new Error("Workspace target directory cannot escape the checkout");
for (const path of artifacts) {
  const canonical = await realpath(path);
  if (!canonical.startsWith(`${target}${sep}`)) {
    throw new Error("Release artifact must be inside the workspace target directory");
  }
  const stat = await lstat(path);
  if (stat.isSymbolicLink()) throw new Error("Release artifacts cannot be symbolic links");
  if (stat.isFile() && (updater || /\.(dmg|msi|exe|deb|rpm|AppImage)$/.test(path))) files.push(path);
}
if (!files.some((path) => /\.(dmg|msi|exe|deb|rpm|AppImage)$/.test(path))) {
  throw new Error("No desktop installers were built");
}
if (updater && !files.some((path) => path.endsWith(".sig"))) {
  throw new Error("Configured updater artifacts must include signatures");
}
for (const path of files) {
  if (updater && /\.(app\.tar\.gz|msi\.zip|nsis\.zip|AppImage\.tar\.gz)$/.test(path) && !files.includes(`${path}.sig`)) {
    throw new Error("Updater archive is missing its signature");
  }
  if (path.endsWith(".sig") && !files.includes(path.slice(0, -4))) {
    throw new Error("Updater signature is missing its archive or installer");
  }
}
// A fresh private directory cannot reuse a symlink left in a restored target cache.
const staging = await mkdtemp(join(target, "desktop-release-assets-"));
const published = new Map();
for (const path of files) {
  let name = basename(path);
  const suffix = name.endsWith(".app.tar.gz.sig") ? ".app.tar.gz.sig" : name.endsWith(".app.tar.gz") ? ".app.tar.gz" : null;
  if (suffix) {
    if (!architectures[matrix]) throw new Error("macOS updater archive appeared on another platform");
    name = `${name.slice(0, -suffix.length)}_${version}_${architectures[matrix]}${suffix}`;
  }
  // GitHub's filename normalization, matched to the pinned action's ghAssetName.
  name = name.trim().replace(/[^a-zA-Z0-9_-]/g, ".").replace(/\.\./g, ".");
  if (published.has(name)) throw new Error("Release artifact names collide after normalization");
  const destination = join(staging, name);
  await copyFile(path, destination);
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(destination)) hash.update(chunk);
  published.set(name, { path: destination, digest: hash.digest("hex") });
}
const sum = `holdmap-desktop-${matrix}.sha256`;
const paths = [...published.values()].map(({ path }) => path).join("\n") + "\n";
await writeFile(sum, [...published].map(([name, { digest }]) => `${digest}  ${name}`).join("\n") + "\n");
await writeFile(`${sum}.paths`, paths);
await writeFile(`${sum}.json`, JSON.stringify([...published].map(([name, asset]) => ({ name, ...asset }))) + "\n");
if (process.env.GITHUB_OUTPUT) await appendFile(process.env.GITHUB_OUTPUT, `sum=${sum}\npaths<<__P__\n${paths}__P__\n`);
console.log(`Collected ${published.size} desktop release assets with matching published names`);
