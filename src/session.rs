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
        if self.name.is_empty() {
            "unnamed"
        } else {
            &self.name
        }
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
pub fn load_all(
    sessions_dir: &std::path::Path,
    workspace: Option<&std::path::Path>,
) -> Result<Vec<Session>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(
        pid: u32,
        session_id: &str,
        cwd: &str,
        name: &str,
        started_at: u64,
        updated_at: Option<u64>,
    ) -> Session {
        Session {
            pid,
            session_id: session_id.to_string(),
            cwd: cwd.to_string(),
            name: name.to_string(),
            status: "in_progress".to_string(),
            started_at,
            updated_at,
            kind: None,
            version: None,
        }
    }

    fn session_json(
        pid: u32,
        session_id: &str,
        cwd: &str,
        name: &str,
        started_at: u64,
        updated_at: Option<u64>,
    ) -> String {
        let mut map = serde_json::Map::new();
        map.insert("pid".into(), serde_json::json!(pid));
        map.insert("sessionId".into(), serde_json::json!(session_id));
        map.insert("cwd".into(), serde_json::json!(cwd));
        map.insert("name".into(), serde_json::json!(name));
        map.insert("startedAt".into(), serde_json::json!(started_at));
        if let Some(ua) = updated_at {
            map.insert("updatedAt".into(), serde_json::json!(ua));
        }
        serde_json::to_string(&serde_json::Value::Object(map)).unwrap()
    }

    // ── display_name ────────────────────────────────────────────────

    #[test]
    fn display_name_returns_name_when_not_empty() {
        let s = make_session(1, "s1", "/tmp", "my-session", 1000, Some(2000));
        assert_eq!(s.display_name(), "my-session");
    }

    #[test]
    fn display_name_returns_unnamed_when_empty() {
        let s = make_session(1, "s1", "/tmp", "", 1000, Some(2000));
        assert_eq!(s.display_name(), "unnamed");
    }

    // ── cwd_short ───────────────────────────────────────────────────

    #[test]
    fn cwd_short_returns_basename() {
        let s = make_session(1, "s1", "/home/user/project", "s1", 1000, Some(2000));
        assert_eq!(s.cwd_short(), "project");
    }

    #[test]
    fn cwd_short_handles_root() {
        let s = make_session(1, "s1", "/", "s1", 1000, Some(2000));
        assert_eq!(s.cwd_short(), "/");
    }

    #[test]
    fn cwd_short_returns_original_if_no_basename() {
        let s = make_session(1, "s1", "", "s1", 1000, Some(2000));
        assert_eq!(s.cwd_short(), "");
    }

    // ── started_at_display ──────────────────────────────────────────

    #[test]
    fn started_at_display_formats_millis_to_secs() {
        let s = make_session(1, "s1", "/tmp", "s1", 5000, Some(6000));
        assert_eq!(s.started_at_display(), "ts_5");
    }

    #[test]
    fn started_at_display_rounds_down() {
        let s = make_session(1, "s1", "/tmp", "s1", 1999, Some(3000));
        assert_eq!(s.started_at_display(), "ts_1");
    }

    // ── load_all ────────────────────────────────────────────────────

    #[test]
    fn load_all_returns_empty_when_dir_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist");
        let sessions = load_all(&path, None).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn load_all_filters_incomplete_sessions() {
        let dir = tempfile::tempdir().unwrap();
        // Complete session
        fs_write(
            dir.path().join("complete.json"),
            &session_json(1, "s1", "/proj", "s1", 1000, Some(3000)),
        );
        // Incomplete session (no updatedAt)
        fs_write(
            dir.path().join("incomplete.json"),
            &session_json(2, "s2", "/proj", "s2", 2000, None),
        );
        let sessions = load_all(dir.path(), None).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s1");
    }

    #[test]
    fn load_all_filters_by_workspace() {
        let dir = tempfile::tempdir().unwrap();
        fs_write(
            dir.path().join("match.json"),
            &session_json(1, "s1", "/home/proj", "s1", 1000, Some(1000)),
        );
        fs_write(
            dir.path().join("subdir.json"),
            &session_json(2, "s2", "/home/proj/src", "s2", 1000, Some(2000)),
        );
        fs_write(
            dir.path().join("similar.json"),
            &session_json(3, "s3", "/home/proj-other", "s3", 1000, Some(3000)),
        );
        fs_write(
            dir.path().join("other.json"),
            &session_json(4, "s4", "/home/other", "s4", 1000, Some(4000)),
        );
        let ws = std::path::Path::new("/home/proj");
        let sessions = load_all(dir.path(), Some(ws)).unwrap();
        assert_eq!(sessions.len(), 2);
        for s in &sessions {
            assert!(s.cwd == "/home/proj" || s.cwd.starts_with("/home/proj/"));
        }
    }

    #[test]
    fn load_all_sorts_by_updated_at_descending() {
        let dir = tempfile::tempdir().unwrap();
        fs_write(
            dir.path().join("a.json"),
            &session_json(1, "a", "/proj", "a", 100, Some(100)),
        );
        fs_write(
            dir.path().join("b.json"),
            &session_json(2, "b", "/proj", "b", 100, Some(300)),
        );
        fs_write(
            dir.path().join("c.json"),
            &session_json(3, "c", "/proj", "c", 100, Some(200)),
        );
        let sessions = load_all(dir.path(), None).unwrap();
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].session_id, "b");
        assert_eq!(sessions[1].session_id, "c");
        assert_eq!(sessions[2].session_id, "a");
    }

    #[test]
    fn load_all_skips_non_json_files() {
        let dir = tempfile::tempdir().unwrap();
        fs_write(
            dir.path().join("session.json"),
            &session_json(1, "s1", "/proj", "s1", 1000, Some(2000)),
        );
        fs_write(dir.path().join("notes.txt"), "not json");
        fs_write(dir.path().join("data"), "not json");
        let sessions = load_all(dir.path(), None).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s1");
    }

    fn fs_write(path: std::path::PathBuf, contents: &str) {
        std::fs::write(&path, contents).unwrap_or_else(|e| {
            panic!("failed to write {}: {e}", path.display());
        });
    }

    #[test]
    fn load_all_skips_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        fs_write(dir.path().join("bad.json"), "this is not valid json {{{{");
        fs_write(
            dir.path().join("good.json"),
            &session_json(1, "s1", "/proj", "s1", 1000, Some(2000)),
        );
        let sessions = load_all(dir.path(), None).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s1");
    }

    #[test]
    fn load_all_skips_unreadable_file() {
        let dir = tempfile::tempdir().unwrap();
        // Create a dangling symlink that can't be read
        #[cfg(unix)]
        {
            let bad_path = dir.path().join("bad.json");
            let nowhere = dir.path().join("nowhere");
            std::os::unix::fs::symlink(&nowhere, &bad_path).ok();
        }
        fs_write(
            dir.path().join("good.json"),
            &session_json(1, "s1", "/proj", "s1", 1000, Some(2000)),
        );
        let sessions = load_all(dir.path(), None).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s1");
    }
}
