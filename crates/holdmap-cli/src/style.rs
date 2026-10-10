//! Terminal styling (ANSI) that honours `--color`, `NO_COLOR`, `CLICOLOR_FORCE` and TTY detection,
//! plus a compact table renderer that fits the terminal width.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use unicode_width::UnicodeWidthStr;

static COLOR: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

pub fn init(choice: ColorChoice) {
    let enabled = match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
                false
            } else if std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0") {
                true
            } else {
                std::io::stdout().is_terminal()
                    && std::env::var("TERM").map_or(true, |t| t != "dumb")
            }
        }
    };
    COLOR.store(enabled, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    COLOR.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum S {
    Plain,
    Bold,
    Dim,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    BoldRed,
    BoldGreen,
    BoldYellow,
    BoldCyan,
}

impl S {
    fn code(self) -> &'static str {
        match self {
            S::Plain => "",
            S::Bold => "1",
            S::Dim => "2",
            S::Green => "32",
            S::Yellow => "33",
            S::Blue => "34",
            S::Magenta => "35",
            S::Cyan => "36",
            S::BoldRed => "1;31",
            S::BoldGreen => "1;32",
            S::BoldYellow => "1;33",
            S::BoldCyan => "1;36",
        }
    }
}

pub fn paint(s: impl AsRef<str>, style: S) -> String {
    let s = terminal_text(s.as_ref());
    if !enabled() || style == S::Plain || s.is_empty() {
        s
    } else {
        format!("\x1b[{}m{s}\x1b[0m", style.code())
    }
}

/// Protect a composed human-readable render while retaining its line breaks and the
/// small set of SGR styles Holdmap emits. Other terminal controls are rendered inert.
/// Individual fields should use core `printable` first so embedded newlines cannot add rows.
pub fn terminal_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(after_escape) = rest.strip_prefix("\x1b[") {
            if let Some(end) = after_escape.bytes().take(5).position(|byte| byte == b'm') {
                if matches!(
                    &after_escape[..end],
                    "0" | "1"
                        | "2"
                        | "32"
                        | "33"
                        | "34"
                        | "35"
                        | "36"
                        | "1;31"
                        | "1;32"
                        | "1;33"
                        | "1;36"
                ) {
                    let length = end + 3;
                    out.push_str(&rest[..length]);
                    rest = &rest[length..];
                    continue;
                }
            }
        }
        let ch = rest.chars().next().expect("nonempty text");
        match ch {
            '\n' => out.push(ch),
            '\r' | '\t' => out.push(' '),
            ch if ch.is_control() => out.push('\u{FFFD}'),
            ch => out.push(ch),
        }
        rest = &rest[ch.len_utf8()..];
    }
    out
}

pub fn bold(s: impl AsRef<str>) -> String {
    paint(s, S::Bold)
}
pub fn dim(s: impl AsRef<str>) -> String {
    paint(s, S::Dim)
}

/// Status glyphs with ASCII fallbacks.
pub fn ok_mark() -> String {
    paint("✔", S::BoldGreen)
}
pub fn err_mark() -> String {
    paint("✖", S::BoldRed)
}
pub fn warn_mark() -> String {
    paint("!", S::BoldYellow)
}
pub fn arrow() -> String {
    paint("→", S::Cyan)
}

pub fn term_width() -> usize {
    if let Ok(c) = std::env::var("COLUMNS") {
        if let Ok(n) = c.parse::<usize>() {
            return n.max(40);
        }
    }
    if std::io::stdout().is_terminal() {
        if let Ok((w, _)) = ratatui::crossterm::terminal::size() {
            return (w as usize).max(40);
        }
    }
    140
}

pub fn width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Collapse newlines and runs of spaces, then truncate: for multi-line commands in one row.
pub fn one_line(s: &str, max: usize) -> String {
    truncate(&s.split_whitespace().collect::<Vec<_>>().join(" "), max)
}

