use anyhow::{Context, Result};
use serde::Deserialize;

/// A snapshot of a Claude Code session from `~/.claude/sessions/<pid>.json`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Session {
    pub pid: u32,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub cwd: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(rename = "startedAt")]
    pub started_at: u64,
    #[serde(rename = "updatedAt")]
    #[serde(default)]
    pub updated_at: Option<u64>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

impl Session {
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

    /// Human-readable started-at timestamp (kept for future use).
    #[allow(dead_code)]
    pub fn started_at_display(&self) -> String {
        let secs = self.started_at / 1000;
        format!("ts_{secs}")
    }
}

/// Load sessions from disk, optionally filtering to those whose cwd
/// starts with `workspace` (the current project directory).
pub fn load_all(sessions_dir: &std::path::Path, workspace: Option<&std::path::Path>) -> Result<Vec<Session>> {
    let all = load_all_unfiltered(sessions_dir)?;
    if let Some(ws) = workspace {
        let ws_str = ws.to_string_lossy();
        // Ensure trailing separator so /home/user/proj doesn't match /home/user/proj-other
        Ok(all
            .into_iter()
            .filter(|s| s.cwd == *ws_str || s.cwd.starts_with(&format!("{ws_str}/")))
            .collect())
    } else {
        Ok(all)
    }
}

/// Load all session files from the sessions directory (no filter).
fn load_all_unfiltered(sessions_dir: &std::path::Path) -> Result<Vec<Session>> {
    let mut sessions: Vec<Session> = Vec::new();

    let entries = match std::fs::read_dir(sessions_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(sessions),
        Err(e) => return Err(e).context("reading sessions directory"),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                if let Ok(session) = serde_json::from_str::<Session>(&contents) {
                    // Skip transient/incomplete session files: updatedAt is a late
                    // field in the JSON; its absence means the file was truncated
                    // mid-write by cds. These files appear and disappear within
                    // seconds, causing sidebar flicker.
                    if session.updated_at.is_none() {
                        continue;
                    }
                    sessions.push(session);
                }
            }
            Err(_) => continue,
        }
    }

    // Sort by most recently updated first
    sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));

    Ok(sessions)
}

