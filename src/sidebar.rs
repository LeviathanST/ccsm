use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::registry::WorkspaceSession;
use crate::session::Session;

/// Unified sidebar entry from live sessions or registry.
pub struct SidebarEntry {
    pub label: String,
    pub detail: String,
    pub is_registry: bool,
    pub status_style: Style,
    /// If this corresponds to a live session, store its data for transcript lookup.
    pub live_session: Option<Session>,
}

/// Sidebar state: session list + navigation.
pub struct Sidebar {
    pub entries: Vec<SidebarEntry>,
    pub list_state: ListState,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            list_state: ListState::default(),
        }
    }

    /// Refresh from live sessions and registry entries.
    /// Registry entries without a matching live session are shown as planned/pending.
    pub fn refresh(
        &mut self,
        live_sessions: Vec<Session>,
        registry_sessions: &[WorkspaceSession],
    ) {
        let mut entries: Vec<SidebarEntry> = Vec::new();

        // Live sessions first
        for s in &live_sessions {
            let status_style = match s.status.as_str() {
                "busy" => Style::default().fg(Color::Yellow),
                "idle" => Style::default().fg(Color::Green),
                _ => Style::default().fg(Color::DarkGray),
            };

            entries.push(SidebarEntry {
                label: s.display_name().to_string(),
                detail: s.cwd_short().to_string(),
                is_registry: false,
                status_style,
                live_session: Some(s.clone()),
            });
        }

        // Registry entries not yet seen as live sessions
        for rs in registry_sessions {
            if live_sessions.iter().any(|ls| ls.session_id == rs.session_id) {
                continue; // already shown above
            }
            let status_style = match rs.status {
                crate::registry::SessionStatus::Completed => {
                    Style::default().fg(Color::Green)
                }
                crate::registry::SessionStatus::InProgress => {
                    Style::default().fg(Color::Yellow)
                }
                crate::registry::SessionStatus::Blocked => {
                    Style::default().fg(Color::Red)
                }
                _ => Style::default().fg(Color::DarkGray),
            };

            let goal_hint = if rs.goal.is_empty() {
                String::new()
            } else {
                format!(" — {}", &rs.goal[..rs.goal.len().min(60)])
            };

            entries.push(SidebarEntry {
                label: rs.name.clone(),
                detail: goal_hint,
                is_registry: true,
                status_style,
                live_session: None,
            });
        }

        self.entries = entries;

        if self.entries.is_empty() {
            self.list_state.select(None);
        } else if self
            .list_state
            .selected()
            .map_or(true, |i| i >= self.entries.len())
        {
            self.list_state.select(Some(0));
        }
    }

    #[allow(dead_code)]
    pub fn selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }

    /// Return a reference to the selected entry, if any.
    pub fn selected_entry(&self) -> Option<&SidebarEntry> {
        self.list_state
            .selected()
            .and_then(|i| self.entries.get(i))
    }

    /// Return a reference to the live session, if the selected entry has one.
    pub fn selected_session(&self) -> Option<&Session> {
        self.selected_entry().and_then(|e| e.live_session.as_ref())
    }

    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        let next = (i + 1).min(self.entries.len() - 1);
        self.list_state.select(Some(next));
    }

    pub fn select_prev(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        let prev = i.saturating_sub(1);
        self.list_state.select(Some(prev));
    }

    /// Render the sidebar into the given area.
    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|e| {
                let icon = if e.is_registry { "📋" } else { "●" };
                let line = Line::from(vec![
                    Span::styled(icon, e.status_style),
                    Span::raw(" "),
                    Span::styled(&e.label, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    Span::styled(&e.detail, Style::default().fg(Color::DarkGray)),
                ]);
                ListItem::new(line)
            })
            .collect();

        let count = self.entries.len();
        let list = List::new(items)
            .block(
                Block::bordered()
                    .title_top(format!(" Sessions ({count}) "))
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
