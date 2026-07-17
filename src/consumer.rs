/// Which AI coding agent ccsm targets.
///
/// Controls which binary is spawned, which home-level config directories
/// are used, and how session files are found/parsed.
///
/// Detection order:
///   1. `--consumer`/`-C` CLI flag (explicit)
///   2. `CCSM_CONSUMER` env var (`"claude"` or `"pi"`)
///   3. Auto-detect: prefer Pi if its config dir is more recent

use anyhow::Context;
use serde::Deserialize;
use std::path::{Path, PathBuf};

// ── Consumer enum ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consumer {
    /// Claude Code — binary `claude`, config at `~/.claude/`.
    /// Sessions: `~/.claude/sessions/<pid>.json`, transcripts: `~/.claude/projects/<slug>/`.
    Claude,
    /// Pi — binary `pi`, config at `~/.pi/agent/`.
    /// Sessions: `~/.pi/agent/sessions/<slug>/<ts>_<uuid>.jsonl`.
    Pi,
    /// OpenCode — binary `opencode2` (V2 beta), config at `~/.config/opencode/`.
    /// Sessions in SQLite at `~/.local/share/opencode/opencode-next.db`.
    /// Override with `OPENCODE_DB` env var. No PID-based session files.
    OpenCode,
}

impl Consumer {
    /// Detect the target consumer.  See module docs for order.
    pub fn detect(home: &Path, explicit: Option<&str>) -> Self {
        if let Some(val) = explicit {
            return Self::parse(val).unwrap_or_else(|_| {
                eprintln!("warning: unknown consumer '{val}', falling back to auto-detect");
                Self::auto_detect(home)
            });
        }

        // Env var override
        if let Ok(val) = std::env::var("CCSM_CONSUMER") {
            return Self::parse(&val).unwrap_or_else(|_| {
                eprintln!("warning: CCSM_CONSUMER='{val}' unknown, falling back to auto-detect");
                Self::auto_detect(home)
            });
        }

        Self::auto_detect(home)
    }

    fn auto_detect(home: &Path) -> Self {
        let pi_dir = home.join(".pi").join("agent");
        let claude_dir = home.join(".claude");
        // OpenCode check: look for the SQLite DB (prefer V2 over V1)
        // NOTE: When V2 ships as stable, `opencode-next.db` → `opencode.db`
        let opencode_base = home.join(".local").join("share").join("opencode");
        let opencode_db = {
            let v2 = opencode_base.join("opencode-next.db");
            let v1 = opencode_base.join("opencode.db");
            if v2.exists() { v2 } else { v1 }
        };

        let candidates: [(Self, &Path); 3] = [(Self::OpenCode, &opencode_db), (Self::Pi, &pi_dir), (Self::Claude, &claude_dir)];

        let mut found: Vec<(Self, std::time::SystemTime)> = candidates
            .iter()
            .filter(|(_, path)| path.exists())
            .map(|(consumer, path)| {
                let mtime = if path.is_dir() {
                    Self::recent_file(path)
                } else {
                    path.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH)
                };
                (*consumer, mtime)
            })
            .collect();

        if found.is_empty() {
            // Fallback: check cwd for project-local config dirs
            if let Ok(cwd) = std::env::current_dir() {
                if cwd.join(".pi").is_dir() {
                    return Self::Pi;
                }
                if cwd.join(".claude").is_dir() {
                    return Self::Claude;
                }
                // Prefer OpenCode on fresh systems
                return Self::OpenCode;
            }
            return Self::OpenCode; // default fallback
        }

