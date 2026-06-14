use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::session::Session;

// ── Tier 1: Global Overview ─────────────────────────────────────────

/// Global session registry at `~/.claude/sessions.json`.
/// Lightweight overview of all workspaces and their sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRegistry {
    pub updated: String,
    pub workspaces: Vec<WorkspaceOverview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceOverview {
    pub path: String,
    pub name: String,
    pub session_count: usize,
    pub last_activity: String,
    pub sessions: Vec<GlobalSessionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSessionEntry {
    pub pid: u32,
    pub session_id: String,
    pub name: String,
    pub status: String,
}

impl GlobalRegistry {
    /// Build from the raw session files in `~/.claude/sessions/`.
    pub fn build(sessions_dir: &PathBuf) -> Result<Self> {
        let sessions = crate::session::load_all(sessions_dir, None)?;

        // Group by workspace (cwd)
        let mut ws_map: std::collections::BTreeMap<String, Vec<&Session>> =
            std::collections::BTreeMap::new();
        for s in &sessions {
            ws_map.entry(s.cwd.clone()).or_default().push(s);
        }

        let mut workspaces: Vec<WorkspaceOverview> = ws_map
            .into_iter()
            .map(|(path, ss)| {
                let name = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&path)
                    .to_string();

                let session_count = ss.len();

                let last_activity = ss
                    .iter()
                    .filter_map(|s| s.updated_at)
                    .max()
                    .map(|ts| format_ts(ts))
                    .unwrap_or_default();

                let sessions: Vec<GlobalSessionEntry> = ss
                    .iter()
                    .map(|s| GlobalSessionEntry {
                        pid: s.pid,
                        session_id: s.session_id.clone(),
                        name: s.display_name().to_string(),
                        status: s.status.clone(),
                    })
                    .collect();

                WorkspaceOverview {
                    path,
                    name,
                    session_count,
                    last_activity,
                    sessions,
                }
            })
            .collect();

        // Sort by most recently active first
        workspaces.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

        Ok(Self {
            updated: now_iso(),
            workspaces,
        })
    }

    /// Load from disk, or build fresh if missing.
    pub fn load_or_build(global_path: &PathBuf, sessions_dir: &PathBuf) -> Result<Self> {
        if global_path.exists() {
            match std::fs::read_to_string(global_path) {
                Ok(contents) => {
                    if let Ok(reg) = serde_json::from_str::<GlobalRegistry>(&contents) {
                        return Ok(reg);
                    }
                }
                Err(_) => {}
            }
        }
        let reg = Self::build(sessions_dir)?;
        reg.save(global_path)?;
        Ok(reg)
    }

    pub fn save(&self, path: &PathBuf) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(path, json).context("writing global registry")?;
        Ok(())
    }

    /// Find a workspace overview by path.
    pub fn find_workspace(&self, path: &str) -> Option<&WorkspaceOverview> {
        self.workspaces.iter().find(|w| w.path == path)
    }
}

// ── Tier 2: Workspace Detail ────────────────────────────────────────

/// Per-workspace session registry at `.claude/sessions.json` in the repo.
/// Rich detail with human/agent-curated goal, scope, status, and tags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRegistry {
    pub updated: String,
    pub sessions: Vec<WorkspaceSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSession {
    pub session_id: String,
    pub name: String,
    pub goal: String,
    pub scope: String,
    #[serde(default = "default_status")]
    pub status: SessionStatus,
    #[serde(default)]
    pub pids: Vec<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub started: String,
    #[serde(default)]
    pub completed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Abandoned,
}

fn default_status() -> SessionStatus {
    SessionStatus::Pending
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Blocked => write!(f, "blocked"),
            Self::Abandoned => write!(f, "abandoned"),
        }
    }
}

impl WorkspaceRegistry {
    /// Load from the repo's `.claude/sessions.json`, or return empty.
    pub fn load(repo_path: &PathBuf) -> Result<Self> {
        let path = repo_path.join(".claude").join("sessions.json");
        if path.exists() {
            let contents = std::fs::read_to_string(&path).context("reading workspace registry")?;
            let mut reg: WorkspaceRegistry =
                serde_json::from_str(&contents).context("parsing workspace registry")?;
            reg.updated = now_iso();
            Ok(reg)
        } else {
            Ok(Self {
                updated: now_iso(),
                sessions: Vec::new(),
            })
        }
    }

