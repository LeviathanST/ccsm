use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

/// A snapshot of a Claude Code session from `~/.claude/sessions/<pid>.json`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Session {
    pub pid: u32,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub cwd: String,
    pub name: String,
    pub status: String,
    #[serde(rename = "startedAt")]
    pub started_at: u64,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<u64>,
    pub kind: Option<String>,
    pub version: Option<String>,
}

impl Session {
    /// Human-friendly status label for the sidebar.
    pub fn status_label(&self) -> &str {
        match self.status.as_str() {
            "busy" => "●",
            "idle" => "○",
            "gone" => "✕",
            _ => "?",
        }
    }

    /// Short display name: session name or cwd basename.
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() { "unnamed" } else { &self.name }
    }

    /// Basename of the working directory.
    pub fn cwd_short(&self) -> &str {
        std::path::Path::new(&self.cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.cwd)
    }
}

/// Load all session files from the sessions directory.
pub fn load_all(sessions_dir: &PathBuf) -> Result<Vec<Session>> {
    let mut sessions: Vec<Session> = Vec::new();

    let entries = match std::fs::read_dir(sessions_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(sessions),
        Err(e) => return Err(e).context("reading sessions directory"),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "json") {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                if let Ok(session) = serde_json::from_str::<Session>(&contents) {
                    sessions.push(session);
                }
            }
            Err(_) => continue,
        }
    }

    // Sort by most recently updated first
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(sessions)
}
