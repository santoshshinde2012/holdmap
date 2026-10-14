/**
 * Resolve download URLs from a published stable release, never an assembling release.
 * @param {{ sourceVersion: string, release: unknown, repositoryUrl: string, requireRelease?: boolean, offline?: boolean }} options
 */
export function publishedReleaseVersion({ sourceVersion, release, repositoryUrl, requireRelease = false, offline = false }) {
  if (requireRelease && offline) {
    throw new Error("HOLDMAP_SITE_REQUIRE_RELEASE=1 cannot be combined with HOLDMAP_SITE_OFFLINE=1. Publishing requires a verified published release.");
  }
  const info = release && typeof release === "object" && !Array.isArray(release)
    ? /** @type {Record<string, unknown>} */ (release) : null;
  const tag = typeof info?.tag_name === "string" ? info.tag_name : "";
  const stable = tag.trim() === tag ? /^v((?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*))$/.exec(tag) : null;
  const published = stable && info?.draft === false && info?.prerelease === false
    && info?.html_url === `${repositoryUrl}/releases/tag/${tag}`;
  if (!offline && published) return stable[1];
  if (requireRelease) {
    throw new Error("Cannot build the published website: GitHub's latest stable release is unavailable or invalid. Retry after a stable release is published; version.txt is not a publishing fallback.");
  }
  return sourceVersion.trim();
}
