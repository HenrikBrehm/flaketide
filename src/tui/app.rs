//! TUI app state machine.

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::domain::{Config, FlakeReport};
use crate::error::Result;
use crate::stats;
use crate::store::Store;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Screen {
    List,
    Detail,
    Help,
}

pub struct App {
    store: Store,
    cfg: Config,
    pub flakes: Vec<FlakeReport>,
    pub list_state: ListState,
    pub screen: Screen,
    prev_screen: Option<Screen>,
}

impl App {
    pub async fn new(store: Store, cfg: Config) -> Result<Self> {
        let flakes = stats::summarize(&store, &cfg.thresholds).await?;
        let mut list_state = ListState::default();
        if !flakes.is_empty() {
            list_state.select(Some(0));
        }
        Ok(Self {
            store,
            cfg,
            flakes,
            list_state,
            screen: Screen::List,
            prev_screen: None,
        })
    }

    pub fn next(&mut self) {
        if self.flakes.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        let next = (i + 1).min(self.flakes.len() - 1);
        self.list_state.select(Some(next));
    }

    pub fn prev(&mut self) {
        let i = self.list_state.selected().unwrap_or(0);
        let prev = i.saturating_sub(1);
        self.list_state.select(Some(prev));
    }

    pub fn enter(&mut self) {
        if matches!(self.screen, Screen::List) && !self.flakes.is_empty() {
            self.prev_screen = Some(self.screen);
            self.screen = Screen::Detail;
        }
    }

    pub fn toggle_help(&mut self) {
        if matches!(self.screen, Screen::Help) {
            self.screen = self.prev_screen.unwrap_or(Screen::List);
        } else {
            self.prev_screen = Some(self.screen);
            self.screen = Screen::Help;
        }
    }

    /// Returns true if the app should exit.
    pub fn on_escape(&mut self) -> bool {
        match self.screen {
            Screen::List => true,
            Screen::Detail | Screen::Help => {
                self.screen = self.prev_screen.unwrap_or(Screen::List);
                self.prev_screen = None;
                false
            }
        }
    }

    pub fn draw(&mut self, f: &mut ratatui::Frame) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(1)])
            .split(f.area());

        let title = Paragraph::new(format!(
            " flaketide — {} flaky tests (screen: {:?})",
            self.flakes.len(),
            self.screen
        ))
        .block(Block::default().borders(Borders::ALL).title("flaketide"));
        f.render_widget(title, layout[0]);

        match self.screen {
            Screen::List => self.draw_list(f, layout[1]),
            Screen::Detail => self.draw_detail(f, layout[1]),
            Screen::Help => self.draw_help(f, layout[1]),
        }

        let hint = Paragraph::new(" q quit  ↑/↓ navigate  Enter detail  ? help ");
        f.render_widget(hint, layout[2]);
    }

    fn draw_list(&mut self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let items: Vec<ListItem> = self
            .flakes
            .iter()
            .map(|v| {
                ListItem::new(format!(
                    "{:>5.2}  {:>5.2}  [{:.2}, {:.2}]  {}/{}  {}",
                    v.severity, v.flake_prob, v.hdi_low, v.hdi_high, v.failures, v.runs, v.id
                ))
            })
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Flaky tests"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("▶ ");
        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn draw_detail(&self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let Some(idx) = self.list_state.selected() else { return; };
        let Some(v) = self.flakes.get(idx) else { return; };
        let body = format!(
            "Test: {}\n\
             Runs: {}    Failures: {}\n\
             Flake prob: {:.3}   95% CI: [{:.3}, {:.3}]\n\
             Severity: {:.3}\n\
             First seen: {}\n\
             Last seen:  {}\n\n\
             Recent messages:\n{}",
            v.id,
            v.runs,
            v.failures,
            v.flake_prob,
            v.hdi_low,
            v.hdi_high,
            v.severity,
            v.first_seen,
            v.last_seen,
            v.recent_messages
                .iter()
                .take(5)
                .map(|m| {
                    // (L1) Strip terminal control sequences before TUI render.
                    let line = m.lines().next().unwrap_or("");
                    let safe = crate::util::sanitize::sanitize_terminal_text(line);
                    format!("  • {}", safe)
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let p = Paragraph::new(body)
            .block(Block::default().borders(Borders::ALL).title("Detail (Esc back)"));
        f.render_widget(p, area);
    }

    fn draw_help(&self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let body = "Keymap:\n\
                    \n  q / Esc          quit (or back from a screen)\
                    \n  ↑ / k            previous test\
                    \n  ↓ / j            next test\
                    \n  Enter            open detail for selected test\
                    \n  ?                toggle this help screen\n";
        let p = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(p, area);
    }
}
