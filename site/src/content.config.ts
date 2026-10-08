import { defineCollection } from "astro:content";
import { glob } from "astro/loaders";
import { z } from "astro/zod";

// The guide pages live here; the CLI reference is the repo's docs/cli.md (generated from the
// CLI's own --help and checked in CI), rendered as-is so the site never drifts from it.
const docs = defineCollection({
  loader: glob({ pattern: "*.md", base: "./src/content/docs" }),
  schema: z.object({ title: z.string(), description: z.string(), order: z.number() }),
});

const reference = defineCollection({
  loader: glob({ pattern: "cli.md", base: "../docs" }),
});

export const collections = { docs, reference };
