// Plain-text helpers for core-generated prose (explanations, plan reasons).

export interface Span { code: boolean; text: string }

/** Split `text` on backtick-quoted segments so commands render as inline code. */
export function codeSpans(text: string): Span[] {
  const out: Span[] = [];
  const re = /`([^`]+)`/g;
  let last = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    if (m.index > last) out.push({ code: false, text: text.slice(last, m.index) });
    out.push({ code: true, text: m[1] });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ code: false, text: text.slice(last) });
  return out;
}
