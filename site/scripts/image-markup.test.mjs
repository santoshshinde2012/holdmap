import assert from "node:assert/strict";
import { test } from "node:test";
import { imageMarkupProblems } from "./image-markup.mjs";

test("actual unprocessed Astro image nodes are rejected before publication", () => {
  assert.deepEqual(imageMarkupProblems('<img __ASTRO_IMAGE_="fixture">'), [
    "image 1: unresolved Astro image placeholder",
    "image 1: missing image source",
    "image 1: missing alt attribute",
  ]);
  for (const source of ["src", "data-src", "srcset", "data-srcset"]) {
    assert.deepEqual(imageMarkupProblems(`<img ${source}="__ASTRO_IMAGE_fixture" alt="Fixture">`), [
      "image 1: unresolved Astro image placeholder",
    ]);
  }
});

test("missing and explicitly empty image sources fail", () => {
  assert.deepEqual(imageMarkupProblems('<img alt="Fixture">'), ["image 1: missing image source"]);
  for (const source of ["", " \t\n"]) {
    assert.deepEqual(imageMarkupProblems(`<img src="${source}" alt="Fixture">`), ["image 1: empty src attribute"]);
  }
  assert.deepEqual(imageMarkupProblems('<img src="" data-src="fixture.webp" alt="Fixture">'), [
    "image 1: empty src attribute",
  ]);
});

test("alt must be present while explicit decorative alt remains valid", () => {
  assert.deepEqual(imageMarkupProblems('<img src="fixture.webp">'), ["image 1: missing alt attribute"]);
  assert.deepEqual(imageMarkupProblems('<img src="fixture.webp" alt="">'), []);
  assert.deepEqual(imageMarkupProblems('<img src="fixture.webp" alt="Description">'), []);
});

test("real lazy, responsive and picture image sources are accepted", () => {
  for (const markup of [
    '<img data-src="fixture.webp" alt="Fixture">',
    '<img srcset="fixture.webp 1x, fixture-large.webp 2x" alt="Fixture">',
    '<img data-srcset="fixture.webp 400w, fixture-large.webp 800w" alt="Fixture">',
    '<picture><source srcset="fixture.avif" type="image/avif"><img alt="Fixture"></picture>',
    '<picture><source data-srcset="fixture.webp"><img alt="Fixture"></picture>',
  ]) assert.deepEqual(imageMarkupProblems(markup), [], markup);
  assert.deepEqual(imageMarkupProblems('<img data-src=" " srcset=" " alt="Fixture">'), [
    "image 1: missing image source",
  ]);
  for (const invalid of [
    '<picture><source src="fixture.webp"><img alt="Fixture"></picture>',
    '<picture><img alt="Fixture"><source srcset="fixture.webp"></picture>',
  ]) assert.deepEqual(imageMarkupProblems(invalid), ["image 1: missing image source"]);
  assert.deepEqual(imageMarkupProblems('<picture><source srcset="__ASTRO_IMAGE_fixture"><img alt="Fixture"></picture>'), [
    "image 1: unresolved Astro image placeholder",
  ]);
});

test("escaped examples, comments and script text are not image elements", () => {
  assert.deepEqual(imageMarkupProblems(`
    <pre><code>&lt;img __ASTRO_IMAGE_="fixture"&gt;</code></pre>
    <!-- <img __ASTRO_IMAGE_="fixture"> -->
    <script type="application/json">{"example":"<img __ASTRO_IMAGE_='fixture'>"}</script>
    <img src="fixture.webp" alt="An __ASTRO_IMAGE_ code example">
  `), []);
});

test("parsed markup covers uppercase, single-quoted and unquoted attributes", () => {
  assert.deepEqual(imageMarkupProblems("<IMG SRC='fixture.webp' ALT='Fixture'>"), []);
  assert.deepEqual(imageMarkupProblems("<img src=fixture.webp alt=Fixture>"), []);
  assert.deepEqual(imageMarkupProblems("<IMG __ASTRO_IMAGE_fixture='metadata' ALT='Fixture'>"), [
    "image 1: unresolved Astro image placeholder",
    "image 1: missing image source",
  ]);
});
