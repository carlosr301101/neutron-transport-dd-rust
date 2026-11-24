//! Terminal user interface built with ratatui.

pub mod app;
pub mod model;

use std::io::IsTerminal;

/// Launch the TUI.
///
/// # Errors
///
/// Returns an error when stdout is not a terminal or the terminal cannot be
/// initialised.
pub fn run() -> Result<u8, String> {
    if !std::io::stdout().is_terminal() {
        return Err("`nts-rs tui` requires an interactive terminal".to_string());
    }

    let mut terminal = ratatui::init();
    let mut app = app::App::default();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result.map(|()| 0).map_err(|error| error.to_string())
}
