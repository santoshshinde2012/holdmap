// Check the HTML a crawler receives, without running client scripts. Reuse the desktop's
// locked DOM parser, as the site already requires its dependencies to build the sample demo.
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { JSDOM } from "../../apps/desktop/node_modules/jsdom/lib/api.js";
import { serializeJsonLd } from "../src/lib/seo.mjs";

const dist = fileURLToPath(new URL("../dist/", import.meta.url));
const siteUrl = "https://santoshshinde2012.github.io/holdmap/";
const utilityPages = new Set(["404.html", "demo/index.html"]);
const canonicalUrls = new Set();
const titles = new Set();
const descriptions = new Set();
const pages = [];
function walk(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const file = join(directory, entry.name);
    if (entry.isDirectory()) walk(file);
    else if (entry.name.endsWith(".html")) pages.push(file);
  }
}
walk(dist);

const image = readFileSync(join(dist, "og.png"));
assert.equal(image.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "Social image must be a PNG");
const imageWidth = image.readUInt32BE(16);
const imageHeight = image.readUInt32BE(20);

for (const file of pages) {
  const path = relative(dist, file).replaceAll("\\", "/");
  const document = new JSDOM(readFileSync(file, "utf8")).window.document;
  const one = (selector) => {
    const nodes = document.querySelectorAll(selector);
    assert.equal(nodes.length, 1, `${path}: expected one ${selector}`);
    return nodes[0];
  };
  const content = (selector) => {
    const value = one(selector).getAttribute("content");
    assert.ok(value?.trim(), `${path}: empty ${selector}`);
    return value;
  };
  const robots = [...document.querySelectorAll('meta[name="robots"]')].map((tag) => tag.content).join(",");
  if (utilityPages.has(path)) {
    assert.match(robots, /\bnoindex\b/, `${path}: utility page must remain noindex`);
    assert.equal(document.querySelectorAll('link[rel="canonical"], script[type="application/ld+json"]').length, 0, `${path}: utility page must not claim an indexable page identity`);
    continue;
  }

  assert.ok(path === "index.html" || path.startsWith("docs/"), `${path}: explicitly review new public routes for SEO`);
  assert.doesNotMatch(robots, /\b(noindex|none|nofollow)\b/, `${path}: public page blocks indexing or navigation`);
  assert.equal(content('meta[name="robots"]'), "max-image-preview:large", `${path}: permit large search image previews`);
  assert.equal(document.documentElement.lang, "en", `${path}: document language`);
  assert.equal(document.querySelectorAll("main h1").length, 1, `${path}: one crawlable main heading`);
  assert.ok(document.querySelector("main h1").textContent.trim(), `${path}: empty main heading`);
  const title = one("head title").textContent.trim();
  const description = content('meta[name="description"]');
  assert.ok(title.includes("holdmap"), `${path}: title lacks the product name`);
  assert.ok(description.length >= 40, `${path}: description must summarize this page`);
  assert.ok(!titles.has(title), `${path}: duplicate page title`);
  assert.ok(!descriptions.has(description), `${path}: duplicate description`);
  titles.add(title);
  descriptions.add(description);

  const canonical = one('link[rel="canonical"]').getAttribute("href");
  const expected = new URL(path.replace(/index\.html$/, ""), siteUrl).href;
  assert.equal(canonical, expected, `${path}: canonical must retain the production origin, /holdmap/ base and trailing slash`);
  assert.ok(!canonicalUrls.has(canonical), `${path}: duplicate canonical`);
  canonicalUrls.add(canonical);
  assert.equal(one('link[rel="sitemap"]').getAttribute("href"), `${siteUrl}sitemap.xml`);
  assert.equal(content('meta[property="og:url"]'), canonical);
  assert.equal(content('meta[property="og:title"]'), title);
  assert.equal(content('meta[property="og:description"]'), description);
  assert.equal(content('meta[property="og:type"]'), path === "index.html" ? "website" : "article");
  assert.equal(content('meta[property="og:site_name"]'), "holdmap");
  assert.equal(content('meta[property="og:image"]'), `${siteUrl}og.png`);
  assert.equal(content('meta[property="og:image:type"]'), "image/png");
  assert.equal(Number(content('meta[property="og:image:width"]')), imageWidth);
  assert.equal(Number(content('meta[property="og:image:height"]')), imageHeight);
  assert.equal(content('meta[name="twitter:card"]'), "summary_large_image");
  assert.equal(content('meta[name="twitter:title"]'), title);
  assert.equal(content('meta[name="twitter:description"]'), description);
  assert.equal(content('meta[name="twitter:image"]'), `${siteUrl}og.png`);
  assert.equal(content('meta[name="twitter:image:alt"]'), content('meta[property="og:image:alt"]'));

  const rawSchema = one('script[type="application/ld+json"]').textContent;
  assert.ok(!rawSchema.includes("<"), `${path}: JSON-LD must escape HTML delimiters`);
  const schema = JSON.parse(rawSchema);
  assert.equal(schema["@context"], "https://schema.org");
  const page = schema["@graph"].find((node) => node["@type"] === "WebPage");
  assert.equal(page.url, canonical);
  assert.equal(page["@id"], `${canonical}#webpage`);
  assert.equal(page.name, title);
  assert.equal(page.description, description);
  assert.equal(page.isPartOf["@id"], `${siteUrl}#website`);
  assert.equal(page.about["@id"], `${siteUrl}#software`);
  if (path === "index.html") {
    const website = schema["@graph"].find((node) => node["@type"] === "WebSite");
    const software = schema["@graph"].find((node) => node["@type"] === "SoftwareApplication");
    assert.equal(website.url, siteUrl);
    assert.equal(software.url, siteUrl);
    assert.equal(software.name, "holdmap");
    assert.equal(software.isAccessibleForFree, true);
    assert.deepEqual(software.operatingSystem, ["macOS", "Linux", "Windows"]);
    assert.ok(!software.aggregateRating && !software.review && !software.softwareVersion, "Do not invent reviews or imply development features are in a published release");
  } else {
    const crumbs = [...one('nav[aria-label="Breadcrumb"]').querySelectorAll("li")];
    const breadcrumb = schema["@graph"].find((node) => node["@type"] === "BreadcrumbList");
    assert.equal(page.breadcrumb["@id"], `${canonical}#breadcrumbs`);
    assert.equal(breadcrumb.itemListElement.length, crumbs.length);
    assert.ok(crumbs.length >= 2, `${path}: breadcrumb must expose parent navigation`);
    crumbs.forEach((crumb, index) => {
      const item = breadcrumb.itemListElement[index];
      assert.equal(item.position, index + 1);
      assert.equal(item.name, crumb.textContent.trim());
      const link = crumb.querySelector("a");
      assert.equal(item.item, link ? new URL(link.getAttribute("href"), canonical).href : canonical);
    });
  }
}
for (const utility of utilityPages) assert.ok(pages.includes(join(dist, utility)), `Missing ${utility}`);

