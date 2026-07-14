use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct PaneState {
    pub captured_bytes: usize,
    pub label: String,
}

#[derive(Debug, Default)]
pub struct SwarmState {
    panes: HashMap<String, PaneState>,
    labels: HashMap<String, String>,
}

impl SwarmState {
    pub fn set_label(&mut self, pane_id: &str, label: &str) {
        self.labels.insert(label.to_string(), pane_id.to_string());
        self.panes.entry(pane_id.to_string())
            .or_default()
            .label = label.to_string();
    }

    pub fn resolve_target(&self, target: &str) -> String {
        if is_tmux_syntax(target) {
            target.to_string()
        } else {
            self.labels.get(target)
                .cloned()
                .unwrap_or_else(|| target.to_string())
        }
    }

    pub fn update_bytes(&mut self, pane_id: &str, total_bytes: usize) -> usize {
        let state = self.panes.entry(pane_id.to_string()).or_default();
        if total_bytes > state.captured_bytes {
            let delta = total_bytes - state.captured_bytes;
            state.captured_bytes = total_bytes;
            delta
        } else if total_bytes < state.captured_bytes {
            // Content shrunk (e.g. buffer clear) — reset baseline, return full content
            state.captured_bytes = total_bytes;
            total_bytes
        } else {
            0
        }
    }

    pub fn label_for(&self, pane_id: &str) -> Option<&str> {
        self.panes.get(pane_id).and_then(|s| {
            if s.label.is_empty() { None } else { Some(s.label.as_str()) }
        })
    }

    #[allow(dead_code)]
    pub fn prune_stale(&mut self, active_ids: &[String]) {
        self.panes.retain(|id, _| active_ids.contains(id));
    }
}

fn is_tmux_syntax(target: &str) -> bool {
    // Pane ID: %0, %1, %12
    if target.starts_with('%') && target[1..].chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // session:window.pane  or  session:window
    if let Some(colon_idx) = target.find(':') {
        if colon_idx > 0 {
            let after = &target[colon_idx + 1..];
            // after colon must start with a digit or contain a dot separator
            if !after.is_empty() && after.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                return true;
            }
        }
    }
    false
}
