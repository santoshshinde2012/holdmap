// Authenticated lookup for both draft and published releases. GitHub's tag route
// returns published releases only, so draft assembly must resolve a numeric ID first.
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execute = promisify(execFile);
const exact = (pattern, value) => typeof value === "string" && pattern.exec(value)?.[0] === value;
const versionPattern = /^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
const MAX_RELEASES = 1000;

export async function lookupRelease(repository, tag) {
  if (!exact(/^[A-Za-z0-9][A-Za-z0-9-]*\/[A-Za-z0-9_.-]+$/, repository)
    || repository.length > 140 || [".", ".."].includes(repository.split("/")[1])
    || !exact(versionPattern, tag) || tag.length > 100
    || tag.split("-").slice(1).join("-").split(".").some((part) => /^0\d+$/.test(part))) {
    throw new Error("Invalid release repository or canonical version tag");
  }
  const deadline = performance.now() + 60_000;
  const get = async (path) => {
    const remaining = Math.floor(deadline - performance.now());
    if (remaining <= 0) throw new Error("Release lookup exceeded its deadline");
    const result = await execute("gh", ["api", "--method", "GET", "-H", "X-GitHub-Api-Version: 2022-11-28", path], {
      timeout: Math.min(30_000, remaining), maxBuffer: 2 * 1024 * 1024,
    });
    return JSON.parse(result.stdout);
  };
  const matches = [];
  let count = 0;
  for (let page = 1; ; page += 1) {
    const releases = await get(`repos/${repository}/releases?per_page=100&page=${page}`);
    if (!Array.isArray(releases) || releases.length > 100 || count + releases.length > MAX_RELEASES) {
      throw new Error("Invalid or unexpectedly large release list");
    }
    count += releases.length;
    for (const release of releases) {
      if (!release || !Number.isSafeInteger(release.id) || release.id <= 0 || typeof release.tag_name !== "string") {
        throw new Error("Invalid release identity in release list");
      }
      if (release.tag_name === tag) matches.push(release);
      if (matches.length > 1) throw new Error("Duplicate release tag in release list");
    }
    if (releases.length < 100) break;
  }
  if (matches.length !== 1) throw new Error(`Release not found for tag ${tag}`);
  const release = await get(`repos/${repository}/releases/${matches[0].id}`);
  if (!release || release.id !== matches[0].id || release.tag_name !== tag
    || typeof release.draft !== "boolean" || release.prerelease !== tag.includes("-")) {
    throw new Error("Release identity or prerelease status differs from TAG");
  }
  return release;
}
