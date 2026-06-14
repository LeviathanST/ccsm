use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::session::Session;

/// Sidebar state: session list + navigation.
pub struct Sidebar {
    pub sessions: Vec<Session>,
    pub list_state: ListState,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            list_state: ListState::default(),
        }
    }

    /// Refresh the session list from disk.
    pub fn refresh(&mut self, sessions: Vec<Session>) {
        self.sessions = sessions;
        // Clamp selection if list shrank
        if self.sessions.is_empty() {
            self.list_state.select(None);
        } else if self
            .list_state
            .selected()
            .map_or(true, |i| i >= self.sessions.len())
        {
            self.list_state.select(Some(0));
        }
    }

    #[allow(dead_code)]
    pub fn selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }

    /// Return a reference to the currently selected session, if any.
    pub fn selected_session(&self) -> Option<&Session> {
        self.list_state
            .selected()
            .and_then(|i| self.sessions.get(i))
    }

    pub fn select_next(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        let next = (i + 1).min(self.sessions.len() - 1);
        self.list_state.select(Some(next));
    }

    pub fn select_prev(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        let prev = i.saturating_sub(1);
        self.list_state.select(Some(prev));
    }

    /// Render the sidebar into the given area.
    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .sessions
            .iter()
            .map(|s| {
                let status_style = match s.status.as_str() {
                    "busy" => Style::default().fg(Color::Yellow),
                    "idle" => Style::default().fg(Color::Green),
                    _ => Style::default().fg(Color::DarkGray),
                };

                let line = Line::from(vec![
                    Span::styled(s.status_label(), status_style),
                    Span::raw(" "),
                    Span::styled(
                        s.display_name(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        s.cwd_short(),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let session_count = self.sessions.len();
        let list = List::new(items)
            .block(
                Block::bordered()
                    .title_top(format!(" Sessions ({session_count}) "))
                    .border_style(Style::default().fg(Color::Rgb(80, 80, 80))),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(180, 180, 180)),
            )
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }
}
