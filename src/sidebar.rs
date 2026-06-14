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
    /// True when this entry is trashed (recoverable).
    pub is_trashed: bool,
    /// True when this entry is a visual separator (not actionable).
    pub is_separator: bool,
    /// Multi-line preview text for the right panel. Empty = no preview.
    pub preview_text: String,
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
    pub fn refresh(
        &mut self,
        live_sessions: Vec<Session>,
        registry_sessions: &[WorkspaceSession],
    ) {
        // Remember which entry is selected so we can re-select it after the
        // list is rebuilt (entries may shift when live sessions come/go).
        let old_index: Option<usize> = self.list_state.selected();
        // Track selection by stable identity (session_id), not label.
        // Labels collide ("unnamed") and change across refreshes.
        let selected_id: Option<String> = old_index
            .and_then(|i| self.entries.get(i))
            .and_then(|e| {
                e.live_session
                    .as_ref()
                    .map(|s| s.session_id.clone())
                    .or_else(|| e.registry_session_id.clone())
            });

        let mut active: Vec<SidebarEntry> = Vec::new();
        let mut trashed: Vec<SidebarEntry> = Vec::new();

        // ── Live sessions ───────────────────────────────────────────
        for s in &live_sessions {
            let status_style = match s.status.as_str() {
                "busy" => Style::default().fg(Color::Yellow),
                "idle" => Style::default().fg(Color::Green),
                _ => Style::default().fg(Color::DarkGray),
            };

            // Match THIS live session to its registry entry, not globally.
            // Prefer session_id match; only fall back to pid match when the
            // registry entry has no session_id yet (avoids overriding a
            // manually-set session_id with a stale live session).
            let matching_registry = registry_sessions
                .iter()
                .find(|rs| {
                    (!rs.session_id.is_empty() && rs.session_id == s.session_id)
                        || (rs.session_id.is_empty() && rs.pids.contains(&s.pid))
                });
            let label = matching_registry
                .and_then(|rs| if rs.name.is_empty() { None } else { Some(rs.name.as_str()) })
                .unwrap_or_else(|| if s.name.is_empty() { "unnamed" } else { s.display_name() })
                .to_string();

            // Build preview from registry data + live extras
            let preview = if let Some(rs) = matching_registry {
                format_preview(
                    &label, &rs.goal, &rs.scope, &s.status, &rs.tags,
                    &rs.session_id, &rs.started, &rs.completed,
                    Some(s.cwd_short()), Some(s.pid),
                )
            } else {
                format_preview(
                    &label, "", "", &s.status, &[],
                    &s.session_id,
                    &crate::registry::format_ts(s.started_at),
                    &s.updated_at.map(crate::registry::format_ts).unwrap_or_default(),
                    Some(s.cwd_short()), Some(s.pid),
                )
            };

            // If the matched registry entry has a different session_id
            // (user manually set it to a transcript they want to resume),
            // carry it through so the Enter handler can prefer it.
            let registry_sid = matching_registry
                .and_then(|rs| if rs.session_id.is_empty() { None } else { Some(rs.session_id.clone()) });

            // If no registry matched but a registry entry with the same NAME
            // exists (with a different session_id), the registry takes priority.
            // Hide this live session — it's a stale/accidental process.
            if matching_registry.is_none() {
                if registry_sessions.iter().any(|rs|
                    rs.name == label
                    && !rs.session_id.is_empty()
                    && rs.session_id != s.session_id
                ) {
                    continue; // registry entry with same name takes priority
                }
            }

            active.push(SidebarEntry {
                label,
                detail: s.cwd_short().to_string(),
                is_registry: false,
                status_style,
                live_session: Some(s.clone()),
                registry_session_id: registry_sid,
                is_trashed: false,
                is_separator: false,
                preview_text: preview,
            });
        }

        // ── Registry entries not seen as live ───────────────────────
        // Reverse-iterate so newer entries (pushed later) are seen first;
        // deduplicate by name preferring non-trashed entries.
        let mut dedup: std::collections::HashMap<String, &WorkspaceSession> =
            std::collections::HashMap::new();
        for rs in registry_sessions.iter().rev() {
            // Pending sessions are upcoming topics — only shown in Ctrl+N wizard
            if rs.status == crate::registry::SessionStatus::Pending {
                continue;
            }
            if !rs.session_id.is_empty()
                && live_sessions.iter().any(|ls| ls.session_id == rs.session_id)
            {
                continue; // already shown as live session
            }
            let is_trash = rs.status == crate::registry::SessionStatus::Trashed;
            dedup
                .entry(rs.name.clone())
                .and_modify(|existing| {
                    // Replace a trashed entry with a non-trashed one if the
                    // newer entry (seen first via .rev()) is non-trashed.
                    let existing_trash =
                        existing.status == crate::registry::SessionStatus::Trashed;
                    if existing_trash && !is_trash {
                        *existing = rs;
                    }
                })
                .or_insert(rs);
        }
        for rs in dedup.values() {

            let is_trash = rs.status == crate::registry::SessionStatus::Trashed;

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
                crate::registry::SessionStatus::Trashed => {
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::CROSSED_OUT)
                }
                _ => Style::default().fg(Color::DarkGray),
            };

            let goal_hint = if rs.goal.is_empty() {
                String::new()
            } else {
                format!(" — {}", &rs.goal[..rs.goal.len().min(60)])
            };

            let entry = SidebarEntry {
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
                is_trashed: is_trash,
                is_separator: false,
                preview_text: format_preview(
                    &rs.name, &rs.goal, &rs.scope, &status_label(rs.status), &rs.tags,
                    &rs.session_id, &rs.started, &rs.completed,
                    None, None,
                ),
            };

            if is_trash {
                trashed.push(entry);
            } else {
                active.push(entry);
            }
        }

        // ── Combine: active first, then separator, then trash ───────
        let mut entries: Vec<SidebarEntry> = Vec::new();
        entries.append(&mut active);
        if !trashed.is_empty() {
            entries.push(SidebarEntry {
                label: String::new(),
                detail: String::new(),
                is_registry: true,
                status_style: Style::default().fg(Color::DarkGray),
                live_session: None,
                registry_session_id: None,
                is_trashed: false,
                is_separator: true,
                preview_text: String::new(),
            });
            entries.append(&mut trashed);
        }

        self.entries = entries;

        // ── Restore selection by identity, fall back to position ───
        if self.entries.is_empty() {
            self.list_state.select(None);
        } else if let Some(ref id) = selected_id {
            let pos = self.entries.iter().position(|e| {
                e.live_session
                    .as_ref()
                    .map(|s| &s.session_id == id)
                    .unwrap_or(false)
                    || e.registry_session_id.as_ref().map(|rs| rs == id).unwrap_or(false)
            });
            // If the entry was deleted, clamp to the old row position
            // so we stay near where we were instead of jumping to 0.
            let fallback = old_index
                .map(|i| i.min(self.entries.len().saturating_sub(1)))
                .unwrap_or(0);
            self.list_state.select(pos.or(Some(fallback)));
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
        match self.list_state.selected() {
            Some(i) => {
                let mut next = (i + 1).min(self.entries.len() - 1);
                // Skip separator entries
                while next < self.entries.len() && self.entries[next].is_separator {
                    next = (next + 1).min(self.entries.len() - 1);
                }
                self.list_state.select(Some(next));
            }
            None => {
                // Nothing selected — go to first non-separator entry.
                let first = self.entries.iter().position(|e| !e.is_separator).unwrap_or(0);
                self.list_state.select(Some(first));
            }
        }
    }

    pub fn select_prev(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        match self.list_state.selected() {
            Some(i) => {
                let mut prev = i.saturating_sub(1);
                // Skip separator entries going backward
                while prev > 0 && self.entries[prev].is_separator {
                    prev = prev.saturating_sub(1);
                }
                self.list_state.select(Some(prev));
            }
            None => {
                // Nothing selected — go to last non-separator entry.
                let last = self
                    .entries
                    .iter()
                    .rposition(|e| !e.is_separator)
                    .unwrap_or(0);
                self.list_state.select(Some(last));
            }
        }
    }

    /// Render the sidebar into the given area.
    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let trash_count = self.entries.iter().filter(|e| e.is_trashed).count();

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|e| {
                if e.is_separator {
                    let line = Line::from(Span::styled(
                        "  ── Trash ──────────────────────────",
                        Style::default().fg(Color::Rgb(60, 60, 60)),
                    ));
                    return ListItem::new(line);
                }

                let icon = if e.is_trashed {
                    "🗑"
                } else if e.is_registry {
                    "📋"
                } else {
                    "●"
                };
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

        let active_count = self.entries.len() - trash_count - if trash_count > 0 { 1 } else { 0 };
        let total = active_count + trash_count;
        let title = if trash_count > 0 {
            format!(" Sessions ({total}) 🗑({trash_count}) ")
        } else {
            format!(" Sessions ({total}) ")
        };
        let list = List::new(items)
            .block(
                Block::bordered()
                    .title_top(title)
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

// ── Preview helper ──────────────────────────────────────────────────────

/// Human-readable status label for the preview panel.
fn status_label(s: crate::registry::SessionStatus) -> &'static str {
    use crate::registry::SessionStatus;
    match s {
        SessionStatus::Pending => "pending",
        SessionStatus::InProgress => "in progress",
        SessionStatus::Completed => "completed",
        SessionStatus::Blocked => "blocked",
        SessionStatus::Abandoned => "abandoned",
        SessionStatus::Trashed => "trashed",
    }
}

/// Build a multi-line preview string for the right panel.
fn format_preview(
    label: &str,
    goal: &str,
    scope: &str,
    status: &str,
    tags: &[String],
    session_id: &str,
    started: &str,
    completed: &str,
    cwd_short: Option<&str>,
    pid: Option<u32>,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("  Name       {}", label));
    lines.push(format!("  Status     {}", status));

    if !goal.is_empty() {
        lines.push(String::new());
        lines.push(format!("  Goal       {}", goal));
    }

    if !scope.is_empty() {
        lines.push(String::new());
        // Word-wrap scope text at ~70 chars for readability.
        for chunk in wrap_lines(&scope, 68) {
            lines.push(format!("  Scope      {}", chunk));
        }
    }

    if !tags.is_empty() {
        lines.push(String::new());
        lines.push(format!("  Tags       {}", tags.join(", ")));
    }

    if let Some(cwd) = cwd_short {
        lines.push(format!("  Cwd        {}", cwd));
    }
    if let Some(pid_val) = pid {
        lines.push(format!("  PID        {}", pid_val));
    }

    if !session_id.is_empty() {
        lines.push(String::new());
        lines.push(format!("  Session    {}", session_id));
    }
    if !started.is_empty() {
        lines.push(format!("  Started    {}", started));
    }
    if !completed.is_empty() {
        lines.push(format!("  Completed  {}", completed));
    }

    lines.join("\n")
}

/// Simple word-wrap: split `s` into chunks at most `width` chars,
/// breaking at word boundaries where possible.
fn wrap_lines(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut remaining = s;
    while !remaining.is_empty() {
        if remaining.len() <= width {
            out.push(remaining.to_string());
            break;
        }
        let mut split = width;
        while split > 0 && remaining.as_bytes().get(split).map_or(true, |b| !b.is_ascii_whitespace()) {
            split -= 1;
        }
        if split == 0 {
            split = width;
        }
        out.push(remaining[..split].trim_end().to_string());
        remaining = remaining[split..].trim_start();
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}
