//! Plain text from the markdown and HTML scanners put in SARIF `help` text, so a fix reads as a
//! sentence instead of `**`, `<details>` and link URLs (which the redactor would mask anyway).

/// Strip markdown emphasis, headings, quotes, tables, code ticks, HTML tags (dropping
/// `<summary>` labels) and link targets, keeping the words.
pub(crate) fn plain(raw: &str) -> String {
    let no_summary = drop_between(raw, "<summary>", "</summary>");
    let no_tags = drop_tags(&no_summary);
    let no_links = link_labels(&no_tags);
    let mut lines = Vec::new();
    for line in no_links.lines() {
        let line = line.trim().trim_start_matches(['#', '>']).trim();
        if line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
            continue; // blank or a table rule
        }
        lines.push(
            line.replace(['|', '`'], " ")
                .replace("**", "")
                .replace("__", ""),
        );
    }
    let joined = lines.join(" ");
    let words: Vec<&str> = joined.split_whitespace().collect();
    words
        .join(" ")
        .replace(", )", ")")
        .replace("( ", "(")
        .replace(" )", ")")
        .replace(" .", ".")
        .replace(" ,", ",")
}

fn drop_between(s: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(open) {
        out.push_str(&rest[..i]);
        match rest[i..].find(close) {
            Some(j) => rest = &rest[i + j + close.len()..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn drop_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        // A tag starts with `<` then a letter or `/`; `a < b` stays.
        if c == '<'
            && chars
                .peek()
                .is_some_and(|n| n.is_ascii_alphabetic() || *n == '/')
        {
            for t in chars.by_ref() {
                if t == '>' {
                    break;
                }
            }
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// `[label](target)` becomes `label`.
fn link_labels(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let link = after.find("](").and_then(|close| {
            let target = &after[close + 2..];
            target.find(')').map(|end| (close, close + 2 + end + 1))
        });
        match link {
            Some((close, consumed)) if !after[..close].contains('[') => {
                out.push_str(&rest[..open]);
                out.push_str(&after[..close]);
                rest = &after[consumed..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn osv_help_markdown_becomes_a_readable_sentence() {
        let md = "**Your dependency is vulnerable to [CVE-2019-10906](https://osv.dev/CVE-2019-10906)**\n(Also published as: [PYSEC-2019-217](https://osv.dev/PYSEC-2019-217), [GHSA-462w-v97r-4m45](https://osv.dev/GHSA-462w-v97r-4m45), ).\n\n## [PYSEC-2019-217](https://osv.dev/PYSEC-2019-217)\n\n<details>\n<summary>Details</summary>\n\n> In Pallets Jinja before 2.10.1, `str.format_map` allows a sandbox escape.\n\n</details>";
        assert_eq!(
            plain(md),
            "Your dependency is vulnerable to CVE-2019-10906 (Also published as: PYSEC-2019-217, \
             GHSA-462w-v97r-4m45). PYSEC-2019-217 In Pallets Jinja before 2.10.1, str.format_map \
             allows a sandbox escape."
        );
    }

    #[test]
    fn tables_comparisons_and_plain_text_survive() {
        assert_eq!(
            plain("**Secret**\n| Severity | Match |\n| --- | --- |\n|CRITICAL|x|"),
            "Secret Severity Match CRITICAL x"
        );
        assert_eq!(
            plain("Use a < b and [not a link]."),
            "Use a < b and [not a link]."
        );
        assert_eq!(plain("Upgrade to 2.0."), "Upgrade to 2.0.");
    }
}
