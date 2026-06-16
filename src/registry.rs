use anyhow::{Context, Result};
use fs2::FileExt;
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

    /// Load with an exclusive lock held for the lifetime of the returned `LockFile`.
    /// Use this for every read-modify-write cycle to prevent races between
    /// chained `ccsm` mutation commands.
    pub fn load_locked(repo_path: &PathBuf) -> Result<(Self, LockFile)> {
        let lock = LockFile::acquire(repo_path)?;
        let reg = Self::load(repo_path)?;
        Ok((reg, lock))
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

    /// Archive: delete transcript + session files but KEEP the registry entry.
    /// Clears `session_id` so the entry remains as a permanent work log.
    /// Returns total bytes freed.
    pub fn archive(
        &mut self,
        session_id: &str,
        name: &str,
        home: &std::path::Path,
        workspace: &std::path::Path,
    ) -> u64 {
        let mut freed: u64 = 0;
        if !session_id.is_empty() {
            let slug = project_slug(workspace);
            let proj_dir = home.join(".claude").join("projects").join(&slug);
            let transcript = proj_dir.join(format!("{session_id}.jsonl"));
            if let Ok(meta) = std::fs::metadata(&transcript) {
                freed += meta.len();
            }
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
                            if let Ok(meta) = std::fs::metadata(&path) {
                                freed += meta.len();
                            }
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        // Clear session_id — keep the entry as a work log
        if let Some(entry) = self
            .sessions
            .iter_mut()
            .find(|e| e.session_id == session_id || (session_id.is_empty() && e.name == name))
        {
            entry.session_id.clear();
            entry.pids.clear();
        }
        self.updated = now_iso();
        freed
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

    /// Default seed entries for ccsm's own workspace.
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

// ── File Locking ─────────────────────────────────────────────────────

/// Advisory exclusive lock on `.claude/sessions.json.lock`.
///
/// Acquired before reading the registry and held until dropped —
/// this prevents the read-modify-write race when multiple `ccsm`
/// mutation commands are chained with `&&` in a single shell call.
///
/// The OS releases the lock automatically if the process exits,
/// so a crash won't leave the registry permanently locked.
pub struct LockFile {
    _file: std::fs::File,
}

impl LockFile {
    pub fn acquire(repo_path: &PathBuf) -> Result<Self> {
        let lock_path = repo_path.join(".claude").join("sessions.json.lock");
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .context("opening lock file")?;
        file.lock_exclusive()
            .context("acquiring exclusive lock on sessions.json")?;
        Ok(Self { _file: file })
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

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// Create a temp workspace with `.claude/sessions.json` pre-populated.
    fn temp_workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let reg_path = claude_dir.join("sessions.json");
        // Start with an empty but valid registry
        let reg = WorkspaceRegistry {
            updated: "test".into(),
            sessions: vec![],
        };
        std::fs::write(&reg_path, serde_json::to_string_pretty(&reg).unwrap()).unwrap();
        (dir, reg_path)
    }

    // ── LockFile tests ─────────────────────────────────────────────

    #[test]
    fn lock_acquire_creates_lockfile() {
        let (dir, _reg_path) = temp_workspace();
        let lock_path = dir.path().join(".claude").join("sessions.json.lock");
        assert!(!lock_path.exists());

        let _lock = LockFile::acquire(&dir.path().to_path_buf()).unwrap();
        assert!(lock_path.exists());
    }

    #[test]
    fn lock_drop_releases() {
        let (dir, _reg_path) = temp_workspace();
        let workspace = dir.path().to_path_buf();

        // Acquire and drop
        let lock = LockFile::acquire(&workspace).unwrap();
        drop(lock);

        // Should be able to acquire again immediately (lock released)
        let _lock2 = LockFile::acquire(&workspace).unwrap();
    }

    #[test]
    fn lock_exclusive_blocks_same_process() {
        let (dir, _reg_path) = temp_workspace();
        let workspace = dir.path().to_path_buf();

        // Acquire exclusive lock on one fd
        let _lock1 = LockFile::acquire(&workspace).unwrap();

        // Try to acquire on a different fd — should fail with try_lock
        let lock_path = workspace.join(".claude").join("sessions.json.lock");
        let file2 = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();

        // try_lock_exclusive should fail because lock1 still holds it
        assert!(fs2::FileExt::try_lock_exclusive(&file2).is_err());
    }

    #[test]
    fn lock_released_after_drop_allows_new_lock() {
        let (dir, _reg_path) = temp_workspace();
        let workspace = dir.path().to_path_buf();

        let lock = LockFile::acquire(&workspace).unwrap();
        drop(lock);

        // Now try_lock should succeed
        let lock_path = workspace.join(".claude").join("sessions.json.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        assert!(fs2::FileExt::try_lock_exclusive(&file).is_ok());
    }

    // ── load_locked tests ──────────────────────────────────────────

    #[test]
    fn load_locked_loads_registry() {
        let (dir, reg_path) = temp_workspace();
        // Write a known entry
        let reg = WorkspaceRegistry {
            updated: "day0T00:00:00Z".into(),
            sessions: vec![WorkspaceSession {
                session_id: "abc-123".into(),
                name: "test-session".into(),
                goal: "test goal".into(),
                scope: String::new(),
                status: SessionStatus::InProgress,
                pids: vec![42],
                tags: vec!["test".into()],
                started: "day0T00:00:00Z".into(),
                completed: String::new(),
            }],
        };
        std::fs::write(&reg_path, serde_json::to_string_pretty(&reg).unwrap()).unwrap();

        let (loaded, _lock) = WorkspaceRegistry::load_locked(&dir.path().to_path_buf()).unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.sessions[0].name, "test-session");
        assert_eq!(loaded.sessions[0].goal, "test goal");
        assert_eq!(loaded.sessions[0].session_id, "abc-123");
    }

    #[test]
    fn load_locked_holds_lock_during_mutation() {
        let (dir, _reg_path) = temp_workspace();
        let workspace = dir.path().to_path_buf();

        let (mut reg, _lock) = WorkspaceRegistry::load_locked(&workspace).unwrap();

        // While lock is held, try_lock should fail on another fd
        let lock_path = workspace.join(".claude").join("sessions.json.lock");
        let other_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        assert!(fs2::FileExt::try_lock_exclusive(&other_file).is_err());

        // Mutate and save while holding the lock
        reg.sessions.push(WorkspaceSession {
            session_id: String::new(),
            name: "locked-mutation".into(),
            goal: "created under lock".into(),
            scope: String::new(),
            status: SessionStatus::Pending,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
        });
        reg.save(&workspace).unwrap();

        // Drop the lock
        drop(_lock);
        drop(reg);

        // Now another lock can be acquired
        let (_reg2, _lock2) = WorkspaceRegistry::load_locked(&workspace).unwrap();
        assert_eq!(_reg2.sessions.len(), 1);
        assert_eq!(_reg2.sessions[0].name, "locked-mutation");
    }

    // ── Concurrent mutation tests ──────────────────────────────────

    #[test]
    fn concurrent_mutations_preserve_all_entries() {
        let (dir, _reg_path) = temp_workspace();
        let workspace = Arc::new(dir.path().to_path_buf());
        let num_threads = 8;

        let mut handles = vec![];
        for i in 0..num_threads {
            let ws = workspace.clone();
            handles.push(std::thread::spawn(move || {
                let name = format!("thread-{}", i);
                let (mut reg, _lock) = WorkspaceRegistry::load_locked(&ws).unwrap();
                reg.sessions.push(WorkspaceSession {
                    session_id: String::new(),
                    name,
                    goal: format!("entry from thread {}", i),
                    scope: String::new(),
                    status: SessionStatus::Pending,
                    pids: vec![],
                    tags: vec![format!("t{}", i)],
                    started: String::new(),
                    completed: String::new(),
                });
                reg.save(&ws).unwrap();
                // _lock dropped here
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // All entries should be present — none lost to race
        let reg = WorkspaceRegistry::load(&workspace).unwrap();
        assert_eq!(reg.sessions.len(), num_threads,
            "expected {} entries, got {} — mutations were lost to a race",
            num_threads, reg.sessions.len());

        let mut names: Vec<_> = reg.sessions.iter().map(|s| s.name.clone()).collect();
        names.sort();
        for i in 0..num_threads {
            assert_eq!(names[i], format!("thread-{}", i));
        }
    }

    #[test]
    fn concurrent_mutations_without_lock_can_lose_state() {
        // This test demonstrates WHY the lock is necessary.
        // Without locks, concurrent read-modify-write can corrupt the file
        // (empty reads, parse failures) or silently lose entries.
        let (dir, _reg_path) = temp_workspace();
        let workspace = Arc::new(dir.path().to_path_buf());
        let num_threads = 8;

        let mut handles = vec![];
        for i in 0..num_threads {
            let ws = workspace.clone();
            handles.push(std::thread::spawn(move || {
                let name = format!("unlocked-{}", i);
                let mut reg = WorkspaceRegistry::load(&ws)
                    .unwrap_or_else(|_| WorkspaceRegistry::empty());
                reg.sessions.push(WorkspaceSession {
                    session_id: String::new(),
                    name,
                    goal: "unlocked entry".into(),
                    scope: String::new(),
                    status: SessionStatus::Pending,
                    pids: vec![],
                    tags: vec![],
                    started: String::new(),
                    completed: String::new(),
                });
                let _ = reg.save(&ws);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // Without locks, the file is often corrupted or entries are lost.
        // We just verify the locked version works correctly — this test
        // exists to document the race condition that load_locked prevents.
        let reg = WorkspaceRegistry::load(&workspace).unwrap_or_else(|_| {
            // File was corrupted by concurrent writes — exactly what the lock prevents
            WorkspaceRegistry::empty()
        });
        eprintln!(
            "unlocked concurrent test: {}/{} entries survived ({} = expected with locking)",
            reg.sessions.len(),
            num_threads,
            num_threads
        );
        // No assertion on count — the file may be corrupt, partially written,
        // or missing entries. This is expected without locking.
    }
}
