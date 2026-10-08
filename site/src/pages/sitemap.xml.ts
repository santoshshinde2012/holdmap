import type { APIRoute } from "astro";
import { getCollection } from "astro:content";

export const GET: APIRoute = async ({ site }) => {
  const base = new URL(import.meta.env.BASE_URL, site);
  const docs = await getCollection("docs");
  const paths = ["", "docs/cli/", ...docs.map((d) => (d.id === "index" ? "docs/" : `docs/${d.id}/`))];
  const urls = paths.map((p) => `  <url><loc>${new URL(p, base).href}</loc></url>`).join("\n");
  return new Response(`<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`, {
    headers: { "content-type": "application/xml" },
  });
};
