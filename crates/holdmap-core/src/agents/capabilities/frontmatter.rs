//! Project only two scalar fields from a bounded YAML header. No alias expansion or includes.

use saphyr_parser::{Event, Parser};
use std::collections::HashSet;

pub(super) const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_EVENTS: usize = 256;
const MAX_DEPTH: usize = 8;
type Metadata = (Option<String>, Option<String>);

/// Parse only the strict opening/closing `---` header in this already bounded prefix.
/// Bytes following its delimiter are not decoded, parsed, retained or executed.
/// A final line without a newline is complete only when the bounded reader observed EOF.
pub(super) fn parse(prefix: &[u8], at_eof: bool) -> Option<Metadata> {
    if prefix.len() > MAX_HEADER_BYTES {
        return None;
    }
    let prefix = prefix.strip_prefix(b"\xef\xbb\xbf").unwrap_or(prefix);
    let mut lines = prefix.split_inclusive(|byte| *byte == b'\n');
    let first = lines.next()?;
    if line(first) != b"---" {
        return None;
    }
    let mut end = first.len();
    let mut header = None;
    for current in lines {
        if line(current) == b"---" && (current.ends_with(b"\n") || at_eof) {
            header = Some(std::str::from_utf8(&prefix[first.len()..end]).ok()?);
            break;
        }
        end += current.len();
    }
    let header = header?;
    let mut documents = 0;
    let mut depth = 0;
    let mut root = false;
    let mut complete = false;
    let mut key: Option<String> = None;
    let mut keys = HashSet::new();
    let mut name = None;
    let mut description = None;
    for (index, event) in Parser::new_from_str(header).enumerate() {
        if index >= MAX_EVENTS {
            return None;
        }
        match event.ok()?.0 {
            Event::StreamStart | Event::Nothing => {}
            Event::DocumentStart(_) => {
                documents += 1;
                if documents != 1 {
                    return None;
                }
            }
            Event::MappingStart(anchor, tag) => {
                if anchor != 0 || tag.is_some() || depth >= MAX_DEPTH {
                    return None;
                }
                if depth == 0 {
                    if root {
                        return None;
                    }
                    root = true;
                } else if depth == 1 && !skip_collection(&mut key) {
                    return None;
                }
                depth += 1;
            }
            Event::SequenceStart(anchor, tag) => {
                if anchor != 0 || tag.is_some() || depth == 0 || depth >= MAX_DEPTH {
                    return None;
                }
                if depth == 1 && !skip_collection(&mut key) {
                    return None;
                }
                depth += 1;
            }
            Event::MappingEnd | Event::SequenceEnd => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    complete = true;
                }
            }
            Event::Scalar(value, _, anchor, tag) => {
                if anchor != 0 || tag.is_some() || depth == 0 {
                    return None;
                }
                if depth == 1 {
                    if let Some(key) = key.take() {
                        match key.as_str() {
                            "name" => name = safe_name(&value),
                            "description" => description = safe_description(&value),
                            _ => {}
                        }
                    } else {
                        if value.len() > 128 || !keys.insert(value.to_string()) {
                            return None;
                        }
                        key = Some(value.into_owned());
                    }
                }
            }
            Event::Alias(_) => return None,
            Event::DocumentEnd => {
                if !complete || depth != 0 || key.is_some() {
                    return None;
                }
            }
            Event::StreamEnd => {
                return (root && complete && documents == 1 && key.is_none())
                    .then_some((name, description));
            }
        }
    }
    None
}