        // Return the one with most recent activity
        found.sort_by(|a, b| b.1.cmp(&a.1));
        found.into_iter().next().unwrap().0
    }

    /// Find the most recent modification time in a directory tree (2 levels deep).
    fn recent_file(dir: &Path) -> std::time::SystemTime {
        let mut latest = std::time::UNIX_EPOCH;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if mtime > latest {
                            latest = mtime;
                        }
                    }
                }
                // Check one level deeper for session dirs
                if entry.path().is_dir() {
                    if let Ok(sub) = std::fs::read_dir(entry.path()) {
                        for sub_entry in sub.flatten() {
                            if let Ok(meta) = sub_entry.metadata() {
                                if let Ok(mtime) = meta.modified() {
                                    if mtime > latest {
                                        latest = mtime;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        latest
    }

    fn parse(s: &str) -> anyhow::Result<Self> {
        match s.to_lowercase().as_str() {
            "claude" => Ok(Self::Claude),
            "pi" => Ok(Self::Pi),
            "opencode" | "open-code" | "oc" => Ok(Self::OpenCode),
            other => anyhow::bail!("unknown consumer '{other}'. Expected: claude, pi, opencode"),
        }
    }

    pub fn is_claude(&self) -> bool {
        matches!(self, Self::Claude)
    }

    pub fn is_pi(&self) -> bool {
        matches!(self, Self::Pi)
    }

    pub fn is_opencode(&self) -> bool {
        matches!(self, Self::OpenCode)
    }

    // ── Binary ────────────────────────────────────────────────────

    /// The CLI binary to spawn for `resume`.
    /// NOTE: V2 ships as `opencode2` during beta; revert to `opencode` on stable.
    pub fn binary(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Pi => "pi",
            Self::OpenCode => "opencode2",
        }
    }

    /// The config subdirectory under `$HOME`.
    pub fn home_config_dir(self) -> &'static str {
        match self {
            Self::Claude => ".claude",
            Self::Pi => ".pi",
            Self::OpenCode => ".config/opencode",
        }
    }

    /// Data directory for session storage (e.g. SQLite DB).
    /// Only meaningful for OpenCode; Claude/Pi store sessions elsewhere.
    pub fn data_dir(self) -> &'static str {
        match self {
            Self::OpenCode => ".local/share/opencode",
            _ => "",
        }
    }

    // ── Project slug ────────────────────────────────────────────

    /// Derive the project slug used by this agent to namespace session data.
    /// Claude: all non-alphanumeric → `-`, raw, e.g. `-home-user-my-project-`
    /// Pi:     collapsed, stripped, wrapped in `--...--`, e.g. `--home-user-my-project--`
    pub fn project_slug(self, workspace: &Path) -> String {
        match self {
            Self::Claude => {
                workspace
                    .to_string_lossy()
                    .chars()
                    .map(|c| if c.is_alphanumeric() { c } else { '-' })
                    .collect()
            }
            Self::Pi => {
                let base = slugify_path(workspace);
                format!("--{}--", base)
            }
            Self::OpenCode => {
                // Deterministic path-based slug for ccsm matching
                workspace
                    .to_string_lossy()
                    .chars()
                    .map(|c| if c.is_alphanumeric() { c } else { '-' })
                    .collect()
            }
        }
    }

    // ── Session directories ──────────────────────────────────────

    /// Directory containing live session files (Claude: PID-based JSON, Pi: workspace dirs).
    pub fn sessions_dir(self, home: &Path) -> PathBuf {
        match self {
            Self::Claude => home.join(".claude").join("sessions"),
            Self::Pi => home.join(".pi").join("agent").join("sessions"),
            Self::OpenCode => home.join(".local").join("share").join("opencode"),
        }
    }

    /// Directory containing transcript/session data for a project slug.
    pub fn projects_dir(self, home: &Path, slug: &str) -> PathBuf {
        match self {
            Self::Claude => home.join(".claude").join("projects").join(slug),
            Self::Pi => home.join(".pi").join("agent").join("sessions").join(slug),
            Self::OpenCode => home.join(".local").join("share").join("opencode"),
        }
    }

    /// Like `projects_dir` but computes the correct slug from the workspace path.
    pub fn projects_dir_for(self, home: &Path, workspace: &Path) -> PathBuf {
        self.projects_dir(home, &self.project_slug(workspace))
    }

    /// Path to a live session file for a given PID (Claude only; Pi doesn't use PID files).
    pub fn live_session_file(self, home: &Path, pid: u32) -> Option<PathBuf> {
        match self {
            Self::Claude => {
                Some(home.join(self.home_config_dir()).join("sessions").join(format!("{pid}.json")))
            }
            Self::Pi | Self::OpenCode => None,
        }
    }

    /// For Claude: direct transcript path by session UUID.
    /// For Pi: search by UUID in the workspace slug directory.
    pub fn find_session_file(self, home: &Path, slug: &str, session_id: &str) -> Option<PathBuf> {
        match self {
            Self::Claude => {
                let path = self.projects_dir(home, slug).join(format!("{session_id}.jsonl"));
                if path.exists() { Some(path) } else { None }
            }
            Self::Pi => {
                let dir = self.projects_dir(home, slug);
                if !dir.is_dir() {
                    return None;
                }
                // Pi files: <timestamp>_<uuid>.jsonl — search for uuid substring
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if name.contains(session_id) && name.ends_with(".jsonl") {
                                return Some(path);
                            }
                        }
                    }
                }
                None
            }
            Self::OpenCode => {
                let db_path = opencode_db_path(home);
                if opencode_session_exists(&db_path, session_id) {
                    Some(db_path) // DB file exists — signals "found"
                } else {
                    None
                }
            }
        }
    }

    /// Like `find_session_file` but computes the correct slug from the workspace path.
    pub fn find_session_file_for(self, home: &Path, workspace: &Path, session_id: &str) -> Option<PathBuf> {
        self.find_session_file(home, &self.project_slug(workspace), session_id)
    }

    /// List all session/transcript files for a workspace slug, sorted by filename.
    pub fn list_session_files(self, home: &Path, slug: &str) -> Vec<PathBuf> {
        match self {
            Self::OpenCode => vec![], // No individual transcript files — all in DB
            _ => {
                let dir = self.projects_dir(home, slug);
                if !dir.is_dir() {
                    return vec![];
                }
                let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .filter_map(|e| {
                        let path = e.ok()?.path();
                        if path.extension().is_some_and(|ext| ext == "jsonl") {
                            Some(path)
                        } else {
                            None
                        }
                    })
                    .collect();
                files.sort();
                files
            }
        }
    }

    // ── System prompt format ─────────────────────────────────────

    /// System prompt wrapper tags.
    pub fn system_prompt_tags(self) -> (&'static str, &'static str) {
        ("<system-reminder>", "</system-reminder>")
    }

    /// Constraint line appended to injected scope.
    pub fn constraint_line(self) -> &'static str {
        "CONSTRAINT: Work within this scope. If you need to do something outside it, ask first."
    }
}

