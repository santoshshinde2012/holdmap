import { describe, expect, it } from "vitest";
import specs from "./fixtures/location-specs.json";
import { claudePlaces, cursorPlaces } from "./mock-places";

// location-specs.json is generated from the Rust registry (holdmap-core demo_location_fixture).
const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const matches = (path: string, relative: string) =>
  new RegExp(`${relative.startsWith("/") ? "^" : "/"}${escape(relative).replace("\\{project\\}", "[^/]+")}$`).test(path);

describe("demo locations follow the Rust registry", () => {
  for (const [product, places] of [["cursor", cursorPlaces("/Users/dev/code/shop-web")], ["claude-code", claudePlaces("/Users/dev/code/shop-api")]] as const) {
    it(`${product}: every sample path is a documented location with the same layer and combine rule`, () => {
      const filed = places.filter((place) => place.path);
      expect(filed.length).toBeGreaterThan(5);
      for (const place of filed) {
        const rows = specs.filter((row) => row.product === product && row.kind === place.kind && matches(place.path, row.relative));
        const layers = rows.map((row) => `${row.scope}/${row.combine}`);
        expect(layers, `${product} ${place.kind} ${place.path}`).toContain(`${place.scope}/${place.combine}`);
      }
    });
  }
});
