import assert from "node:assert/strict";
import { test } from "node:test";
import { publishedReleaseVersion } from "../src/lib/published-release.mjs";

const repositoryUrl = "https://github.com/santoshshinde2012/holdmap";
const stable = (tag = "v0.6.0") => ({ tag_name: tag, draft: false, prerelease: false, html_url: `${repositoryUrl}/releases/tag/${tag}` });
const resolve = (release, options = {}) => publishedReleaseVersion({ sourceVersion: "0.7.0", repositoryUrl, release, requireRelease: true, ...options });

test("publishing uses the published stable release while a newer source version assembles", () => {
  assert.equal(resolve(stable()), "0.6.0");
  assert.equal(resolve(stable("v0.7.0")), "0.7.0");
});

test("publishing fails when GitHub latest is unavailable or malformed", () => {
  for (const release of [null, undefined, [], {}, "v0.6.0", { tag_name: "v0.6.0" }]) {
    assert.throws(() => resolve(release), /latest stable release is unavailable or invalid/);
  }
});

test("publishing rejects draft and prerelease metadata even with a stable-looking tag", () => {
  for (const release of [{ ...stable(), draft: true }, { ...stable(), prerelease: true }]) {
    assert.throws(() => resolve(release), /latest stable release is unavailable or invalid/);
  }
});

test("publishing requires canonical stable tags that match constructed download URLs", () => {
  for (const tag of ["0.6.0", "v0.6", "v00.6.0", "v0.6.0-rc.1", "v0.6.0+build.1", " v0.6.0", "v0.6.0\n"]) {
    assert.throws(() => resolve(stable(tag)), /latest stable release is unavailable or invalid/, tag);
  }
});

test("publishing rejects a release URL that disagrees with its repository or tag", () => {
  for (const html_url of [undefined, `${repositoryUrl}/releases/tag/v0.7.0`, "https://github.com/another/repo/releases/tag/v0.6.0"]) {
    assert.throws(() => resolve({ ...stable(), html_url }), /latest stable release is unavailable or invalid/);
  }
});

test("explicit offline builds use a deterministic source fallback", () => {
  assert.equal(resolve(null, { offline: true, requireRelease: false }), "0.7.0");
  assert.equal(resolve(stable(), { offline: true, requireRelease: false }), "0.7.0");
});

test("offline cannot bypass the publishing requirement", () => {
  for (const release of [null, stable()]) {
    assert.throws(() => resolve(release, { offline: true }), /REQUIRE_RELEASE=1 cannot be combined with HOLDMAP_SITE_OFFLINE=1/);
  }
});

test("optional local builds retain fallback behavior and prefer a valid published release", () => {
  assert.equal(resolve(null, { requireRelease: false }), "0.7.0");
  assert.equal(resolve(stable(), { requireRelease: false }), "0.6.0");
});