    /// Merge auto-data from live session files into the registry.
    /// Updates pids, session_id, and timestamps; preserves curated fields.
    ///
    /// Matching strategy:
    /// 1. Exact match by `session_id`
    /// 2. Fallback: link unlinked registry entries (empty session_id, no pids)
    ///    to live sessions from the same workspace. This handles the case
    ///    where Ctrl+N creates a registry entry before cds writes its
    ///    session file — on next refresh, the live session gets linked.
    pub fn merge_live_sessions(
        &mut self,
        sessions_dir: &PathBuf,
        workspace_path: &str,
    ) -> Result<()> {
        let all =
            crate::session::load_all(sessions_dir, Some(&PathBuf::from(workspace_path)))?;

        for live in &all {
            // Strategy 1: exact match by session_id
            let matched = self
                .sessions
                .iter_mut()
                .find(|e| e.session_id == live.session_id);

            if let Some(entry) = matched {
                if !entry.pids.contains(&live.pid) {
                    entry.pids.push(live.pid);
                }
                if entry.started.is_empty() {
                    entry.started = format_ts(live.started_at);
                }
                if live.status == "busy" && entry.status == SessionStatus::Pending {
                    entry.status = SessionStatus::InProgress;
                }
            } else {
                // Strategy 2: link last unlinked entry (most recent Ctrl+N)
                if let Some(entry) = self
                    .sessions
                    .iter_mut()
                    .rev()
                    .find(|e| e.session_id.is_empty() && e.pids.is_empty())
                {
                    entry.session_id = live.session_id.clone();
                    entry.pids.push(live.pid);
                    if entry.started.is_empty() {
                        entry.started = format_ts(live.started_at);
                    }
                    if live.status == "busy" && entry.status == SessionStatus::Pending {
                        entry.status = SessionStatus::InProgress;
                    }
                }
            }
        }

        self.updated = now_iso();
        Ok(())
    }

    /// Save back to disk.
    pub fn save(&self, repo_path: &PathBuf) -> Result<()> {
        let path = repo_path.join(".claude").join("sessions.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json).context("writing workspace registry")?;
        Ok(())
    }

    /// Seed with initial entries if empty. Safe to call on every startup.
    pub fn seed(&mut self, entries: Vec<WorkspaceSession>) {
        if self.sessions.is_empty() {
            self.sessions = entries;
        }
    }

    /// Default seed entries for cc-tui's own workspace.
    /// Each project should define its own seed based on its build plan.
    pub fn default_seed() -> Vec<WorkspaceSession> {
        vec![
            WorkspaceSession {
                session_id: String::new(),
                name: "phase-1-pty-embedding".into(),
                goal: "Embed cds in a PTY with fixed-grid ANSI rendering".into(),
                scope: "Phase 1: spawn cds via portable-pty, render ANSI output as styled ratatui Text using tmux-style fixed-grid approach. Input passthrough (typing, arrows, Ctrl+C, Tab, F-keys). Quit on Ctrl+Q.".into(),
                status: SessionStatus::Completed,
                pids: vec![],
                tags: vec!["pty".into(), "ratatui".into(), "vt100".into(), "phase-1".into()],
                started: String::new(),
                completed: String::new(),
            },
            WorkspaceSession {
                session_id: String::new(),
                name: "phase-2-sidebar".into(),
                goal: "Add sidebar with session list and focus switching".into(),
                scope: "Phase 2: read ~/.claude/sessions/*.json, render session list with status indicators, 30/70 layout split, Tab focus switching, arrow/vim key navigation, workspace-aware filtering, session detail overlay.".into(),
                status: SessionStatus::Completed,
                pids: vec![],
                tags: vec!["sidebar".into(), "ratatui".into(), "sessions".into(), "phase-2".into()],
                started: String::new(),
                completed: String::new(),
            },
            WorkspaceSession {
                session_id: String::new(),
                name: "phase-3-session-replay".into(),
                goal: "View historical session transcripts in the PTY panel".into(),
                scope: "Enter on session loads JSONL transcript, renders user/assistant messages and tool calls with scroll support (↑↓/PgUp/PgDn/Home), Esc/Tab returns to live cds. ViewMode enum switches between Live and Transcript.".into(),
                status: SessionStatus::Completed,
                pids: vec![],
                tags: vec!["transcript".into(), "replay".into(), "phase-3".into()],
                started: String::new(),
                completed: String::new(),
            },
            WorkspaceSession {
                session_id: String::new(),
                name: "session-registry".into(),
                goal: "Two-tier session registry for team visibility".into(),
                scope: "Tier 1: global overview at ~/.claude/sessions.json scanning all workspaces. Tier 2: per-repo .claude/sessions.json with goal/scope/status/tags. Auto-merges live session data. Survives ephemeral session cleanup.".into(),
                status: SessionStatus::InProgress,
                pids: vec![],
                tags: vec!["registry".into(), "sessions".into(), "team".into()],
                started: String::new(),
                completed: String::new(),
            },
        ]
    }

    /// Create an empty registry.
    pub fn empty() -> Self {
        Self {
            updated: String::new(),
            sessions: Vec::new(),
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn now_iso() -> String {
    // Simple ISO-like timestamp without chrono dependency
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs = ts % 86400;
    let days = ts / 86400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("day{days}T{h:02}:{m:02}:{s:02}Z")
}

fn format_ts(ms: u64) -> String {
    let secs = ms / 1000;
    let day_secs = secs % 86400;
    let days = secs / 86400;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    format!("day{days}T{h:02}:{m:02}Z")
}
