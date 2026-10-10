//! Display helpers for the Agents tab. Selection and actions remain in `app`.

use holdmap_core::agents::{AccessLevel, Agent, AgentsReport, FolderSource};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

/// Search only identity, PID, working folders and displayed tool names, never commands.
pub(super) fn filtered_indices(report: Option<&AgentsReport>, query: &str) -> Vec<usize> {
    report
        .map(|report| {
            report
                .agents
                .iter()
                .enumerate()
                .filter(|(_, agent)| agent.matches(query))
                .map(|(index, _)| index)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn working_folders(agent: &Agent) -> usize {
    agent
        .folders
        .iter()
        .filter(|folder| folder.source != FolderSource::Recent)
        .count()
        + agent.more_folders
}

pub(super) fn access_attention(agent: &Agent) -> bool {
    agent.access.root
        || agent
            .access
            .facts
            .iter()
            .any(|fact| fact.level == AccessLevel::Elevated)
}

/// Keep OS/file-derived labels on one printable line, including hostile filenames.
pub(super) fn safe_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_control()
                || matches!(ch, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

/// Wrap before rendering so scrolling has a precise bound at every terminal width.
/// Keeping span styles makes long paths and evidence readable without a new dependency.
pub(super) fn wrap_lines(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut wrapped = Vec::new();
    for line in lines {
        let mut spans = Vec::new();
        let mut used = 0;
        for span in line.spans {
            let mut part = String::new();
            for ch in span.content.chars() {
                let ch_width = ch.width().unwrap_or(0);
                if used + ch_width > usize::from(width) && used > 0 {
                    if !part.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut part), span.style));
                    }
                    wrapped.push(Line::from(std::mem::take(&mut spans)));
                    used = 0;
                }
                // A full-width glyph cannot fit into a one-column terminal.
                if ch_width > usize::from(width) {
                    part.push('?');
                    used += 1;
                } else {
                    part.push(ch);
                    used += ch_width;
                }
            }
            if !part.is_empty() {
                spans.push(Span::styled(part, span.style));
            }
        }
        wrapped.push(Line::from(spans));
    }
    wrapped
}

#[cfg(test)]
pub(super) fn test_agent(pid: u32, name: &str) -> Agent {
    use holdmap_core::agents::*;
    Agent {
        id: format!("agent:{pid}"),
        product: "codex".into(),
        name: name.into(),
        vendor: "OpenAI".into(),
        kind: AgentKind::Cli,
        pid,
        process_name: "codex".into(),
        command: "private prompt --api-key super-secret".into(),
        started_at: 0,
        parent: None,
        memory_bytes: 64 * 1024 * 1024,
        cpu_percent: 1.0,
        processes: Vec::new(),
        more_processes: 0,
        process_ids: vec![pid],
        tools: Vec::new(),
        more_tools: 0,
        folders: Vec::new(),
        more_folders: 0,
        ports: Vec::new(),
        links: Vec::new(),
        more_links: 0,
        access: AgentAccess {
            user: Some("developer".into()),
            uid: Some(501),
            root: false,
            mine: true,
            facts: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtering_uses_identity_and_pid_without_searching_private_commands() {
        let report = AgentsReport {
            agents: vec![test_agent(123, "Codex"), test_agent(456, "Cursor")],
            ..Default::default()
        };
        assert_eq!(filtered_indices(Some(&report), "cursor"), vec![1]);
        assert_eq!(filtered_indices(Some(&report), "pid:123"), vec![0]);
        assert!(filtered_indices(Some(&report), "super-secret").is_empty());
        assert_eq!(filtered_indices(Some(&report), ""), vec![0, 1]);
        assert!(filtered_indices(None, "").is_empty());
    }

    #[test]
    fn report_text_cannot_emit_controls_or_hide_text_with_bidi_markers() {
        let safe = safe_text("tool\x1b[31m\nfile\t\u{202e}secret\u{2066}");
        assert_eq!(safe, "tool [31m file  secret ");
        assert!(!safe.chars().any(char::is_control));
    }

    #[test]
    fn wrapped_text_fits_narrow_and_unicode_panels_and_retains_styles() {
        let style = ratatui::style::Style::new().fg(ratatui::style::Color::Cyan);
        let lines = wrap_lines(
            vec![
                Line::from(vec![Span::styled("ab界cd", style)]),
                Line::raw(""),
            ],
            4,
        );
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].to_string(), "ab界");
        assert_eq!(lines[1].to_string(), "cd");
        assert_eq!(lines[0].spans[0].style, style);
        assert!(lines.iter().all(|line| line.width() <= 4));
        assert!(wrap_lines(vec![Line::raw("test")], 0).is_empty());
        assert_eq!(wrap_lines(vec![Line::raw("界")], 1)[0].to_string(), "?");
    }
}
