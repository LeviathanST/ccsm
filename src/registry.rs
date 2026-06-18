use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

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
    pub fn build(sessions_dir: &std::path::Path) -> Result<Self> {
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
                    .map(format_ts)
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
    pub fn load_or_build(global_path: &std::path::Path, sessions_dir: &std::path::Path) -> Result<Self> {
        if global_path.exists()
            && let Ok(contents) = std::fs::read_to_string(global_path)
            && let Ok(reg) = serde_json::from_str::<GlobalRegistry>(&contents)
        {
            return Ok(reg);
        }
        let reg = Self::build(sessions_dir)?;
        reg.save(global_path)?;
        Ok(reg)
    }

    pub fn save(&self, path: &std::path::Path) -> Result<()> {
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

/// A retired Claude session — kept for history when `ccsm refresh` swaps
/// out a stale session for a fresh one within the same ccsm session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetiredSession {
    pub id: String,
    pub retired_at: String,
    pub reason: String,
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
    /// Retired Claude session_ids — one ccsm session may chain through
    /// multiple Claude sessions as the context window fills up.
    #[serde(default)]
    pub retired_session_ids: Vec<RetiredSession>,
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
    pub fn load(repo_path: &std::path::Path) -> Result<Self> {
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
    pub fn load_locked(repo_path: &std::path::Path) -> Result<(Self, LockFile)> {
        let lock = LockFile::acquire(repo_path)?;
        let reg = Self::load(repo_path)?;
        Ok((reg, lock))
    }

    /// Save back to disk.
    pub fn save(&self, repo_path: &std::path::Path) -> Result<()> {
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
                    if path.extension().is_none_or(|e| e != "json") {
                        continue;
                    }
                    if let Ok(contents) = std::fs::read_to_string(&path)
                        && contents.contains(session_id) {
                            let _ = std::fs::remove_file(&path);
                        }
                }
            }
        }

        // Delete the detail file
        let detail = workspace
            .join(".claude")
            .join("sessions")
            .join(format!("{name}.md"));
        let _ = std::fs::remove_file(&detail);

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
                    if path.extension().is_none_or(|e| e != "json") {
                        continue;
                    }
                    if let Ok(contents) = std::fs::read_to_string(&path)
                        && contents.contains(session_id) {
                            if let Ok(meta) = std::fs::metadata(&path) {
                                freed += meta.len();
                            }
                            let _ = std::fs::remove_file(&path);
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
                retired_session_ids: vec![],
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
                retired_session_ids: vec![],
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
                retired_session_ids: vec![],
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
                retired_session_ids: vec![],
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
    pub fn acquire(repo_path: &std::path::Path) -> Result<Self> {
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

pub fn now_iso() -> String {
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

/// Simple Levenshtein distance — used to suggest corrections for typos.
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev = (0..=b.len()).collect::<Vec<_>>();
    let mut curr = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

pub(crate) fn note_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let secs_per_day: u64 = 86400;
    let days = secs / secs_per_day;
    let day_secs = secs % secs_per_day;
    let hours = day_secs / 3600;
    let mins = (day_secs % 3600) / 60;

    let (y, m, d) = days_to_date(days);
    format!("{:04}-{:02}-{:02} {:02}:{:02}Z", y, m, d, hours, mins)
}

/// Convert days since 1970-01-01 to (year, month, day).
fn days_to_date(mut days: u64) -> (u32, u32, u32) {
    let mut year: u32 = 1970;
    loop {
        let diy: u64 = if is_leap(year) { 366 } else { 365 };
        if days < diy { break; }
        days -= diy;
        year += 1;
    }
    let mdays: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month: u32 = 1;
    for &md in &mdays {
        if days < md { break; }
        days -= md;
        month += 1;
    }
    (year, month, (days + 1) as u32)
}

fn is_leap(y: u32) -> bool {
    y.is_multiple_of(4) && !y.is_multiple_of(100) || y.is_multiple_of(400)
}

/// Insert `new_entry` into the Progress Log section of `contents`.
/// Prepends (newest at top) — inserts right after the `## Progress Log`
/// header, past any blank lines or HTML comments.
pub(crate) fn insert_note(contents: &str, new_entry: &str) -> String {
    let lines: Vec<&str> = contents.lines().collect();

    if let Some(hdr) = lines.iter().position(|l| l.trim() == "## Progress Log") {
        let mut ins = hdr + 1;
        let mut comment = false;
        while ins < lines.len() {
            let t = lines[ins].trim();
            if t.is_empty() {
                ins += 1;
            } else if t.starts_with("<!--") {
                comment = true;
                ins += 1;
            } else if comment && (t == "-->" || t.ends_with("-->")) {
                comment = false;
                ins += 1;
            } else if comment {
                ins += 1;
            } else {
                break;
            }
        }

        let mut out = String::with_capacity(contents.len() + new_entry.len() + 2);
        for line in &lines[..ins] {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(new_entry);
        if ins < lines.len() { out.push('\n'); }
        for line in &lines[ins..] {
            out.push_str(line);
            out.push('\n');
        }
        out
    } else {
        let mut out = contents.to_string();
        if !out.ends_with('\n') { out.push('\n'); }
        out.push('\n');
        out.push_str("## Progress Log\n\n");
        out.push_str(new_entry);
        out.push('\n');
        out
    }
}

/// Replace the body of a `## SectionName` in a markdown string.
pub(crate) fn replace_detail_section(md: &str, header: &str, new_body: &str) -> String {
    let lines: Vec<&str> = md.lines().collect();

    let hdr_idx = lines.iter().position(|l| {
        let t = l.trim();
        t == header || t.starts_with(&format!("{} ", header))
    });

    match hdr_idx {
        Some(hdr) => {
            let end = lines[hdr + 1..]
                .iter()
                .position(|l| l.starts_with("## "))
                .map(|p| hdr + 1 + p)
                .unwrap_or(lines.len());

            let mut out = String::new();
            for line in &lines[..=hdr] {
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
            out.push_str(new_body);
            if end < lines.len() {
                out.push('\n');
            }
            for line in &lines[end..] {
                out.push_str(line);
                out.push('\n');
            }
            out
        }
        None => {
            let mut out = md.to_string();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&format!("\n{}\n\n{}\n", header, new_body));
            out
        }
    }
}

pub(crate) fn is_kebab_case(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub(crate) fn harvest_from_pid(home: &std::path::Path, pid: u32) -> anyhow::Result<String> {
    let session_file = home.join(".claude").join("sessions").join(format!("{pid}.json"));
    if !session_file.exists() {
        anyhow::bail!(
            "no session file at {}\n  Is PID {} running?",
            session_file.display(), pid
        );
    }
    let contents = std::fs::read_to_string(&session_file)
        .context("reading session file")?;
    let s: crate::session::Session = serde_json::from_str(&contents)
        .context("parsing session file")?;
    if s.session_id.is_empty() {
        anyhow::bail!("session file for PID {} has no sessionId yet", pid);
    }
    Ok(s.session_id)
}

pub(crate) fn validate_session_id(sid: &str) -> anyhow::Result<()> {
    let parts: Vec<&str> = sid.split('-').collect();
    if parts.len() == 5
        && parts[0].len() == 8
        && parts[1].len() == 4
        && parts[2].len() == 4
        && parts[3].len() == 4
        && parts[4].len() == 12
        && sid.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
    {
        Ok(())
    } else {
        anyhow::bail!(
            "'{}' does not look like a session UUID (e.g. f493397b-...-4d5f15da0311).\n\
             If you renamed the session in the TUI, the name changed but the UUID didn't.\n\
             Use --pid <pid> instead: ccsm attach {} --pid <pid>",
            sid, sid
        );
    }
}

pub(crate) fn parse_sections(md: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut current_header: Option<String> = None;
    let mut current_body = String::new();

    for line in md.lines() {
        if line.starts_with("## ") {
            if let Some(h) = current_header.take() {
                sections.push((h, std::mem::take(&mut current_body)));
            }
            current_header = Some(line.strip_prefix("## ").unwrap().trim().to_string());
        } else if current_header.is_some() {
            if !current_body.is_empty() {
                current_body.push('\n');
            }
            current_body.push_str(line);
        }
    }
    if let Some(h) = current_header
        && (!current_body.trim().is_empty() || sections.iter().any(|(_, b)| !b.trim().is_empty())) {
            sections.push((h, current_body));
        }
    sections
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
                retired_session_ids: vec![],
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
            retired_session_ids: vec![],
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
                    retired_session_ids: vec![],
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
                    retired_session_ids: vec![],
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