impl std::fmt::Display for Consumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Claude => write!(f, "claude"),
            Self::Pi => write!(f, "pi"),
            Self::OpenCode => write!(f, "opencode"),
        }
    }
}

// ── Pi Session file reader ─────────────────────────────────────────

/// Minimal representation of a Pi session entry.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PiSessionEntry {
    Message {
        #[serde(rename = "type")]
        entry_type: String,
        id: Option<String>,
        timestamp: Option<String>,
        message: Option<PiMessage>,
    },
    Other(serde_json::Value),
}

#[derive(Debug, Deserialize)]
pub struct PiMessage {
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<Vec<PiContentBlock>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PiContentBlock {
    Text { text: String },
    Other(serde_json::Value),
}

/// Metadata extracted from a Pi session file.
#[derive(Debug, Clone, Default)]
pub struct PiSessionMeta {
    pub session_id: String,
    pub name: String,
    pub started_at: u64,
    pub updated_at: Option<u64>,
}

/// Read a Pi session file and extract metadata.
/// Pi session files are JSONL — we read the first few lines to find context.
pub fn read_pi_session_meta(path: &Path) -> anyhow::Result<PiSessionMeta> {
    use std::io::BufRead;

    let file = std::fs::File::open(path)
        .with_context(|| format!("opening Pi session file: {}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut meta = PiSessionMeta::default();

    // Extract UUID from filename: <timestamp>_<uuid>.jsonl
    if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
        if let Some(uuid_part) = name.split('_').nth(1) {
            meta.session_id = uuid_part.to_string();
        }
    }

    // Read lines to find user messages that might reveal the session name
    for line in reader.lines().flatten().take(200) {
        if let Ok(entry) = serde_json::from_str::<PiSessionEntry>(&line) {
            if let PiSessionEntry::Message {
                message: Some(msg), ..
            } = entry
            {
                if msg.role.as_deref() == Some("user") {
                    if let Some(blocks) = &msg.content {
                        for block in blocks {
                            if let PiContentBlock::Text { text } = block {
                                if meta.name.is_empty() {
                                    if let Some(n) = extract_session_name(text) {
                                        meta.name = n;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(meta)
}

/// Collapse consecutive dashes and strip leading/trailing ones,
/// matching Pi's slugify logic in .pi/extensions/ccsm/index.ts
///   `cwd.replace(/[^a-zA-Z0-9]/g, "-").replace(/-+/g, "-").replace(/^-|-$/g, "")`
fn slugify_path(path: &Path) -> String {
    // Step 1: replace non-alphanumeric with '-'
    let s: String = path
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    // Step 2: collapse consecutive dashes
    let mut collapsed = String::with_capacity(s.len());
    let mut prev_was_dash = false;
    for ch in s.chars() {
        if ch == '-' {
            if prev_was_dash {
                continue;
            }
            prev_was_dash = true;
        } else {
            prev_was_dash = false;
        }
        collapsed.push(ch);
    }
    // Step 3: strip leading/trailing dashes
    collapsed.trim_matches('-').to_string()
}

/// Crude extraction of session name from a user prompt.
fn extract_session_name(text: &str) -> Option<String> {
    let triggers = ["work on ", "implement ", "fix ", "add ", "refactor ", "build "];
    let lower = text.to_lowercase();
    for trigger in &triggers {
        if let Some(pos) = lower.find(trigger) {
            let after = &text[pos + trigger.len()..];
            let name = after
                .split(|c: char| c == '.' || c == '!' || c == '?' || c == '\n')
                .next()
                .unwrap_or(after)
                .trim()
                .chars()
                .take(60)
                .collect::<String>();
            if !name.is_empty() {
                return Some(name.to_lowercase().replace(' ', "-"));
            }
        }
    }
    None
}

// ── OpenCode DB helpers ─────────────────────────────────────────────

/// Path to opencode's SQLite session database.
pub fn opencode_db_path(home: &Path) -> PathBuf {
    if let Ok(env_path) = std::env::var("OPENCODE_DB") {
        return PathBuf::from(env_path);
    }
    home.join(".local")
        .join("share")
        .join("opencode")
        .join("opencode-next.db")
}

/// Check if a session exists in opencode's database.
pub fn opencode_session_exists(db_path: &Path, session_id: &str) -> bool {
    use rusqlite::Connection;
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    conn.query_row(
        "SELECT 1 FROM session WHERE id = ?1",
        [session_id],
        |_| Ok(()),
    )
    .is_ok()
}

/// List all session IDs in opencode's database.
pub fn opencode_list_sessions(db_path: &Path) -> Vec<String> {
    use rusqlite::Connection;
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut stmt = match conn.prepare("SELECT id FROM session ORDER BY time_created DESC") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], |row| row.get::<_, String>(0))
        .into_iter()
        .flatten()
        .filter_map(|r| r.ok())
        .collect()
}

/// Harvest a newly created opencode session ID by polling the DB.
/// Returns the first session with `time_created > before` and matching `directory`.
/// Polls up to ~5s (50 attempts × 100ms).
pub fn opencode_harvest_session(db_path: &Path, directory: &str, before_ts: i64) -> Option<String> {
    use rusqlite::Connection;
    for _ in 0..50 {
        let conn = Connection::open(db_path).ok()?;
        let mut stmt = conn
            .prepare("SELECT id FROM session WHERE directory = ?1 AND time_created > ?2 ORDER BY time_created DESC LIMIT 1")
            .ok()?;
        let result: Option<String> = stmt
            .query_map([directory, &before_ts.to_string()], |row| row.get(0))
            .ok()?
            .filter_map(|r| r.ok())
            .next();
        if result.is_some() {
            return result;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    None
}

/// Update the title of an opencode session in the DB.
pub fn opencode_update_title(db_path: &Path, session_id: &str, title: &str) -> anyhow::Result<()> {
    use rusqlite::Connection;
    let conn = Connection::open(db_path)
        .map_err(|e| anyhow::anyhow!("failed to open opencode DB: {e}"))?;
    conn.execute(
        "UPDATE session SET title = ?1 WHERE id = ?2",
        [title, session_id],
    )?;
    Ok(())
}

/// Read the title of an opencode session from the DB.
pub fn opencode_get_title(db_path: &Path, session_id: &str) -> Option<String> {
    use rusqlite::Connection;
    let conn = Connection::open(db_path).ok()?;
    conn.query_row(
        "SELECT title FROM session WHERE id = ?1",
        [session_id],
        |row| row.get(0),
    ).ok()
}

/// Find a session for a directory created after a timestamp (single query, no polling).
/// Unlike `opencode_harvest_session`, this does NOT loop — call when the session
/// is expected to already exist (e.g. after the agent child has exited).
pub fn opencode_find_session_since(db_path: &Path, directory: &str, since_ts: i64) -> Option<String> {
    use rusqlite::Connection;
    let conn = Connection::open(db_path).ok()?;
    let mut stmt = conn
        .prepare("SELECT id FROM session WHERE directory = ?1 AND time_created > ?2 ORDER BY time_created DESC LIMIT 1")
        .ok()?;
    stmt.query_map([directory, &since_ts.to_string()], |row| row.get(0))
        .ok()?
        .filter_map(|r| r.ok())
        .next()
}

/// Get the most recent session ID for a directory (non-polling, single check).
pub fn opencode_latest_session(db_path: &Path, directory: &str) -> Option<String> {
    use rusqlite::Connection;
    let conn = Connection::open(db_path).ok()?;
    let mut stmt = conn
        .prepare("SELECT id FROM session WHERE directory = ?1 ORDER BY time_created DESC LIMIT 1")
        .ok()?;
    stmt.query_map([directory], |row| row.get(0))
        .ok()?
        .filter_map(|r| r.ok())
        .next()
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consumer_parse() {
        assert_eq!(Consumer::parse("claude").unwrap(), Consumer::Claude);
        assert_eq!(Consumer::parse("pi").unwrap(), Consumer::Pi);
        assert_eq!(Consumer::parse("opencode").unwrap(), Consumer::OpenCode);
        assert_eq!(Consumer::parse("open-code").unwrap(), Consumer::OpenCode);
        assert_eq!(Consumer::parse("oc").unwrap(), Consumer::OpenCode);
        assert!(Consumer::parse("unknown").is_err());
    }

    #[test]
    fn test_binary_names() {
        assert_eq!(Consumer::Claude.binary(), "claude");
        assert_eq!(Consumer::Pi.binary(), "pi");
        assert_eq!(Consumer::OpenCode.binary(), "opencode2");
    }

    #[test]
    fn test_home_config_dirs() {
        assert_eq!(Consumer::Claude.home_config_dir(), ".claude");
        assert_eq!(Consumer::Pi.home_config_dir(), ".pi");
        assert_eq!(Consumer::OpenCode.home_config_dir(), ".config/opencode");
    }

    #[test]
    fn test_is_opencode() {
        assert!(Consumer::OpenCode.is_opencode());
        assert!(!Consumer::Claude.is_opencode());
        assert!(!Consumer::Pi.is_opencode());
    }

    #[test]
    fn test_data_dir() {
        assert_eq!(Consumer::OpenCode.data_dir(), ".local/share/opencode");
        assert_eq!(Consumer::Claude.data_dir(), "");
    }

    #[test]
    fn test_opencode_update_title() {
        use rusqlite::Connection;
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode-next.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_test123', 'Greeting', '/tmp', 1000);",
        )
        .unwrap();

        opencode_update_title(&db_path, "ses_test123", "swarm-mcp").unwrap();

        let title: String = conn
            .query_row("SELECT title FROM session WHERE id = 'ses_test123'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "swarm-mcp");
    }

    #[test]
    fn test_opencode_update_title_nonexistent_id_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode-next.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);",
        )
        .unwrap();

        // SQLite UPDATE with no matching WHERE is not an error — 0 rows affected.
        assert!(opencode_update_title(&db_path, "ses_nonexistent", "test").is_ok());
    }

    #[test]
    fn test_opencode_get_title() {
        use rusqlite::Connection;
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode-next.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_test123', 'Original Title', '/tmp', 1000);",
        )
        .unwrap();

        let title = opencode_get_title(&db_path, "ses_test123").expect("should find title");
        assert_eq!(title, "Original Title");

        let missing = opencode_get_title(&db_path, "ses_nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_opencode_db_path_env_var_override() {
        let dir = tempfile::tempdir().unwrap();
        let custom_db = dir.path().join("custom.db");
        // SAFETY: tests run single-threaded; env var is set/cleared within this scope
        unsafe { std::env::set_var("OPENCODE_DB", &custom_db) };
        let result = opencode_db_path(dir.path());
        assert_eq!(result, custom_db);
        unsafe { std::env::remove_var("OPENCODE_DB") };
    }

    #[test]
    fn test_opencode_db_path_default() {
        let dir = tempfile::tempdir().unwrap();
        let result = opencode_db_path(dir.path());
        let expected = dir.path().join(".local").join("share").join("opencode").join("opencode-next.db");
        assert_eq!(result, expected);
    }
}
