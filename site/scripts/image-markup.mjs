// Inspect parsed image elements rather than matching source text: documentation examples
// and comments may legitimately mention Astro's reserved image placeholder.
import { JSDOM } from "../../apps/desktop/node_modules/jsdom/lib/api.js";

const sourceAttributes = ["src", "data-src", "srcset", "data-srcset"];
const pictureSourceAttributes = ["srcset", "data-srcset"];
const placeholder = (value) => value.toUpperCase().includes("__ASTRO_IMAGE_");
const hasSource = (element, attributes = sourceAttributes) => attributes.some((name) => element.getAttribute(name)?.trim());

export function imageMarkupProblems(html) {
  const dom = new JSDOM(html);
  try {
    const problems = [];
    for (const [index, image] of [...dom.window.document.querySelectorAll("img")].entries()) {
      const label = `image ${index + 1}`;
      const siblings = [...(image.parentElement?.children ?? [])];
      const pictureSources = image.parentElement?.localName === "picture"
        ? siblings.slice(0, siblings.indexOf(image)).filter((element) => element.localName === "source")
        : [];
      if ([image, ...pictureSources].some((element) => [...element.attributes].some((attribute) =>
        placeholder(attribute.name) || (sourceAttributes.includes(attribute.name) && placeholder(attribute.value))
      ))) {
        problems.push(`${label}: unresolved Astro image placeholder`);
      }
      if (image.hasAttribute("src") && !image.getAttribute("src").trim()) {
        problems.push(`${label}: empty src attribute`);
      } else if (!hasSource(image) && !pictureSources.some((element) => hasSource(element, pictureSourceAttributes))) {
        // A source may be supplied by srcset/picture or by the site's explicit lazy loader.
        problems.push(`${label}: missing image source`);
      }
      if (!image.hasAttribute("alt")) problems.push(`${label}: missing alt attribute`);
    }
    return problems;
  } finally {
    dom.window.close();
  }
}
