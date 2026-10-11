//! Printable, bounded search input. Pasted text never becomes application shortcuts.

pub(super) const MAX_SEARCH_BYTES: usize = 512;

pub(super) fn append_search_text(query: &mut String, text: &str) {
    // A terminal may supply one large paste event; bound both retained text and work.
    for ch in text.chars().take(MAX_SEARCH_BYTES * 4) {
        let ch = if ch.is_whitespace() {
            ' '
        } else if ch.is_control()
            || matches!(ch, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            continue;
        } else {
            ch
        };
        if ch == ' ' && query.ends_with(' ') {
            continue;
        }
        if query.len() + ch.len_utf8() > MAX_SEARCH_BYTES {
            break;
        }
        query.push(ch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiline_paste_becomes_printable_search_text_without_terminal_controls() {
        let mut query = "agent ".into();
        append_search_text(&mut query, "\r\n  codex\tservice\u{1b}\u{7}\u{202e}\nxy");
        assert_eq!(query, "agent codex service xy");
        assert!(query.chars().all(|ch| !ch.is_control()));
    }

    #[test]
    fn search_is_bounded_without_splitting_unicode() {
        let mut query = "a".repeat(MAX_SEARCH_BYTES - 4);
        append_search_text(&mut query, "界界\nxy");
        assert_eq!(query.len(), MAX_SEARCH_BYTES - 1);
        assert!(query.ends_with('界'));
        append_search_text(&mut query, "x".repeat(MAX_SEARCH_BYTES * 10).as_str());
        assert_eq!(query.len(), MAX_SEARCH_BYTES);
    }
}
