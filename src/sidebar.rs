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
    /// If this is a registry entry with a linked session_id, store it for transcript lookup.
    pub registry_session_id: Option<String>,
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
    /// `workspace_cwd` is the current workspace path (for transcript lookup on registry entries).
    pub fn refresh(
        &mut self,
        live_sessions: Vec<Session>,
        registry_sessions: &[WorkspaceSession],
    ) {
        // Remember which entry is selected so we can re-select it after the
        // list is rebuilt (entries may shift when live sessions come/go).
        let selected_id: Option<String> = self
            .list_state
            .selected()
            .and_then(|i| self.entries.get(i))
            .and_then(|e| {
                e.live_session
                    .as_ref()
                    .map(|s| s.session_id.clone())
                    .or_else(|| e.registry_session_id.clone())
                    .or_else(|| Some(e.label.clone()))
            });

        let mut entries: Vec<SidebarEntry> = Vec::new();

        // Live sessions first.
        // If the session file has no name (cds writes sessionId but no display name),
        // resolve it from the matching registry entry so users see "Test4" not "unnamed".
        for s in &live_sessions {
            let status_style = match s.status.as_str() {
                "busy" => Style::default().fg(Color::Yellow),
                "idle" => Style::default().fg(Color::Green),
                _ => Style::default().fg(Color::DarkGray),
            };

            let label = if s.name.is_empty() {
                registry_sessions
                    .iter()
                    .find(|rs| rs.session_id == s.session_id)
                    .map(|rs| rs.name.as_str())
                    .unwrap_or("unnamed")
                    .to_string()
            } else {
                s.display_name().to_string()
            };

            entries.push(SidebarEntry {
                label,
                detail: s.cwd_short().to_string(),
                is_registry: false,
                status_style,
                live_session: Some(s.clone()),
                registry_session_id: None,
            });
        }

        // Registry entries not yet seen as live sessions
        for rs in registry_sessions {
            if !rs.session_id.is_empty()
                && live_sessions.iter().any(|ls| ls.session_id == rs.session_id)
            {
                continue; // already shown as live session
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
                registry_session_id: if rs.session_id.is_empty() {
                    None
                } else {
                    Some(rs.session_id.clone())
                },
            });
        }

        self.entries = entries;

        // Preserve selection on the same logical entry after list rebuild.
        // When a live session replaces a registry entry (same session_id but
        // different position), the old row index is stale.  Track by identity.
        if self.entries.is_empty() {
            self.list_state.select(None);
        } else if let Some(ref id) = selected_id {
            let pos = self.entries.iter().position(|e| {
                e.live_session
                    .as_ref()
                    .map(|s| &s.session_id == id)
                    .unwrap_or(false)
                    || e.registry_session_id.as_ref().map(|rs| rs == id).unwrap_or(false)
                    || &e.label == id
            });
            self.list_state.select(pos.or(Some(0)));
        } else {
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
