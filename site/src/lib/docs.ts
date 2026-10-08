import { getCollection, type CollectionEntry } from "astro:content";
import { repoUrl, url } from "./site";

export type DocLink = { title: string; href: string; id: string };
export type Heading = { depth: number; slug: string; text: string };

/** Headings the Markdown renderer collected for an entry. */
export const headingsOf = (entry: CollectionEntry<"docs"> | CollectionEntry<"reference">): Heading[] =>
  ((entry.rendered?.metadata as { headings?: Heading[] } | undefined)?.headings ?? []);

/** The guide in reading order, with the generated CLI reference last. */
export async function guide(): Promise<DocLink[]> {
  const docs = (await getCollection("docs")).sort((a, b) => a.data.order - b.data.order);
  return [
    ...docs.map((d) => ({ id: d.id, title: d.data.title, href: url(d.id === "index" ? "docs/" : `docs/${d.id}/`) })),
    { id: "cli", title: "CLI reference", href: url("docs/cli/") },
  ];
}

export const editUrl = (entry: CollectionEntry<"docs"> | CollectionEntry<"reference">) =>
  entry.collection === "reference"
    ? `${repoUrl}/blob/main/docs/cli.md`
    : `${repoUrl}/edit/main/site/src/content/docs/${entry.id}.md`;

/** Links in docs/cli.md are written for GitHub (../README.md); point them at the site or repo. */
export function fixRepoLinks(html: string): string {
  return html
    // Autolinked examples such as http://localhost:PORT are text, not links.
    .replace(/<a href="https?:\/\/localhost[^"]*">([^<]*)<\/a>/g, "<code>$1</code>")
    .replace(/href="\.\.\/README\.md"/g, `href="${url()}"`)
    .replace(/href="\.\.\/([^"#]+)"/g, `href="${repoUrl}/blob/main/$1"`)
    .replace(/href="(?!https?:|#|\/|mailto:)([^"]+\.md)(#[^"]*)?"/g, `href="${repoUrl}/blob/main/docs/$1$2"`);
}
