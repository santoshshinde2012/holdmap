//! Semantic text styles for the TUI. A terminal only has weight (bold / normal) and colour, so the
//! desktop type roles map onto them the same way everywhere:
//!
//! | desktop role        | terminal                         |
//! |---------------------|----------------------------------|
//! | title / heading     | bold (block titles: accent bold) |
//! | port numbers        | accent bold                      |
//! | label (small caps)  | muted bold (table headers)       |
//! | body                | default                          |
//! | caption / body-sm   | muted                            |
//! | kbd                 | accent bold                      |
use super::ui::{ACCENT, MUTED};
use ratatui::style::{Modifier, Style};

pub fn heading() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}
/// Block titles and the recommendation lead-in.
pub fn title() -> Style {
    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
}
pub fn port() -> Style {
    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
}
pub fn label() -> Style {
    Style::new().fg(MUTED).add_modifier(Modifier::BOLD)
}
pub fn muted() -> Style {
    Style::new().fg(MUTED)
}
pub fn key() -> Style {
    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
}
