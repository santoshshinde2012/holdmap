// Pure helpers for the add-analyzer flow: inline validation of a draft, the step model, how a
// tool's annotations are shown (server hints, never trusted) and hidden characters in tool text.

export type AddSource = "preset" | "command" | "url" | "agents" | "manifest";
export type AddStep = "source" | "tools" | "review";
export const ADD_STEPS: { id: AddStep; label: string }[] = [
  { id: "source", label: "Source" }, { id: "tools", label: "Tools" }, { id: "review", label: "Review" },
];

/** Same rule as the manifest parser: reverse-DNS, lowercase letters, digits, dashes, ≤ 64. */
export function idError(id: string): string | null {
  const v = id.trim();
  if (!v) return "Required.";
  if (v.length > 64) return "64 characters at most.";
  const parts = v.split(".");
  if (parts.length < 2 || parts.some((p) => !p || !/^[a-z0-9-]+$/.test(p))) return "Use reverse-DNS: lowercase letters, digits and dashes, like local.my-scanner.";
  return null;
}

/** "My Scanner!" → "local.my-scanner". */
export function idFromName(name: string): string {
  const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 56);
  return slug ? `local.${slug}` : "local.";
}

export function isLocalHost(host: string): boolean {
  const h = host.replace(/^\[|\]$/g, "");
  return h === "localhost" || h === "::1" || /^127\./.test(h);
}

export function urlError(url: string): string | null {
  const v = url.trim();
  if (!v) return "Required.";
  let u: URL;
  try { u = new URL(v); } catch { return "Not a valid URL."; }
  if (u.protocol !== "http:" && u.protocol !== "https:") return "Use http or https.";
  if (!isLocalHost(u.hostname) && u.protocol !== "https:") return "A server on another machine must use https.";
  return null;
}

export function keyNamesError(raw: string): string | null {
  const bad = raw.split(/[\s,]+/).filter(Boolean).find((k) => !/^[A-Za-z_][A-Za-z0-9_]*$/.test(k));
  return bad ? `"${bad}" isn't a valid key name (letters, digits and _).` : null;
}

export interface DraftFields { id: string; command: string; url: string; keys: string; manifest: string; serverPicked: boolean }
/** Field → message for what blocks "Next"; empty when the draft can be added. */
export function draftErrors(source: AddSource, f: DraftFields): Record<string, string> {
  const out: Record<string, string> = {};
  const put = (k: string, e: string | null) => { if (e) out[k] = e; };
  if (source === "manifest") {
    if (!f.manifest.trim()) out.manifest = "Paste a manifest.";
    else if (!/^\s*id\s*=/m.test(f.manifest)) out.manifest = "A manifest needs an id = \"…\" line.";
    return out;
  }
  if (source === "preset") return out;
  put("id", idError(f.id));
  if (source === "command") { if (!f.command.trim()) out.command = "Required."; put("keys", keyNamesError(f.keys)); }
  if (source === "url") put("url", urlError(f.url));
  if (source === "agents") { if (!f.serverPicked) out.server = "Pick a server."; put("keys", keyNamesError(f.keys)); }
  return out;
}

export interface Hint { id: string; label: string; tone: "ok" | "warn" | "plain"; tip: string }
/** Badges from MCP tool annotations. They are hints from the server, so the tips say so. */
export function toolHints(annotations: Record<string, unknown> | null | undefined): Hint[] {
  const a = annotations ?? {};
  const out: Hint[] = [];
  const says = "The server says so; holdmap doesn't rely on it.";
  if (a.readOnlyHint === true) out.push({ id: "ro", label: "Read-only", tone: "ok", tip: `Doesn't change anything. ${says}` });
  else if (a.destructiveHint === true) out.push({ id: "destructive", label: "May delete or overwrite", tone: "warn", tip: `May make destructive changes. ${says}` });
  if (a.openWorldHint === true) out.push({ id: "open", label: "Reaches outside", tone: "warn", tip: `Talks to external systems. ${says}` });
  else if (a.openWorldHint === false) out.push({ id: "closed", label: "Local only", tone: "plain", tip: `Works on a closed set of data. ${says}` });
  if (a.readOnlyHint !== true && a.idempotentHint === true) out.push({ id: "idem", label: "Idempotent", tone: "plain", tip: `Repeating a call has no extra effect. ${says}` });
  return out;
}

const HIDDEN = /[\u200B-\u200F\u202A-\u202E\u2060-\u2064\uFEFF]|<!--[\s\S]*?-->/g;
/** Zero-width / bidi characters and HTML comments: text a reader can't see but a model reads. */
export function hiddenText(text: string | null | undefined): number {
  return (text ?? "").match(HIDDEN)?.length ?? 0;
}
/** The same text with hidden characters made visible as ⟦U+XXXX⟧. */
export function revealHidden(text: string): string {
  return text.replace(/[\u200B-\u200F\u202A-\u202E\u2060-\u2064\uFEFF]/g, (c) => `⟦U+${c.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0")}⟧`);
}