const sitemap = new JSDOM(readFileSync(join(dist, "sitemap.xml"), "utf8"), { contentType: "application/xml" }).window.document;
assert.equal(sitemap.documentElement.namespaceURI, "http://www.sitemaps.org/schemas/sitemap/0.9");
const locations = [...sitemap.querySelectorAll("url > loc")].map((node) => node.textContent);
assert.equal(new Set(locations).size, locations.length, "Sitemap contains duplicate URLs");
assert.deepEqual([...locations].sort(), [...canonicalUrls].sort(), "Sitemap must contain every public canonical and exclude demo/404");

// Exercise the actual serializer with hostile text: the JSON must survive parsing while
// an HTML parser must see a single inert script rather than the injected closing tag.
const hostile = { text: '</ScRiPt><script>alert("x")</script><!-- & "', nested: { text: "<tag>" } };
const serialized = serializeJsonLd(hostile);
const fixture = new JSDOM(`<script type="application/ld+json">${serialized}</script><p id="after">after</p>`).window.document;
assert.equal(fixture.querySelectorAll("script").length, 1);
assert.deepEqual(JSON.parse(fixture.querySelector("script").textContent), hostile);
assert.equal(fixture.querySelector("#after").textContent, "after");
console.log(`✔ SEO: ${canonicalUrls.size} public pages, ${utilityPages.size} noindex pages, canonical/social/schema/sitemap consistency and safe JSON-LD`);
