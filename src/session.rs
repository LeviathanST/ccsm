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

    /// Claude Code's project-directory slug for this session's cwd.
    /// Slashes become dashes, leading slash dropped.
    /// e.g. `/home/levi/workspaces/cc-tui` → `-home-levi-workspaces-cc-tui`
    pub fn cwd_slug(&self) -> String {
        self.cwd.replace('/', "-")
    }

    /// Human-readable started-at timestamp.
    pub fn started_at_display(&self) -> String {
        // started_at is in milliseconds since epoch
        let secs = self.started_at / 1000;
        // Simple display: seconds since epoch → roughly readable
        // For a proper display we'd want chrono, but this keeps deps light
        format!("ts_{secs}")
    }
}

/// Check if a transcript file exists for a session.
pub fn transcript_exists(home: &str, session: &Session) -> bool {
    let path = PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(session.cwd_slug())
        .join(format!("{}.jsonl", session.session_id));
    path.exists()
}

/// Return the size of the transcript file in bytes, if it exists.
pub fn transcript_size(home: &str, session: &Session) -> Option<u64> {
    let path = PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(session.cwd_slug())
        .join(format!("{}.jsonl", session.session_id));
    std::fs::metadata(&path).ok().map(|m| m.len())
}

/// Load sessions from disk, optionally filtering to those whose cwd
/// starts with `workspace` (the current project directory).
pub fn load_all(sessions_dir: &PathBuf, workspace: Option<&PathBuf>) -> Result<Vec<Session>> {
    let all = load_all_unfiltered(sessions_dir)?;
    if let Some(ws) = workspace {
        let ws_str = ws.to_string_lossy().to_string();
        Ok(all
            .into_iter()
            .filter(|s| s.cwd.starts_with(&ws_str))
            .collect())
    } else {
        Ok(all)
    }
}

/// Load all session files from the sessions directory (no filter).
fn load_all_unfiltered(sessions_dir: &PathBuf) -> Result<Vec<Session>> {
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
