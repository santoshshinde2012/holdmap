/** Serialize JSON-LD for an HTML script element without allowing a closing script tag.
 * @param {object} value
 */
export function serializeJsonLd(value) {
  return JSON.stringify(value).replaceAll("<", "\\u003c");
}