fn line(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn skip_collection(key: &mut Option<String>) -> bool {
    key.take()
        .is_some_and(|key| !matches!(key.as_str(), "name" | "description"))
}

fn safe_name(value: &str) -> Option<String> {
    (super::safe_name(value) && crate::redact::line(value) == value).then(|| value.to_owned())
}

pub(super) fn safe_description(value: &str) -> Option<String> {
    if value.chars().count() > 1024 || value.chars().any(|character| {
        character.is_control() && !matches!(character, '\n' | '\r' | '\t')
            || matches!(character, '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}')
    }) { return None; }
    let text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = text.to_ascii_lowercase();
    if text.is_empty()
        || text.contains("://")
        || text.contains(['<', '>'])
        || text.contains(crate::redact::MASK)
        || [
            "sk-",
            "ghp_",
            "github_pat_",
            "xoxb-",
            "xoxp-",
            "-----begin ",
        ]
        .iter()
        .any(|prefix| lower.contains(prefix))
        || crate::redact::line(&text) != text
    {
        return None;
    }
    if text.chars().count() > 300 {
        Some(text.chars().take(299).chain(std::iter::once('…')).collect())
    } else {
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_folding_quotes_and_literal_scalars_are_parsed_by_the_yaml_implementation() {
        for (yaml, expected) in [
            (
                "description: >-\n  Review code\n  before shipping.\n",
                "Review code before shipping.",
            ),
            (
                "description: |\n  Review code\n  before shipping.\n",
                "Review code before shipping.",
            ),
            (
                "description: \"Review: code # carefully\\nthen ship\"\n",
                "Review: code # carefully then ship",
            ),
            (
                "description: 'Review code: it''s useful'\n",
                "Review code: it's useful",
            ),
        ] {
            let input = format!("---\nname: review\n{yaml}---\nprivate-instruction-body-fixture");
            let metadata = parse(input.as_bytes(), true).unwrap();
            assert_eq!(metadata.0.as_deref(), Some("review"));
            assert_eq!(metadata.1.as_deref(), Some(expected));
        }
    }

    #[test]
    fn malformed_ambiguous_aliased_tagged_deep_and_unbounded_headers_are_omitted() {
        let deep = format!(
            "metadata: {}0{}\n",
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        );
        let events = (0..MAX_EVENTS)
            .map(|index| format!("field{index}: value\n"))
            .collect::<String>();
        for header in [
            "description: [one, two]\n",
            "name: {field: value}\n",
            "name: one\nname: two\n",
            "name: &name review\ndescription: *name\n",
            "name: !include private-fixture\n",
            "? [complex, key]\n: value\n",
            "description: \"unterminated\n",
            &deep,
            &events,
        ] {
            assert!(
                parse(
                    format!("---\n{header}---\nprivate-body-fixture").as_bytes(),
                    true
                )
                .is_none(),
                "unsafe YAML shape must omit frontmatter metadata"
            );
        }
        assert!(parse(b"# Heading\nprivate-body-fixture", true).is_none());
        assert!(parse(b"---\nname: review\nprivate-body-fixture", true).is_none());
        assert!(parse(&vec![b'x'; MAX_HEADER_BYTES + 1], true).is_none());
    }

    #[test]
    fn header_only_decoding_preserves_crlf_bom_and_never_uses_body_metadata() {
        let mut input = b"\xef\xbb\xbf---\r\nname: review\r\nmetadata: {category: code}\r\ndescription: Review code\r\n---\r\n".to_vec();
        input.extend_from_slice(b"description: private-body-fixture\n\xff\xfe");
        let metadata = parse(&input, true).unwrap();
        assert_eq!(metadata.0.as_deref(), Some("review"));
        assert_eq!(metadata.1.as_deref(), Some("Review code"));
        let no_metadata = parse(
            b"---\nmetadata: {name: private-nested-fixture}\n---\nname: private-body-fixture",
            true,
        )
        .unwrap();
        assert_eq!(no_metadata, (None, None));
    }

    #[test]
    fn closing_delimiters_must_be_complete_at_the_prefix_boundary() {
        let eof = b"---\nname: review\ndescription: Review code\n---";
        assert!(parse(eof, true).is_some());
        assert!(parse(eof, false).is_none());
        for bom in [false, true] {
            let mut input = if bom {
                b"\xef\xbb\xbf".to_vec()
            } else {
                Vec::new()
            };
            input.extend_from_slice(b"---\nname: review\ndescription: Review code\n#");
            input.resize(MAX_HEADER_BYTES - 4, b'x');
            input.extend_from_slice(b"\n---not-a-delimiter\n");
            assert_eq!(&input[MAX_HEADER_BYTES - 3..MAX_HEADER_BYTES], b"---");
            assert!(parse(&input[..MAX_HEADER_BYTES], false).is_none());

            // A complete delimiter with its newline at the exact limit remains usable.
            input.truncate(MAX_HEADER_BYTES - 5);
            input.extend_from_slice(b"\n---\n");
            assert_eq!(input.len(), MAX_HEADER_BYTES);
            let metadata = parse(&input, false).unwrap();
            assert_eq!(metadata.0.as_deref(), Some("review"));
            assert_eq!(metadata.1.as_deref(), Some("Review code"));
        }
    }

    #[test]
    fn descriptions_are_one_line_bounded_and_drop_unsafe_or_recognized_secret_text() {
        for value in [
            "API_TOKEN=private-credential-fixture",
            "Bearer private-bearer-fixture",
            "https://example.invalid/?token=private-url-fixture",
            "sk-private-token-fixture",
            "<script>private-fixture</script>",
            "private\u{1b}[2Jfixture",
            "private\u{202e}fixture",
        ] {
            assert!(
                safe_description(value).is_none(),
                "unsafe metadata must be omitted rather than displayed or clipped"
            );
        }
        assert!(safe_description(&format!(
            "{} API_TOKEN=private-late-fixture",
            "word ".repeat(70)
        ))
        .is_none());
        let description = safe_description(&"word ".repeat(100)).unwrap();
        assert_eq!(description.chars().count(), 300);
        assert!(description.ends_with('…'));
        assert_eq!(
            safe_description("  Review\n\tcode  ").as_deref(),
            Some("Review code")
        );
    }
}
