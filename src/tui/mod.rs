//! Interactive ratatui explorer.

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::domain::Config;
use crate::error::{FlaketideError, Result};
use crate::store::Store;

pub mod app;
pub mod screens;
pub mod widgets;

pub use app::App;

pub async fn run_tui(store: Store, cfg: Config) -> Result<()> {
    enable_raw_mode().map_err(io::Error::from)?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen).map_err(io::Error::from)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(io::Error::from)?;

    let mut app = App::new(store, cfg).await?;

    let result = event_loop(&mut terminal, &mut app).await;

    disable_raw_mode().map_err(io::Error::from)?;
    terminal
        .backend_mut()
        .execute(LeaveAlternateScreen)
        .map_err(io::Error::from)?;

    result
}

async fn event_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal
            .draw(|f| app.draw(f))
            .map_err(|e| FlaketideError::Other(e.into()))?;

        if event::poll(Duration::from_millis(250)).map_err(io::Error::from)? {
            if let Event::Key(key) = event::read().map_err(io::Error::from)? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            if app.on_escape() {
                                return Ok(());
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => app.next(),
                        KeyCode::Up | KeyCode::Char('k') => app.prev(),
                        KeyCode::Enter => app.enter(),
                        KeyCode::Char('?') => app.toggle_help(),
                        _ => {}
                    }
                }
            }
        }
    }
}
