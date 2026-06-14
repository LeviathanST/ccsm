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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Abandoned,
    /// Soft-deleted: hidden from normal view, recoverable.
    Trashed,
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
            Self::Trashed => write!(f, "trashed"),
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

    /// Write spawn info directly to the registry entry. Called at spawn time
    /// when we own the child and know its pid. session_id can be None for
    /// fresh spawns (filled in later from the session file on disk).
    pub fn link_spawn(
        &mut self,
        name: &str,
        pid: u32,
        session_id: Option<&str>,
    ) {
        // Find by name, prefer newest (reverse iterate).
        if let Some(entry) = self.sessions.iter_mut().rev()
            .find(|e| e.name == name)
        {
            entry.pids.clear();
            entry.pids.push(pid);
            // Only set session_id if the entry doesn't have one yet
            // (respects manually-set session_ids from the user).
            if entry.session_id.is_empty() {
                if let Some(sid) = session_id {
                    if !sid.is_empty() {
                        entry.session_id = sid.to_string();
                    }
                }
            }
            if entry.started.is_empty() {
                entry.started = now_iso();
            }
            self.updated = now_iso();
        }
    }

    /// Refresh hook called every 2s. Two jobs:
    /// 1. Fill session_id for fresh spawns (where Claude writes it after startup).
    /// 2. Clean stale pids — remove pids whose process has died (no session file).
    pub fn refresh_from_live(
        &mut self,
        sessions_dir: &PathBuf,
        workspace_path: &str,
    ) -> Result<()> {
        let all = crate::session::load_all(sessions_dir, Some(&PathBuf::from(workspace_path)))?;

        // Only keep pids that still have a live session file on disk.
        let live_pids: std::collections::HashSet<u32> =
            all.iter().map(|s| s.pid).collect();

        for entry in self.sessions.iter_mut() {
            // Fill empty session_id from live session file (fresh spawns).
            if entry.session_id.is_empty()
                && entry.status == SessionStatus::InProgress
                && !entry.pids.is_empty()
            {
                if let Some(live) = all.iter().find(|s| entry.pids.contains(&s.pid)) {
                    entry.session_id = live.session_id.clone();
                    if entry.started.is_empty() {
                        entry.started = format_ts(live.started_at);
                    }
                }
            }

            // Clean stale pids — remove any pid whose session file is gone.
            entry.pids.retain(|p| live_pids.contains(p));
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

    /// Soft-delete: mark a session as Trashed.  No files are touched.
    /// Matches by session_id; falls back to name for seed entries with empty id.
    pub fn trash(&mut self, session_id: &str, name: &str) -> bool {
        if let Some(entry) = self
            .sessions
            .iter_mut()
            .find(|e| e.session_id == session_id || (session_id.is_empty() && e.name == name))
        {
            entry.status = SessionStatus::Trashed;
            self.updated = now_iso();
            true
        } else {
            false
        }
    }

    /// Un-trash: move a trashed session back to InProgress.
    pub fn recover(&mut self, session_id: &str, name: &str) -> bool {
        if let Some(entry) = self
            .sessions
            .iter_mut()
            .find(|e| e.session_id == session_id || (session_id.is_empty() && e.name == name))
        {
            entry.status = SessionStatus::InProgress;
            self.updated = now_iso();
            true
        } else {
            false
        }
    }

    /// Permanently delete a single session: transcript JSONL, any lingering
    /// session files, and the registry entry.  `workspace` is the repo root.
    /// Matches by session_id; falls back to name for seed entries.
    pub fn clean(
        &mut self,
        session_id: &str,
        name: &str,
        home: &std::path::Path,
        workspace: &std::path::Path,
    ) {
        // Only delete files if we have a real session_id
        if !session_id.is_empty() {
            let slug = project_slug(workspace);
            let proj_dir = home.join(".claude").join("projects").join(&slug);
            let transcript = proj_dir.join(format!("{session_id}.jsonl"));
            let _ = std::fs::remove_file(&transcript);
            let session_subdir = proj_dir.join(session_id);
            let _ = std::fs::remove_dir_all(&session_subdir);

            // Remove any session files with this session_id
            if let Ok(entries) = std::fs::read_dir(home.join(".claude").join("sessions")) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(true, |e| e != "json") {
                        continue;
                    }
                    if let Ok(contents) = std::fs::read_to_string(&path) {
                        if contents.contains(session_id) {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        self.sessions.retain(|e| {
            !(e.session_id == session_id
                || (session_id.is_empty() && e.name == name && e.session_id.is_empty()))
        });
        self.updated = now_iso();
    }

    /// Permanently clean every trashed session at once.
    pub fn clean_all_trashed(&mut self, home: &std::path::Path, workspace: &std::path::Path) {
        let trashed: Vec<(String, String)> = self
            .sessions
            .iter()
            .filter(|e| e.status == SessionStatus::Trashed)
            .map(|e| (e.session_id.clone(), e.name.clone()))
            .collect();
        for (sid, name) in &trashed {
            self.clean(sid, name, home, workspace);
        }
        self.updated = now_iso();
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

pub(crate) fn format_ts(ms: u64) -> String {
    let secs = ms / 1000;
    let day_secs = secs % 86400;
    let days = secs / 86400;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    format!("day{days}T{h:02}:{m:02}Z")
}

/// Derive the Claude Code project slug from a workspace path.
/// Claude replaces '/' and other non-alphanumeric chars with '-' in the
/// absolute path, e.g. `/home/user/my_project` → `-home-user-my-project`.
pub(crate) fn project_slug(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect()
}
