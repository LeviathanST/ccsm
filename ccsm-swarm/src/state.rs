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
        if target.starts_with('%') || target.contains(':') || target.contains('.') {
            target.to_string()
        } else {
            self.labels.get(target)
                .cloned()
                .unwrap_or_else(|| target.to_string())
        }
    }

    pub fn update_bytes(&mut self, pane_id: &str, total_bytes: usize) -> usize {
        let state = self.panes.entry(pane_id.to_string()).or_default();
        let delta = if total_bytes > state.captured_bytes {
            total_bytes - state.captured_bytes
        } else {
            0
        };
        state.captured_bytes = total_bytes;
        delta
    }

    pub fn label_for(&self, pane_id: &str) -> Option<&str> {
        self.panes.get(pane_id).and_then(|s| {
            if s.label.is_empty() { None } else { Some(s.label.as_str()) }
        })
    }
}
