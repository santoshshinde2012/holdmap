// Resolve website content separately from the trusted workflow's deployment revision.
import { execFile } from "node:child_process";
import { appendFile } from "node:fs/promises";
import { promisify } from "node:util";
import { lookupRelease } from "./release-api.mjs";

const exact = (pattern, value) => typeof value === "string" && pattern.exec(value)?.[0] === value;
const shaPattern = /^[0-9a-f]{40}$/;
const event = process.env.GITHUB_EVENT_NAME;
const ref = process.env.GITHUB_REF;
const tag = process.env.SOURCE_TAG ?? "";
let revision = process.env.GITHUB_SHA;
if (!exact(shaPattern, revision)) throw new Error("Invalid workflow revision");

if (event === "workflow_dispatch") {
  if (ref !== "refs/heads/main") throw new Error("Pages release dispatch must run from main");
  if (!exact(/^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/, tag) || tag.length > 100) {
    throw new Error("Pages source_tag must be a canonical stable vX.Y.Z tag");
  }
  const repository = process.env.GITHUB_REPOSITORY;
  const release = await lookupRelease(repository, tag);
  if (release.draft || release.prerelease) throw new Error("Pages source_tag must identify a published stable release");

  const execute = promisify(execFile);
  const deadline = performance.now() + 60_000;
  const get = async (path) => {
    const remaining = Math.floor(deadline - performance.now());
    if (remaining <= 0) throw new Error("Pages tag resolution exceeded its deadline");
    const response = await execute("gh", ["api", "--method", "GET", "-H", "X-GitHub-Api-Version: 2022-11-28", path], {
      timeout: Math.min(15_000, remaining), maxBuffer: 2 * 1024 * 1024,
    });
    return JSON.parse(response.stdout);
  };
  const reference = await get(`repos/${repository}/git/ref/tags/${tag}`);
  if (!reference || reference.ref !== `refs/tags/${tag}`) throw new Error("Pages source tag reference differs");
  let target = reference.object;
  const seen = new Set();
  while (true) {
    if (!target || !exact(shaPattern, target.sha)) throw new Error("Invalid Pages source tag object");
    if (target.type === "commit") {
      revision = target.sha;
      break;
    }
    if (target.type !== "tag" || seen.has(target.sha) || seen.size >= 8) {
      throw new Error("Pages source tag chain is invalid or exceeds its limit");
    }
    seen.add(target.sha);
    const annotated = await get(`repos/${repository}/git/tags/${target.sha}`);
    if (!annotated || annotated.sha !== target.sha) throw new Error("Annotated Pages tag identity differs");
    target = annotated.object;
  }
} else if ((event === "push" && ref === "refs/heads/main") || event === "pull_request") {
  if (tag) throw new Error("Pages source_tag is only supported on main workflow dispatch");
} else {
  throw new Error("Unsupported Pages workflow event or ref");
}

// Outputs contain validated literal values only, after every source check succeeds.
await appendFile(process.env.GITHUB_OUTPUT, `revision=${revision}\n`);
console.log(`Website source: ${tag || ref} at ${revision}`);
