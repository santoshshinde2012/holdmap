//! Interactive terminal UI (ratatui): keyboard-first list with search, filters, sorting, a details
//! pane with the plain-English explanation, and stop/kill with a confirmation dialog.

mod app;
mod graph;
mod graph_ui;
mod theme;
mod ui;

use anyhow::Result;
use app::App;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use std::time::Duration;

pub fn run(docker: bool) -> Result<u8> {
    let mut terminal = ratatui::init();
    let mut app = App::new(docker);
    let result = (|| -> Result<()> {
        while !app.quit {
            app.tick();
            terminal.draw(|f| ui::draw(f, &mut app))?;
            if event::poll(Duration::from_millis(120))? {
                match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => app.on_key(k),
                    Event::Mouse(m) => app.on_mouse(m),
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result?;
    Ok(crate::exit::OK)
}
