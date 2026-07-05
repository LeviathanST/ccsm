/// Which AI coding agent ccsm targets.
///
/// Controls which binary is spawned, which home-level config directories
/// are used, and how session files are found/parsed.
///
/// Detection order:
///   1. `--consumer`/`-C` CLI flag (explicit)
///   2. `CCSM_CONSUMER` env var (`"claude"`, `"pi"`, or `"codewhale"`)
///   3. Auto-detect: prefer the most recently active config directory

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
    /// CodeWhale — binary `codewhale`, config at `~/.codewhale/`.
    /// Sessions: `~/.codewhale/sessions/<uuid>.json` (flat), state DB at `~/.codewhale/state.db`.
    CodeWhale,
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
        let codewhale_dir = home.join(".codewhale");

        let candidates = [
            (Self::Pi, &pi_dir),
            (Self::Claude, &claude_dir),
            (Self::CodeWhale, &codewhale_dir),
        ];

        let mut found: Vec<(Self, std::time::SystemTime)> = candidates
            .iter()
            .filter(|(_, dir)| dir.is_dir())
            .map(|(consumer, dir)| (*consumer, Self::recent_file(dir)))
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
                if cwd.join(".codewhale").is_dir() {
                    return Self::CodeWhale;
                }
            }
            return Self::Claude; // default fallback
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
            "codewhale" => Ok(Self::CodeWhale),
            other => anyhow::bail!("unknown consumer '{other}'. Expected: claude, pi, codewhale"),
        }
    }

    pub fn is_claude(&self) -> bool {
        matches!(self, Self::Claude)
    }

    pub fn is_pi(&self) -> bool {
        matches!(self, Self::Pi)
    }

    pub fn is_codewhale(&self) -> bool {
        matches!(self, Self::CodeWhale)
    }

    // ── Binary ────────────────────────────────────────────────────

    /// The CLI binary to spawn for `resume`.
    pub fn binary(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Pi => "pi",
            Self::CodeWhale => "codewhale",
        }
    }

    /// The config subdirectory under `$HOME`.
    pub fn home_config_dir(self) -> &'static str {
        match self {
            Self::Claude => ".claude",
            Self::Pi => ".pi",
            Self::CodeWhale => ".codewhale",
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
            Self::CodeWhale => {
                // CodeWhale uses the workspace path directly (no slugification)
                workspace.to_string_lossy().to_string()
            }
        }
    }

    // ── Session directories ──────────────────────────────────────

    /// Directory containing live session files (Claude: PID-based JSON, Pi: workspace dirs).
    pub fn sessions_dir(self, home: &Path) -> PathBuf {
        match self {
            Self::Claude => home.join(".claude").join("sessions"),
            Self::Pi => home.join(".pi").join("agent").join("sessions"),
            Self::CodeWhale => home.join(".codewhale").join("sessions"),
        }
    }

    /// Directory containing transcript/session data for a project slug.
    pub fn projects_dir(self, home: &Path, slug: &str) -> PathBuf {
        match self {
            Self::Claude => home.join(".claude").join("projects").join(slug),
            Self::Pi => home.join(".pi").join("agent").join("sessions").join(slug),
            Self::CodeWhale => {
                // CodeWhale stores sessions globally (flat directory), not per-project
                home.join(".codewhale").join("sessions")
            }
        }
    }

    /// Like `projects_dir` but computes the correct slug from the workspace path.
    pub fn projects_dir_for(self, home: &Path, workspace: &Path) -> PathBuf {
        self.projects_dir(home, &self.project_slug(workspace))
    }

    /// Path to a live session file for a given PID (Claude only; Pi and CodeWhale don't use PID files).
    pub fn live_session_file(self, home: &Path, pid: u32) -> Option<PathBuf> {
        match self {
            Self::Claude => {
                Some(home.join(self.home_config_dir()).join("sessions").join(format!("{pid}.json")))
            }
            Self::Pi => None,           // Pi doesn't write PID-based session files
            Self::CodeWhale => None,    // CodeWhale doesn't write PID-based session files
        }
    }

    /// For Claude: direct transcript path by session UUID.
    /// For Pi: search by UUID in the workspace slug directory.
    /// For CodeWhale: search `~/.codewhale/sessions/` for `<uuid>.json` file or
    /// read JSON content to match `metadata.id`.
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
            Self::CodeWhale => {
                // Try direct file path first (fast path)
                let dir = self.sessions_dir(home);
                let direct = dir.join(format!("{session_id}.json"));
                if direct.exists() {
                    return Some(direct);
                }
                // Fallback: scan files for matching metadata.id
                if !dir.is_dir() {
                    return None;
                }
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().is_some_and(|ext| ext == "json") {
                            if let Ok(contents) = std::fs::read_to_string(&path) {
                                if let Ok(meta) = serde_json::from_str::<CodeWhaleSessionFile>(&contents) {
                                    if meta.metadata.id == session_id || meta.metadata.id.starts_with(session_id) {
                                        return Some(path);
                                    }
                                }
                            }
                        }
                    }
                }
                None
            }
        }
    }

    /// Like `find_session_file` but computes the correct slug from the workspace path.
    pub fn find_session_file_for(self, home: &Path, workspace: &Path, session_id: &str) -> Option<PathBuf> {
        self.find_session_file(home, &self.project_slug(workspace), session_id)
    }

    /// List all session/transcript files for a workspace slug, sorted by filename.
    /// Claude/Pi: `.jsonl` files. CodeWhale: `.json` files.
    pub fn list_session_files(self, home: &Path, slug: &str) -> Vec<PathBuf> {
        let dir = self.projects_dir(home, slug);
        if !dir.is_dir() {
            return vec![];
        }
        let ext = match self {
            Self::CodeWhale => "json",
            _ => "jsonl",
        };
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|e| {
                let path = e.ok()?.path();
                if path.extension().is_some_and(|ext2| ext2 == ext) {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();
        files.sort();
        files
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
            Self::CodeWhale => write!(f, "codewhale"),
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

// ── CodeWhale Session file reader ────────────────────────────────────

/// Minimal representation of a saved CodeWhale session JSON file.
#[derive(Debug, Deserialize)]
pub struct CodeWhaleSessionFile {
    pub schema_version: Option<u32>,
    pub metadata: CodeWhaleSessionMeta,
}

#[derive(Debug, Deserialize)]
pub struct CodeWhaleSessionMeta {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Metadata extracted from a CodeWhale session file.
#[derive(Debug, Clone, Default)]
pub struct CodeWhaleSessionMetaExtracted {
    pub session_id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub workspace: String,
}

/// Read a CodeWhale session file and extract metadata.
pub fn read_codewhale_session_meta(path: &Path) -> anyhow::Result<CodeWhaleSessionMetaExtracted> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("opening CodeWhale session file: {}", path.display()))?;
    let session: CodeWhaleSessionFile = serde_json::from_str(&contents)
        .with_context(|| format!("parsing CodeWhale session file: {}", path.display()))?;

    Ok(CodeWhaleSessionMetaExtracted {
        session_id: session.metadata.id.clone(),
        title: session.metadata.title.unwrap_or_default(),
        created_at: session.metadata.created_at.unwrap_or_default(),
        updated_at: session.metadata.updated_at.unwrap_or_default(),
        workspace: session.metadata.workspace.unwrap_or_default(),
    })
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consumer_parse() {
        assert_eq!(Consumer::parse("claude").unwrap(), Consumer::Claude);
        assert_eq!(Consumer::parse("pi").unwrap(), Consumer::Pi);
        assert_eq!(Consumer::parse("codewhale").unwrap(), Consumer::CodeWhale);
        assert!(Consumer::parse("unknown").is_err());
    }

    #[test]
    fn test_binary_names() {
        assert_eq!(Consumer::Claude.binary(), "claude");
        assert_eq!(Consumer::Pi.binary(), "pi");
        assert_eq!(Consumer::CodeWhale.binary(), "codewhale");
    }

    #[test]
    fn test_home_config_dirs() {
        assert_eq!(Consumer::Claude.home_config_dir(), ".claude");
        assert_eq!(Consumer::Pi.home_config_dir(), ".pi");
        assert_eq!(Consumer::CodeWhale.home_config_dir(), ".codewhale");
    }

    #[test]
    fn test_is_methods() {
        assert!(Consumer::Claude.is_claude());
        assert!(!Consumer::Claude.is_pi());
        assert!(!Consumer::Claude.is_codewhale());

        assert!(!Consumer::Pi.is_claude());
        assert!(Consumer::Pi.is_pi());
        assert!(!Consumer::Pi.is_codewhale());

        assert!(!Consumer::CodeWhale.is_claude());
        assert!(!Consumer::CodeWhale.is_pi());
        assert!(Consumer::CodeWhale.is_codewhale());
    }

    #[test]
    fn test_display() {
        assert_eq!(Consumer::Claude.to_string(), "claude");
        assert_eq!(Consumer::Pi.to_string(), "pi");
        assert_eq!(Consumer::CodeWhale.to_string(), "codewhale");
    }

    #[test]
    fn test_sessions_dir() {
        let home = std::path::Path::new("/home/user");
        assert_eq!(
            Consumer::CodeWhale.sessions_dir(home),
            home.join(".codewhale").join("sessions")
        );
    }

    #[test]
    fn test_projects_dir() {
        let home = std::path::Path::new("/home/user");
        // CodeWhale projects_dir is the same as sessions_dir (flat storage)
        assert_eq!(
            Consumer::CodeWhale.projects_dir(home, "any-slug"),
            Consumer::CodeWhale.sessions_dir(home)
        );
    }

    #[test]
    fn test_live_session_file() {
        let home = std::path::Path::new("/home/user");
        assert!(Consumer::CodeWhale.live_session_file(home, 12345).is_none());
    }

    #[test]
    fn test_system_prompt_tags() {
        let (open, close) = Consumer::CodeWhale.system_prompt_tags();
        assert_eq!(open, "<system-reminder>");
        assert_eq!(close, "</system-reminder>");
    }

    #[test]
    fn test_project_slug() {
        let ws = std::path::Path::new("/home/user/my-project");
        // CodeWhale uses raw path (no slugification)
        let slug = Consumer::CodeWhale.project_slug(ws);
        assert_eq!(slug, "/home/user/my-project");
    }

    #[test]
    fn test_codewhale_session_file_roundtrip() {
        // Test with a known-good JSON structure
        let json = r#"{
            "schema_version": 1,
            "metadata": {
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "title": "Test Session",
                "created_at": "2026-07-05T12:00:00Z",
                "updated_at": "2026-07-05T13:00:00Z",
                "workspace": "/home/user/project"
            },
            "messages": []
        }"#;

        let session: CodeWhaleSessionFile = serde_json::from_str(json).unwrap();
        assert_eq!(session.schema_version, Some(1));
        assert_eq!(session.metadata.id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(session.metadata.title.as_deref(), Some("Test Session"));
        assert_eq!(
            session.metadata.workspace.as_deref(),
            Some("/home/user/project")
        );
    }

    #[test]
    fn test_codewhale_session_file_minimal() {
        // Test with minimal fields (title and workspace can be missing)
        let json = r#"{
            "schema_version": 1,
            "metadata": {
                "id": "550e8400-e29b-41d4-a716-446655440000"
            },
            "messages": []
        }"#;

        let session: CodeWhaleSessionFile = serde_json::from_str(json).unwrap();
        assert_eq!(session.metadata.id, "550e8400-e29b-41d4-a716-446655440000");
        assert!(session.metadata.title.is_none());
        assert!(session.metadata.workspace.is_none());
    }
}
