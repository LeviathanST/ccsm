use crate::ErrorCode;
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
    /// OpenCode — binary `opencode`, config at `~/.config/opencode/`.
    /// Sessions in SQLite at `~/.local/share/opencode/opencode.db`.
    /// No PID-based session files.
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
        // OpenCode check: look for the SQLite DB
        let opencode_db = home
            .join(".local")
            .join("share")
            .join("opencode")
            .join("opencode.db");

        let candidates: [(Self, &Path); 3] = [
            (Self::OpenCode, &opencode_db),
            (Self::Pi, &pi_dir),
            (Self::Claude, &claude_dir),
        ];

        let mut found: Vec<(Self, std::time::SystemTime)> = candidates
            .iter()
            .filter(|(_, path)| path.exists())
            .map(|(consumer, path)| {
                let mtime = if path.is_dir() {
                    Self::recent_file(path)
                } else {
                    path.metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH)
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
                if let Ok(meta) = entry.metadata()
                    && let Ok(mtime) = meta.modified()
                    && mtime > latest
                {
                    latest = mtime;
                }
                // Check one level deeper for session dirs
                if entry.path().is_dir()
                    && let Ok(sub) = std::fs::read_dir(entry.path())
                {
                    for sub_entry in sub.flatten() {
                        if let Ok(meta) = sub_entry.metadata()
                            && let Ok(mtime) = meta.modified()
                            && mtime > latest
                        {
                            latest = mtime;
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
            other => anyhow::bail!(
                "{} unknown consumer '{other}'. Expected: claude, pi, opencode",
                ErrorCode::Invalid
            ),
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
    pub fn binary(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
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
            Self::Claude => workspace
                .to_string_lossy()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect(),
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
            Self::Claude => Some(
                home.join(self.home_config_dir())
                    .join("sessions")
                    .join(format!("{pid}.json")),
            ),
            Self::Pi | Self::OpenCode => None,
        }
    }

    /// For Claude: direct transcript path by session UUID.
    /// For Pi: search by UUID in the workspace slug directory.
    pub fn find_session_file(self, home: &Path, slug: &str, session_id: &str) -> Option<PathBuf> {
        match self {
            Self::Claude => {
                let path = self
                    .projects_dir(home, slug)
                    .join(format!("{session_id}.jsonl"));
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
                        if let Some(name) = path.file_name().and_then(|n| n.to_str())
                            && name.contains(session_id)
                            && name.ends_with(".jsonl")
                        {
                            return Some(path);
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
    pub fn find_session_file_for(
        self,
        home: &Path,
        workspace: &Path,
        session_id: &str,
    ) -> Option<PathBuf> {
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
        "CONSTRAINT:\n  - Work within this scope. If you need to do something outside it, ask first.\n  - If the goal or scope is ambiguous, do NOT guess — ask targeted clarifying questions.\n  - Do NOT start work until you can answer WHAT is being built and WHY."
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
    if let Some(name) = path.file_stem().and_then(|n| n.to_str())
        && let Some(uuid_part) = name.split('_').nth(1)
    {
        meta.session_id = uuid_part.to_string();
    }

    // Read lines to find user messages that might reveal the session name
    for line in reader.lines().flatten().take(200) {
        if let Ok(entry) = serde_json::from_str::<PiSessionEntry>(&line)
            && let PiSessionEntry::Message {
                message: Some(msg), ..
            } = entry
            && msg.role.as_deref() == Some("user")
            && let Some(blocks) = &msg.content
        {
            for block in blocks {
                if let PiContentBlock::Text { text } = block
                    && meta.name.is_empty()
                    && let Some(n) = extract_session_name(text)
                {
                    meta.name = n;
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
    let triggers = [
        "work on ",
        "implement ",
        "fix ",
        "add ",
        "refactor ",
        "build ",
    ];
    let lower = text.to_lowercase();
    for trigger in &triggers {
        if let Some(pos) = lower.find(trigger) {
            let after = &text[pos + trigger.len()..];
            let name = after
                .split(['.', '!', '?', '\n'])
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
    home.join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db")
}

/// Check if a session exists in opencode's database.
pub fn opencode_session_exists(db_path: &Path, session_id: &str) -> bool {
    use rusqlite::Connection;
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    conn.query_row("SELECT 1 FROM session WHERE id = ?1", [session_id], |_| {
        Ok(())
    })
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
        .map_err(|e| anyhow::anyhow!("{} failed to open opencode DB: {e}", ErrorCode::Invalid))?;
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
    )
    .ok()
}

/// Find a session for a directory created after a timestamp (single query, no polling).
/// Unlike `opencode_harvest_session`, this does NOT loop — call when the session
/// is expected to already exist (e.g. after the agent child has exited).
pub fn opencode_find_session_since(
    db_path: &Path,
    directory: &str,
    since_ts: i64,
) -> Option<String> {
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
        assert_eq!(Consumer::OpenCode.binary(), "opencode");
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
        let db_path = dir.path().join("opencode.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_test123', 'Greeting', '/tmp', 1000);",
        )
        .unwrap();

        opencode_update_title(&db_path, "ses_test123", "swarm-mcp").unwrap();

        let title: String = conn
            .query_row(
                "SELECT title FROM session WHERE id = 'ses_test123'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title, "swarm-mcp");
    }

    #[test]
    fn test_opencode_update_title_nonexistent_id_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
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
        let db_path = dir.path().join("opencode.db");
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
    fn test_detect_explicit() {
        let home = Path::new("/tmp");
        assert_eq!(Consumer::detect(home, Some("claude")), Consumer::Claude);
        assert_eq!(Consumer::detect(home, Some("pi")), Consumer::Pi);
        assert_eq!(Consumer::detect(home, Some("opencode")), Consumer::OpenCode);
        assert_eq!(
            Consumer::detect(home, Some("open-code")),
            Consumer::OpenCode
        );
        assert_eq!(Consumer::detect(home, Some("oc")), Consumer::OpenCode);
    }

    #[test]
    fn test_binary_name_returns_opencode() {
        assert_eq!(Consumer::OpenCode.binary(), "opencode");
    }

    #[test]
    fn test_binary_name_returns_claude() {
        assert_eq!(Consumer::Claude.binary(), "claude");
    }

    #[test]
    fn test_projects_dir_for_different_paths() {
        let home = Path::new("/home/user");
        let workspace = Path::new("/home/user/projects/my-app");
        let claude_path = Consumer::Claude.projects_dir_for(home, workspace);
        let opencode_path = Consumer::OpenCode.projects_dir_for(home, workspace);

        assert_ne!(claude_path, opencode_path);
        assert_eq!(
            claude_path,
            home.join(".claude")
                .join("projects")
                .join("-home-user-projects-my-app")
        );
        assert_eq!(
            opencode_path,
            home.join(".local").join("share").join("opencode")
        );
    }

    #[test]
    fn test_home_config_dirs_returns_opencode_path() {
        assert_eq!(Consumer::OpenCode.home_config_dir(), ".config/opencode");
    }

    #[test]
    fn test_is_opencode_returns_true_for_opencode() {
        assert!(Consumer::OpenCode.is_opencode());
        assert!(!Consumer::Claude.is_opencode());
        assert!(!Consumer::Pi.is_opencode());
    }

    #[test]
    fn test_consumer_clone() {
        let c = Consumer::OpenCode;
        let cloned = c;
        assert_eq!(c, cloned);
        let cloned2 = c;
        assert_eq!(c, cloned2);
    }

    #[test]
    fn test_consumer_debug() {
        assert_eq!(format!("{:?}", Consumer::Claude), "Claude");
        assert_eq!(format!("{:?}", Consumer::Pi), "Pi");
        assert_eq!(format!("{:?}", Consumer::OpenCode), "OpenCode");
    }

    #[test]
    fn test_consumer_display() {
        assert_eq!(format!("{}", Consumer::Claude), "claude");
        assert_eq!(format!("{}", Consumer::Pi), "pi");
        assert_eq!(format!("{}", Consumer::OpenCode), "opencode");
    }

    // ── slugify_path ───────────────────────────────────────────────

    #[test]
    fn test_slugify_path_normal() {
        assert_eq!(
            slugify_path(Path::new("/home/user/projects/my-app")),
            "home-user-projects-my-app"
        );
    }

    #[test]
    fn test_slugify_path_consecutive_special_chars() {
        assert_eq!(
            slugify_path(Path::new("/home//user///projects")),
            "home-user-projects"
        );
    }

    #[test]
    fn test_slugify_path_leading_trailing_slashes() {
        assert_eq!(slugify_path(Path::new("/home/user/")), "home-user");
    }

    #[test]
    fn test_slugify_path_root() {
        assert_eq!(slugify_path(Path::new("/")), "");
    }

    #[test]
    fn test_slugify_path_only_special() {
        assert_eq!(slugify_path(Path::new("///")), "");
    }

    #[test]
    fn test_slugify_path_already_clean() {
        assert_eq!(slugify_path(Path::new("hello")), "hello");
    }

    #[test]
    fn test_slugify_path_empty() {
        assert_eq!(slugify_path(Path::new("")), "");
    }

    #[test]
    fn test_slugify_path_with_dots() {
        assert_eq!(
            slugify_path(Path::new("/home/user/my.cool.app")),
            "home-user-my-cool-app"
        );
    }

    // ── extract_session_name ───────────────────────────────────────

    #[test]
    fn test_extract_session_work_on() {
        assert_eq!(
            extract_session_name("work on my-feature"),
            Some("my-feature".into())
        );
    }

    #[test]
    fn test_extract_session_implement() {
        assert_eq!(
            extract_session_name("implement login form."),
            Some("login-form".into())
        );
    }

    #[test]
    fn test_extract_session_fix() {
        assert_eq!(
            extract_session_name("fix bug in auth!"),
            Some("bug-in-auth".into())
        );
    }

    #[test]
    fn test_extract_session_add() {
        assert_eq!(
            extract_session_name("add tests for consumer\nand more"),
            Some("tests-for-consumer".into())
        );
    }

    #[test]
    fn test_extract_session_refactor() {
        assert_eq!(
            extract_session_name("refactor auth module?"),
            Some("auth-module".into())
        );
    }

    #[test]
    fn test_extract_session_build() {
        assert_eq!(
            extract_session_name("build the thing"),
            Some("the-thing".into())
        );
    }

    #[test]
    fn test_extract_session_no_trigger() {
        assert_eq!(extract_session_name("random message without trigger"), None);
    }

    #[test]
    fn test_extract_session_trigger_alone() {
        assert_eq!(extract_session_name("work on "), None);
    }

    #[test]
    fn test_extract_session_empty_text() {
        assert_eq!(extract_session_name(""), None);
    }

    #[test]
    fn test_extract_session_long_truncated() {
        let long = format!("add {}", "a".repeat(100));
        let expected: String = "a".repeat(60);
        assert_eq!(extract_session_name(&long), Some(expected));
    }

    #[test]
    fn test_extract_session_case_insensitive() {
        assert_eq!(
            extract_session_name("WORK ON my-feature"),
            Some("my-feature".into())
        );
    }

    // ── opencode_session_exists ─────────────────────────────────────

    #[test]
    fn test_opencode_session_exists_found() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_abc', 'Test', '/tmp', 1000);",
        ).unwrap();
        assert!(opencode_session_exists(&db_path, "ses_abc"));
    }

    #[test]
    fn test_opencode_session_exists_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_abc', 'Test', '/tmp', 1000);",
        ).unwrap();
        assert!(!opencode_session_exists(&db_path, "ses_xyz"));
    }

    #[test]
    fn test_opencode_session_exists_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("nonexistent.db");
        assert!(!opencode_session_exists(&db_path, "anything"));
    }

    // ── opencode_list_sessions ──────────────────────────────────────

    #[test]
    fn test_opencode_list_sessions_empty_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);",
        ).unwrap();
        let sessions = opencode_list_sessions(&db_path);
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_opencode_list_sessions_ordered() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_old', 'Old', '/tmp', 100);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_new', 'New', '/tmp', 200);",
        ).unwrap();
        let sessions = opencode_list_sessions(&db_path);
        assert_eq!(sessions, vec!["ses_new", "ses_old"]);
    }

    #[test]
    fn test_opencode_list_sessions_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("nonexistent.db");
        let sessions = opencode_list_sessions(&db_path);
        assert!(sessions.is_empty());
    }

    // ── opencode_harvest_session ────────────────────────────────────

    #[test]
    fn test_opencode_harvest_session_found() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_new', 'New', '/my/proj', 2000);",
        ).unwrap();
        let result = opencode_harvest_session(&db_path, "/my/proj", 1000);
        assert_eq!(result, Some("ses_new".into()));
    }

    #[test]
    fn test_opencode_harvest_session_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_old', 'Old', '/my/proj', 500);",
        ).unwrap();
        // No session with time_created > 1000 matching directory
        let result = opencode_harvest_session(&db_path, "/my/proj", 1000);
        assert!(result.is_none());
    }

    #[test]
    fn test_opencode_harvest_session_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("nonexistent.db");
        let result = opencode_harvest_session(&db_path, "/my/proj", 1000);
        assert!(result.is_none());
    }

    #[test]
    fn test_opencode_harvest_session_wrong_dir() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_new', 'New', '/other/path', 2000);",
        ).unwrap();
        let result = opencode_harvest_session(&db_path, "/my/proj", 1000);
        assert!(result.is_none());
    }

    // ── opencode_latest_session ─────────────────────────────────────

    #[test]
    fn test_opencode_latest_session_found() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_early', 'Early', '/my/proj', 100);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_late', 'Late', '/my/proj', 200);",
        ).unwrap();
        let result = opencode_latest_session(&db_path, "/my/proj");
        assert_eq!(result, Some("ses_late".into()));
    }

    #[test]
    fn test_opencode_latest_session_no_match() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_one', 'One', '/other', 100);",
        ).unwrap();
        assert!(opencode_latest_session(&db_path, "/no/match").is_none());
    }

    #[test]
    fn test_opencode_latest_session_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("nonexistent.db");
        assert!(opencode_latest_session(&db_path, "/my/proj").is_none());
    }

    #[test]
    fn test_opencode_latest_session_empty_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);",
        ).unwrap();
        assert!(opencode_latest_session(&db_path, "/my/proj").is_none());
    }

    // ── opencode_find_session_since ─────────────────────────────────

    #[test]
    fn test_opencode_find_session_since_found() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_new', 'New', '/my/proj', 2000);",
        ).unwrap();
        let result = opencode_find_session_since(&db_path, "/my/proj", 1000);
        assert_eq!(result, Some("ses_new".into()));
    }

    #[test]
    fn test_opencode_find_session_since_before_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_old', 'Old', '/my/proj', 500);",
        ).unwrap();
        assert!(opencode_find_session_since(&db_path, "/my/proj", 1000).is_none());
    }

    #[test]
    fn test_opencode_find_session_since_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("nonexistent.db");
        assert!(opencode_find_session_since(&db_path, "/my/proj", 1000).is_none());
    }

    #[test]
    fn test_opencode_find_session_since_wrong_dir() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_new', 'New', '/other', 2000);",
        ).unwrap();
        assert!(opencode_find_session_since(&db_path, "/my/proj", 1000).is_none());
    }

    // ── Consumer::project_slug ──────────────────────────────────────

    #[test]
    fn test_project_slug_claude() {
        let slug = Consumer::Claude.project_slug(Path::new("/home/user/my-app"));
        assert_eq!(slug, "-home-user-my-app");
    }

    #[test]
    fn test_project_slug_pi() {
        let slug = Consumer::Pi.project_slug(Path::new("/home/user/my-app"));
        assert_eq!(slug, "--home-user-my-app--");
    }

    #[test]
    fn test_project_slug_pi_empty_path() {
        let slug = Consumer::Pi.project_slug(Path::new("/"));
        assert_eq!(slug, "----");
    }

    #[test]
    fn test_project_slug_opencode() {
        let slug = Consumer::OpenCode.project_slug(Path::new("/home/user/my-app"));
        assert_eq!(slug, "-home-user-my-app");
    }

    #[test]
    fn test_project_slug_pi_consecutive_special() {
        let slug = Consumer::Pi.project_slug(Path::new("/home//user///app"));
        assert_eq!(slug, "--home-user-app--");
    }

    // ── Consumer::list_session_files ─────────────────────────────────

    #[test]
    fn test_list_session_files_claude_with_files() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let projects = home.join(".claude").join("projects").join("my-slug");
        std::fs::create_dir_all(&projects).unwrap();
        std::fs::write(projects.join("aaa.jsonl"), "").unwrap();
        std::fs::write(projects.join("bbb.jsonl"), "").unwrap();
        std::fs::write(projects.join("ccc.txt"), "").unwrap(); // should be ignored
        let files = Consumer::Claude.list_session_files(&home, "my-slug");
        assert_eq!(files.len(), 2);
        assert!(files[0].ends_with("aaa.jsonl"));
        assert!(files[1].ends_with("bbb.jsonl"));
    }

    #[test]
    fn test_list_session_files_claude_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let projects = home.join(".claude").join("projects").join("my-slug");
        std::fs::create_dir_all(&projects).unwrap();
        let files = Consumer::Claude.list_session_files(&home, "my-slug");
        assert!(files.is_empty());
    }

    #[test]
    fn test_list_session_files_claude_no_dir() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let files = Consumer::Claude.list_session_files(&home, "nonexistent");
        assert!(files.is_empty());
    }

    #[test]
    fn test_list_session_files_pi_with_files() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let sessions = home
            .join(".pi")
            .join("agent")
            .join("sessions")
            .join("my-slug");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(sessions.join("100_ses_a.jsonl"), "").unwrap();
        std::fs::write(sessions.join("200_ses_b.jsonl"), "").unwrap();
        let files = Consumer::Pi.list_session_files(&home, "my-slug");
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_list_session_files_pi_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let sessions = home
            .join(".pi")
            .join("agent")
            .join("sessions")
            .join("my-slug");
        std::fs::create_dir_all(&sessions).unwrap();
        let files = Consumer::Pi.list_session_files(&home, "my-slug");
        assert!(files.is_empty());
    }

    #[test]
    fn test_list_session_files_opencode() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let files = Consumer::OpenCode.list_session_files(&home, "any-slug");
        assert!(files.is_empty());
    }

    // ── Consumer::find_session_file ──────────────────────────────────

    #[test]
    fn test_find_session_file_claude_found() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let transcript = home
            .join(".claude")
            .join("projects")
            .join("my-slug")
            .join("uuid-123.jsonl");
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, "").unwrap();
        let result = Consumer::Claude.find_session_file(&home, "my-slug", "uuid-123");
        assert_eq!(result, Some(transcript));
    }

    #[test]
    fn test_find_session_file_claude_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let result = Consumer::Claude.find_session_file(&home, "my-slug", "uuid-999");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_session_file_pi_found() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let slug_dir = home
            .join(".pi")
            .join("agent")
            .join("sessions")
            .join("my-slug");
        std::fs::create_dir_all(&slug_dir).unwrap();
        let file = slug_dir.join("123456789_ses_target_uuid.jsonl");
        std::fs::write(&file, "").unwrap();
        // Noise — different uuid
        std::fs::write(slug_dir.join("987654321_ses_other_uuid.jsonl"), "").unwrap();
        let result = Consumer::Pi.find_session_file(&home, "my-slug", "target_uuid");
        assert_eq!(result, Some(file));
    }

    #[test]
    fn test_find_session_file_pi_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let slug_dir = home
            .join(".pi")
            .join("agent")
            .join("sessions")
            .join("my-slug");
        std::fs::create_dir_all(&slug_dir).unwrap();
        std::fs::write(slug_dir.join("123_ses_other.jsonl"), "").unwrap();
        let result = Consumer::Pi.find_session_file(&home, "my-slug", "nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_session_file_pi_no_dir() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let result = Consumer::Pi.find_session_file(&home, "my-slug", "uuid-123");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_session_file_opencode_found() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let db_dir = home.join(".local").join("share").join("opencode");
        std::fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_found', 'Found', '/tmp', 1000);",
        ).unwrap();
        let result = Consumer::OpenCode.find_session_file(&home, "any-slug", "ses_found");
        assert_eq!(result, Some(db_path));
    }

    #[test]
    fn test_find_session_file_opencode_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let db_dir = home.join(".local").join("share").join("opencode");
        std::fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_created INTEGER);\
             INSERT INTO session (id, title, directory, time_created) VALUES ('ses_existing', 'Existing', '/tmp', 1000);",
        ).unwrap();
        let result = Consumer::OpenCode.find_session_file(&home, "any-slug", "ses_missing");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_session_file_opencode_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let result = Consumer::OpenCode.find_session_file(&home, "any-slug", "ses_any");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_session_file_pi_substring_match() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let slug_dir = home
            .join(".pi")
            .join("agent")
            .join("sessions")
            .join("my-slug");
        std::fs::create_dir_all(&slug_dir).unwrap();
        let file = slug_dir.join("100_ses_abc_def.jsonl");
        std::fs::write(&file, "").unwrap();
        // Search with just a substring of the uuid
        let result = Consumer::Pi.find_session_file(&home, "my-slug", "abc_def");
        assert_eq!(result, Some(file));
    }
}