pub fn truncate(s: &str, max: usize) -> String {
    if width(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw + 1 > max {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

/// A table cell: text plus style.
#[derive(Debug, Clone)]
pub struct Cell(pub String, pub S);

impl Cell {
    pub fn new(s: impl Into<String>, st: S) -> Self {
        Cell(holdmap_core::util::printable(&s.into()).into_owned(), st)
    }
}

/// Column definition.
pub struct Col {
    pub title: &'static str,
    pub right: bool,
    /// Flexible columns absorb truncation when the terminal is narrow.
    pub flex: bool,
    pub min: usize,
}

pub fn col(title: &'static str) -> Col {
    Col {
        title,
        right: false,
        flex: false,
        min: title.len(),
    }
}
pub fn rcol(title: &'static str) -> Col {
    Col {
        title,
        right: true,
        flex: false,
        min: title.len(),
    }
}
pub fn flex(title: &'static str, min: usize) -> Col {
    Col {
        title,
        right: false,
        flex: true,
        min,
    }
}

/// Render rows as a borderless, aligned table that fits `max_width`.
pub fn table(cols: &[Col], rows: &[Vec<Cell>], max_width: usize) -> String {
    let gap = 2;
    let mut widths: Vec<usize> = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            rows.iter()
                .map(|r| width(&r[i].0))
                .max()
                .unwrap_or(0)
                .max(width(c.title))
        })
        .collect();
    let total = |w: &[usize]| w.iter().sum::<usize>() + gap * (w.len().saturating_sub(1));
    // Shrink flexible columns (rightmost first) until it fits.
    while total(&widths) > max_width {
        let Some((i, _)) = cols
            .iter()
            .enumerate()
            .filter(|(i, c)| c.flex && widths[*i] > c.min)
            .max_by_key(|(i, _)| widths[*i])
        else {
            break;
        };
        let over = total(&widths) - max_width;
        widths[i] = widths[i].saturating_sub(over).max(cols[i].min);
    }
    let mut out = String::new();
    let line = |cells: Vec<(String, S)>, out: &mut String| {
        let mut parts = Vec::new();
        for (i, (text, st)) in cells.into_iter().enumerate() {
            let t = truncate(&text, widths[i]);
            let pad = widths[i].saturating_sub(width(&t));
            let last = i == cols.len() - 1;
            let cell = if cols[i].right {
                format!("{}{}", " ".repeat(pad), paint(&t, st))
            } else if last {
                paint(&t, st)
            } else {
                format!("{}{}", paint(&t, st), " ".repeat(pad))
            };
            parts.push(cell);
        }
        out.push_str(parts.join(&" ".repeat(gap)).trim_end());
        out.push('\n');
    };
    line(
        cols.iter().map(|c| (c.title.to_string(), S::Dim)).collect(),
        &mut out,
    );
    for r in rows {
        line(r.iter().map(|c| (c.0.clone(), c.1)).collect(), &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate("hello world", 6), "hello…");
        assert_eq!(truncate("hi", 6), "hi");
    }

    #[test]
    fn one_line_collapses_whitespace() {
        assert_eq!(
            one_line("python3 -c import x\n  x.run()", 80),
            "python3 -c import x x.run()"
        );
        assert_eq!(one_line("a\nb c d e f", 6), "a b c…");
    }

    #[test]
    fn table_fits_width() {
        let cols = [rcol("PORT"), col("PROC"), flex("LABEL", 5)];
        let rows = vec![vec![
            Cell::new("3000", S::Plain),
            Cell::new("node", S::Plain),
            Cell::new("a very long label that should be truncated", S::Plain),
        ]];
        let t = table(&cols, &rows, 30);
        for l in t.lines() {
            assert!(width(l) <= 30, "{l}");
        }
        assert!(t.contains('…'));
    }

    #[test]
    fn terminal_text_allows_only_application_styles_and_line_breaks() {
        let text = "\x1b[1mTitle\x1b[0m\npath\x1b]52;c;cGF5bG9hZA==\x07\x1b[2J\u{9b}3J\rhidden";
        let safe = terminal_text(text);
        assert!(safe.starts_with("\x1b[1mTitle\x1b[0m\npath"));
        assert!(!safe.contains("\x1b]52"));
        assert!(!safe.contains("\x1b[2J"));
        assert!(!safe.contains('\u{9b}'));
        assert!(!safe.contains('\r'));
        assert!(!safe.contains('\x07'));
        assert_eq!(terminal_text("\x1b[8mhidden"), "�[8mhidden");
    }

    #[test]
    fn table_metadata_cannot_inject_terminal_commands_or_rows() {
        let rows = [vec![Cell::new("name\nforged\x1b[2J", S::Plain)]];
        let text = table(&[col("NAME")], &rows, 80);
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("name forged�[2J"));
        assert!(!text.contains('\x1b'));
    }
}
