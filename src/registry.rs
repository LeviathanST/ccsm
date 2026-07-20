use crate::ErrorCode;
use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

// ── Workspace Identity ────────────────────────────────────────────

/// Workspace identity loaded from the `.ccsm` TOML file at project root.
///
/// `version` is the ccsm version that created this identity (from Cargo.toml).
/// On upgrade, migration code checks this field to run version-specific migrations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceIdentity {
    pub version: String,
    pub id: String,
}

/// Resolved workspace context for the current invocation.
pub struct WorkspaceContext {
    pub id: String,
    pub root: PathBuf,
    pub slug: String,
}

/// Home directory used for `~/.ccsm/` resolution. Override via `HOME` env var.
pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// Global data directory for a workspace.
/// Default: `$HOME/.ccsm/<id>/`.
/// Override: `CCSM_DATA_DIR` env var sets a custom base (path is `<CCSM_DATA_DIR>/<id>/`).
pub fn global_data_dir(id: &str) -> PathBuf {
    let base = std::env::var("CCSM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".ccsm"));
    base.join(id)
}

/// Path to the session registry: `~/.ccsm/<id>/sessions.json`
pub fn global_registry_path(id: &str) -> PathBuf {
    global_data_dir(id).join("sessions.json")
}

/// Path to the lock file: `~/.ccsm/<id>/sessions.json.lock`
pub fn global_lock_path(id: &str) -> PathBuf {
    global_data_dir(id).join("sessions.json.lock")
}

/// Path to a session detail file: `~/.ccsm/<id>/sessions/<name>.md`
pub fn global_detail_path(id: &str, name: &str) -> PathBuf {
    global_data_dir(id)
        .join("sessions")
        .join(format!("{name}.md"))
}

/// Path to the session detail template: `~/.ccsm/<id>/session-detail-template.md`
pub fn global_template_path(id: &str) -> PathBuf {
    global_data_dir(id).join("session-detail-template.md")
}

/// Path to a group detail file: `~/.ccsm/<id>/session-group/<name>.md`
pub fn global_group_path(id: &str, name: &str) -> PathBuf {
    global_data_dir(id)
        .join("session-group")
        .join(format!("{name}.md"))
}

/// Path to a worktree: `~/.ccsm/<id>/worktrees/<name>/`
pub fn global_worktree_path(id: &str, name: &str) -> PathBuf {
    global_data_dir(id).join("worktrees").join(name)
}

/// Path to the project config: `~/.ccsm/<id>/config.toml`
pub fn global_config_path(id: &str) -> PathBuf {
    global_data_dir(id).join("config.toml")
}

/// Walk up from `start` looking for a `.ccsm` identity file.
/// Returns the directory containing the file and its parsed contents.
pub fn find_project_root(start: &Path) -> Result<Option<(PathBuf, WorkspaceIdentity)>> {
    let mut current = Some(start);
    while let Some(dir) = current {
        let ccsm_file = dir.join(".ccsm");
        if ccsm_file.is_file() {
            let content = std::fs::read_to_string(&ccsm_file)
                .with_context(|| format!("reading {}", ccsm_file.display()))?;
            let identity: WorkspaceIdentity = toml::from_str(&content).with_context(|| {
                format!(
                    "parsing {} — expected `version` and `id` fields",
                    ccsm_file.display()
                )
            })?;
            return Ok(Some((dir.to_path_buf(), identity)));
        }
        current = dir.parent();
    }
    Ok(None)
}

/// Walk up from CWD to find the project root and workspace identity.
/// Errors if no `.ccsm` file exists — use `init_identity()` to create one.
/// On existing identity with stale version, runs version-gated migrations.
/// Also handles legacy `.ccsm/sessions.json/` → identity file migration.
pub fn resolve_identity() -> Result<WorkspaceContext> {
    let cwd = std::env::current_dir()?;

    if let Some((root, identity)) = find_project_root(&cwd)? {
        run_identity_migrations(&identity, &root)?;
        let slug = project_slug(&identity.id);
        return Ok(WorkspaceContext {
            id: identity.id,
            root,
            slug,
        });
    }

    // Check for legacy `.ccsm/sessions.json` to auto-migrate
    let mut current = Some(cwd.as_path());
    while let Some(dir) = current {
        if dir.join(".ccsm").join("sessions.json").exists() {
            let root = dir.to_path_buf();
            let id = uuid_v4();
            eprintln!(
                "ccsm: migrating from {}/.ccsm/ to ~/.ccsm/{id}/",
                root.display()
            );
            let ccsm_path = root.join(".ccsm");
            migrate_legacy_data(&root, &id)?;
            if ccsm_path.is_dir() {
                std::fs::remove_dir_all(&ccsm_path)?;
            }
            let content = format!(
                "version = \"{}\"\nid = \"{id}\"\n",
                env!("CARGO_PKG_VERSION")
            );
            std::fs::write(&ccsm_path, &content).context("writing .ccsm identity file")?;
            let slug = project_slug(&id);
            return Ok(WorkspaceContext { id, root, slug });
        }
        current = dir.parent();
    }

    anyhow::bail!(
        "{} no .ccsm identity file found in this project.\n\
         Run `ccsm init` to set up session tracking in the current directory.",
        ErrorCode::NoSession
    );
}

/// Create a `.ccsm` identity file at the nearest git root (or CWD).
/// Idempotent — won't overwrite an existing identity.
/// Also ensures the global data directory exists.
pub fn init_identity() -> Result<WorkspaceContext> {
    let cwd = std::env::current_dir()?;

    if let Some((root, identity)) = find_project_root(&cwd)? {
        eprintln!("ccsm: .ccsm identity already exists at {}", root.display());
        let slug = project_slug(&identity.id);
        return Ok(WorkspaceContext {
            id: identity.id,
            root,
            slug,
        });
    }

    let root = find_nearest_git_root(&cwd).unwrap_or(cwd);
    let id = uuid_v4();
    let ccsm_path = root.join(".ccsm");
    if ccsm_path.is_dir() {
        std::fs::remove_dir_all(&ccsm_path)?;
    }
    if !ccsm_path.exists() {
        let content = format!(
            "version = \"{}\"\nid = \"{id}\"\n",
            env!("CARGO_PKG_VERSION")
        );
        std::fs::write(&ccsm_path, &content).context("writing .ccsm identity file")?;
    }
    let slug = project_slug(&id);
    ensure_data_dir(&id)?;
    eprintln!("ccsm: initialised workspace {id} at {}", root.display());
    Ok(WorkspaceContext { id, root, slug })
}

static MIGRATIONS_RAN: AtomicBool = AtomicBool::new(false);

/// Run version-gated migrations when the `.ccsm` identity file is from an older version.
/// Add new migration arms here as ccsm evolves.
/// Guarded by MIGRATIONS_RAN to avoid re-prompting when multiple code paths
/// call resolve_identity() in a single process.
fn run_identity_migrations(identity: &WorkspaceIdentity, root: &Path) -> Result<()> {
    if MIGRATIONS_RAN.swap(true, Ordering::Relaxed) {
        return Ok(());
    }

    let current = env!("CARGO_PKG_VERSION");
    if identity.version == current {
        return Ok(());
    }
    match identity.version.as_str() {
        "1" => {
            // Old hardcoded version from pre-0.15.0 dev — update to semver
            let content = format!("version = \"{current}\"\nid = \"{}\"\n", identity.id);
            std::fs::write(root.join(".ccsm"), &content)
                .context("rewriting .ccsm identity with current version")?;
            eprintln!(
                "ccsm: migrated .ccsm identity from v{} to v{}",
                identity.version, current
            );
        }
        "0.15.0" => {
            // Strip stale worktree field from registry (field removed in 0.16.0)
            if let Err(e) = strip_stale_worktree(identity) {
                eprintln!("ccsm: warning: failed to strip worktree fields from registry: {e}");
            }
            let content = format!("version = \"{current}\"\nid = \"{}\"\n", identity.id);
            std::fs::write(root.join(".ccsm"), &content)
                .context("rewriting .ccsm identity with current version")?;
            eprintln!(
                "ccsm: migrated .ccsm identity from v{} to v{} (stale worktree fields stripped)",
                identity.version, current
            );
        }
        _ => {
            // Unknown version — warn, don't block. The hard safety guard
            // (binary < project) is handled by check_version() in main.rs.
            // Binary > project is safe — the chain runner handles upgrades.
            eprintln!(
                "ccsm: .ccsm identity version \"{}\" doesn't match (expected {}). Run `ccsm migrate` to update.",
                identity.version, current,
            );
        }
    }
    Ok(())
}

/// Migrate legacy `<project>/.ccsm/` data to `~/.ccsm/<id>/`.
pub fn migrate_legacy_data(root: &Path, id: &str) -> Result<()> {
    ensure_data_dir(id)?;
    let src = root.join(".ccsm");

    // sessions.json
    let src_json = src.join("sessions.json");
    let dst_json = global_registry_path(id);
    if src_json.exists() {
        std::fs::copy(&src_json, &dst_json).context("copying legacy sessions.json")?;
    }

    // sessions/ detail files
    let src_sessions = src.join("sessions");
    let dst_sessions = global_data_dir(id).join("sessions");
    if src_sessions.is_dir() {
        std::fs::create_dir_all(&dst_sessions).context("creating global sessions dir")?;
        if let Ok(entries) = std::fs::read_dir(&src_sessions) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    let name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("");
                    let dst = global_detail_path(id, name);
                    std::fs::copy(&path, &dst)
                        .with_context(|| format!("copying detail file {}", path.display()))?;
                }
            }
        }
    }

    // session-group/
    let src_group = src.join("session-group");
    let dst_group = global_data_dir(id).join("session-group");
    if src_group.is_dir() {
        std::fs::create_dir_all(&dst_group).context("creating global session-group dir")?;
        if let Ok(entries) = std::fs::read_dir(&src_group) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    let name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("");
                    let dst = global_group_path(id, name);
                    std::fs::copy(&path, &dst)
                        .with_context(|| format!("copying group file {}", path.display()))?;
                }
            }
        }
    }

    // session-detail-template.md
    let src_tpl = src.join("session-detail-template.md");
    if src_tpl.exists() {
        std::fs::copy(&src_tpl, global_template_path(id)).context("copying template file")?;
    }

    // config.toml
    let src_config = src.join("config.toml");
    if src_config.exists() {
        std::fs::copy(&src_config, global_config_path(id)).context("copying config.toml")?;
    }

    // Delete old .ccsm/ directory (non-critical cleanup)
    let _ = std::fs::remove_dir_all(&src);

    Ok(())
}

/// Strip stale `worktree` field from sessions.json by re-reading and re-saving.
/// In 0.16.0 the `worktree` field was removed from WorkspaceSession — serde
/// ignores it on deserialize and omits it on serialize, so a re-save is enough.
pub(crate) fn strip_stale_worktree(identity: &WorkspaceIdentity) -> Result<()> {
    let reg_path = global_registry_path(&identity.id);
    if !reg_path.exists() {
        return Ok(());
    }
    let contents = std::fs::read_to_string(&reg_path)?;
    let mut reg: WorkspaceRegistry = serde_json::from_str(&contents)
        .context("parsing sessions.json to strip stale worktree fields")?;
    reg.updated = now_iso();

    // Re-save — serde automatically omits the removed `worktree` field
    let new_contents = serde_json::to_string_pretty(&reg)?;
    std::fs::write(&reg_path, new_contents).context("writing cleaned sessions.json")?;
    Ok(())
}

/// Ensure the global data directory structure exists for a workspace.
pub fn ensure_data_dir(id: &str) -> Result<()> {
    let dir = global_data_dir(id);
    std::fs::create_dir_all(dir.join("sessions")).context("creating global sessions dir")?;
    std::fs::create_dir_all(dir.join("session-group"))
        .context("creating global session-group dir")?;
    std::fs::create_dir_all(dir.join("worktrees")).context("creating global worktrees dir")?;
    Ok(())
}

/// Generate a random UUID v4 (no external crate dependency).
pub fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id() as u128;
    let r1 = ts.wrapping_mul(pid).wrapping_add(0xdeadbeef);
    let r2 = ts.wrapping_add(pid).wrapping_mul(0xcafebabe);
    let r3 = r1.wrapping_mul(r2).wrapping_add(0xdecafbad);
    let r4 = r2.wrapping_mul(0x9e3779b9).wrapping_add(ts);
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (r1 & 0xffffffff) as u32,
        ((r2 >> 16) & 0xffff) as u16,
        ((r3 >> 48) & 0x0fff) as u16,
        (0x8000 | ((r4 >> 32) & 0x3fff)) as u16,
        (r3.wrapping_mul(r4) & 0xffffffffffff) as u64,
    )
}

/// Find the nearest git repository root from a starting path.
pub fn find_nearest_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".git").exists() || dir.join(".git").is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

// ── Group ─────────────────────────────────────────────────────────────

/// Ordering within a session group.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(untagged)]
pub enum GroupRank {
    /// No ordering — tie-break alphabetically.
    #[default]
    Free,
    /// Numeric rank — lower = higher priority.
    Number(u32),
}

impl std::fmt::Display for GroupRank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Free => write!(f, "free"),
            Self::Number(n) => write!(f, "{}", n),
        }
    }
}

/// A named group a session belongs to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub rank: GroupRank,
}

// ── Workspace Detail ────────────────────────────────────────────────

/// Per-workspace session registry at `~/.ccsm/<id>/sessions.json`.
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
    /// Which agent owns this session: "claude" or "pi".
    /// Used for cross-agent resume warnings.
    #[serde(default)]
    pub consumer: String,
    /// Group this session belongs to (optional).
    #[serde(default)]
    pub group: Option<Group>,
    /// Session names this session depends on (must complete first).
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Target git branch for this session (optional).
    /// Set with `ccsm new -b <branch>`; checked at resume via inject-scope.
    /// ccsm tracks this as metadata — it does not create or switch branches.
    #[serde(default)]
    pub branch: String,
    /// Whether this session should use a git worktree.
    /// Set with `ccsm new --worktree`; governed by config.worktrees policy.
    #[serde(default)]
    pub use_worktree: bool,
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

/// Allowed status transitions:
///
/// | from → to           | command        |
/// |---------------------|----------------|
/// | pending → in_progress | start       |
/// | in_progress → completed | complete  |
/// | in_progress → blocked  | block     |
/// | in_progress → abandoned | abandon  |
/// | blocked → abandoned    | abandon   |
/// | trashed → in_progress  | recover   |
/// | * → pending         | pending (reset) |
/// | * → trashed         | trash           |
/// | from == to          | (no-op)         |
///
/// All other transitions return `false`.
impl SessionStatus {
    pub fn transition_allowed(from: Self, to: Self) -> bool {
        if from == to {
            return true;
        }
        matches!(
            (from, to),
            (Self::Pending, Self::InProgress)
                | (Self::InProgress, Self::Completed)
                | (Self::InProgress, Self::Blocked)
                | (Self::InProgress, Self::Abandoned)
                | (Self::Blocked, Self::Abandoned)
                | (Self::Trashed, Self::InProgress)
                | (_, Self::Pending)
                | (_, Self::Trashed)
        )
    }
}

impl WorkspaceRegistry {
    /// Load from `~/.ccsm/<id>/sessions.json` where `<id>` is resolved from
    /// the `.ccsm` identity file in the project root (found by walking up from CWD).
    /// Returns an empty registry if no file exists yet (fresh project).
    pub fn load() -> Result<Self> {
        let data_dir = resolve_data_dir()?;
        Self::load_from(&data_dir)
    }

    /// Load from a specific data directory (used for migration and tests).
    pub fn load_from(data_dir: &Path) -> Result<Self> {
        let path = data_dir.join("sessions.json");
        if path.exists() {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let mut reg: WorkspaceRegistry =
                serde_json::from_str(&contents)
                    .with_context(|| format!(
                        "parsing {} — JSON is malformed\n  → check for trailing/missing commas, unclosed brackets, or stray characters\n  → backup or delete the file to start fresh",
                        path.display(),
                    ))?;
            reg.updated = now_iso();
            return Ok(reg);
        }
        Ok(Self {
            updated: now_iso(),
            sessions: Vec::new(),
        })
    }

    /// Load with an exclusive lock held for the lifetime of the returned `LockFile`.
    /// Use this for every read-modify-write cycle to prevent races between
    /// chained `ccsm` mutation commands.
    pub fn load_locked() -> Result<(Self, LockFile)> {
        let data_dir = resolve_data_dir()?;
        let lock = LockFile::acquire_for_data_dir(&data_dir)?;
        let reg = Self::load_from(&data_dir)?;
        Ok((reg, lock))
    }

    /// Load from a specific data directory with an exclusive lock (for tests).
    pub fn load_locked_from(data_dir: &Path) -> Result<(Self, LockFile)> {
        let lock = LockFile::acquire_for_data_dir(data_dir)?;
        let reg = Self::load_from(data_dir)?;
        Ok((reg, lock))
    }

    /// Save to `~/.ccsm/<id>/sessions.json`.
    pub fn save(&self) -> Result<()> {
        let data_dir = resolve_data_dir()?;
        self.save_to(&data_dir)
    }

    /// Save to a specific data directory (used for migration and tests).
    pub fn save_to(&self, data_dir: &Path) -> Result<()> {
        let path = data_dir.join("sessions.json");
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
    /// session files, and the registry entry.
    /// Matches by session_id; falls back to name for seed entries.
    pub fn clean(
        &mut self,
        session_id: &str,
        name: &str,
        home: &std::path::Path,
        workspace: &std::path::Path,
        consumer: crate::consumer::Consumer,
    ) {
        // Only delete files if we have a real session_id
        if !session_id.is_empty() {
            if !consumer.is_opencode() {
                let proj_dir = consumer.projects_dir_for(home, workspace);
                let slug = consumer.project_slug(workspace);
                let transcript = if consumer.is_pi() {
                    consumer
                        .find_session_file(home, &slug, session_id)
                        .unwrap_or_else(|| proj_dir.join(format!("_{session_id}.jsonl")))
                } else {
                    proj_dir.join(format!("{session_id}.jsonl"))
                };
                let _ = std::fs::remove_file(&transcript);
            }

            // Remove any live session files with this session_id
            if let Ok(entries) = std::fs::read_dir(consumer.sessions_dir(home)) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_none_or(|e| e != "json") {
                        continue;
                    }
                    if let Ok(contents) = std::fs::read_to_string(&path)
                        && contents.contains(session_id)
                    {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }

        // Delete the detail file from global data dir
        if let Ok(ctx) = resolve_identity() {
            let detail = global_detail_path(&ctx.id, name);
            let _ = std::fs::remove_file(&detail);
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
        consumer: crate::consumer::Consumer,
    ) -> u64 {
        let mut freed: u64 = 0;
        if !session_id.is_empty() {
            if !consumer.is_opencode() {
                let slug = consumer.project_slug(workspace);
                let transcript = consumer
                    .find_session_file(home, &slug, session_id)
                    .unwrap_or_else(|| {
                        consumer
                            .projects_dir(home, &slug)
                            .join(format!("{session_id}.jsonl"))
                    });
                if transcript.exists() {
                    if let Ok(meta) = std::fs::metadata(&transcript) {
                        freed += meta.len();
                    }
                    let _ = std::fs::remove_file(&transcript);
                }
            }

            // Remove any live session files with this session_id
            if let Ok(entries) = std::fs::read_dir(consumer.sessions_dir(home)) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_none_or(|e| e != "json") {
                        continue;
                    }
                    if let Ok(contents) = std::fs::read_to_string(&path)
                        && contents.contains(session_id)
                    {
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
    pub fn clean_all_trashed(
        &mut self,
        home: &std::path::Path,
        workspace: &std::path::Path,
        consumer: crate::consumer::Consumer,
    ) {
        let trashed: Vec<(String, String)> = self
            .sessions
            .iter()
            .filter(|e| e.status == SessionStatus::Trashed)
            .map(|e| (e.session_id.clone(), e.name.clone()))
            .collect();
        for (sid, name) in &trashed {
            self.clean(sid, name, home, workspace, consumer);
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
                group: None,
                depends_on: vec![],
                branch: String::new(),
                use_worktree: false,
                retired_session_ids: vec![],
                consumer: String::new(),
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
                group: None,
                depends_on: vec![],
                branch: String::new(),
                use_worktree: false,
                retired_session_ids: vec![],
                consumer: String::new(),
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
                group: None,
                depends_on: vec![],
                branch: String::new(),
                use_worktree: false,
                retired_session_ids: vec![],
                consumer: String::new(),
            },
            WorkspaceSession {
                session_id: String::new(),
                name: "session-registry".into(),
                goal: "Global session registry at ~/.ccsm/ with per-project isolation".into(),
                scope: "Global data at ~/.ccsm/<id>/ with sessions.json, detail files, groups, worktrees, and config. Per-project .ccsm identity file for UUID-based workspace resolution. Survives ephemeral agent cleanup.".into(),
                status: SessionStatus::InProgress,
                pids: vec![],
                tags: vec!["registry".into(), "sessions".into(), "team".into()],
                started: String::new(),
                completed: String::new(),
                group: None,
                depends_on: vec![],
                branch: String::new(),
                use_worktree: false,
                retired_session_ids: vec![],
                consumer: String::new(),
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

/// Advisory exclusive lock on `~/.ccsm/<id>/sessions.json.lock`.
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
    /// Acquire a lock within a specific data directory (tests/migration).
    pub fn acquire_for_data_dir(data_dir: &Path) -> Result<Self> {
        let lock_path = data_dir.join("sessions.json.lock");
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

/// Parse a `day{days}T{time}Z` timestamp and return the age in days
/// (0 if unparseable or empty).
pub fn session_age_days(ts: &str) -> u64 {
    if ts.is_empty() {
        return 0;
    }
    let now_days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86400;

    // Parse "day<number>..."
    let stripped = ts
        .strip_prefix("day")
        .and_then(|s| s.split('T').next().and_then(|n| n.parse::<u64>().ok()));

    match stripped {
        Some(days) => now_days.saturating_sub(days),
        None => 0,
    }
}

/// Derive a project slug from a workspace identity UUID.
/// Using UUID guarantees the same slug on every machine, unlike
/// the previous path-based derivation which tied slug to filesystem layout.
pub(crate) fn project_slug(id: &str) -> String {
    format!("ccsm-{id}")
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
        if days < diy {
            break;
        }
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
        if days < md {
            break;
        }
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
        if ins < lines.len() {
            out.push('\n');
        }
        for line in &lines[ins..] {
            out.push_str(line);
            out.push('\n');
        }
        out
    } else {
        let mut out = contents.to_string();
        if !out.ends_with('\n') {
            out.push('\n');
        }
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

/// Resolve the global data directory path from the current environment.
/// Convenience: resolves identity via `resolve_identity()`.
pub fn resolve_data_dir() -> Result<PathBuf> {
    let ctx = resolve_identity()?;
    Ok(global_data_dir(&ctx.id))
}

/// Sync the `> **status** | started ... | completed ...` line in the detail file
/// for a session to match the registry state. No-op if detail file doesn't exist.
/// The detail file lives in `~/.ccsm/<id>/sessions/<name>.md`.
pub fn sync_status_line(name: &str) {
    let detail_path = match resolve_data_dir() {
        Ok(dir) => dir.join("sessions").join(format!("{name}.md")),
        Err(_) => return,
    };

    if !detail_path.exists() {
        return;
    }

    let reg = match WorkspaceRegistry::load() {
        Ok(r) => r,
        Err(_) => return,
    };
    let Some(session) = reg.sessions.iter().find(|s| s.name == name) else {
        return;
    };

    let started = if session.started.is_empty() {
        ""
    } else {
        &session.started
    };
    let completed = if session.completed.is_empty() {
        ""
    } else {
        &session.completed
    };
    let new_line = format!(
        "> **{}** | started {} | completed {}",
        session.status, started, completed,
    );

    let Ok(contents) = std::fs::read_to_string(&detail_path) else {
        return;
    };
    let mut updated = String::new();
    let mut found = false;
    for line in contents.lines() {
        if line.trim_start().starts_with("> **") && line.contains("| started") {
            updated.push_str(&new_line);
            updated.push('\n');
            found = true;
        } else {
            updated.push_str(line);
            updated.push('\n');
        }
    }

    if found {
        let _ = std::fs::write(&detail_path, updated);
    }
}

pub(crate) fn is_kebab_case(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub(crate) fn harvest_from_pid(home: &std::path::Path, pid: u32) -> anyhow::Result<String> {
    let session_file = home
        .join(".claude")
        .join("sessions")
        .join(format!("{pid}.json"));
    if !session_file.exists() {
        anyhow::bail!(
            "{} no session file at {}\n  Is PID {} running?",
            ErrorCode::NoSession,
            session_file.display(),
            pid
        );
    }
    let contents = std::fs::read_to_string(&session_file).context("reading session file")?;
    let s: crate::session::Session =
        serde_json::from_str(&contents).context("parsing session file")?;
    if s.session_id.is_empty() {
        anyhow::bail!(
            "{} session file for PID {} has no sessionId yet",
            ErrorCode::NoSession,
            pid
        );
    }
    Ok(s.session_id)
}

pub(crate) fn validate_session_id(sid: &str) -> anyhow::Result<()> {
    // Accept OpenCode ses_* format (e.g. ses_abc123...)
    if sid.starts_with("ses_")
        && sid.len() > 4
        && sid[4..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Ok(());
    }
    // Accept standard 8-4-4-4-12 UUID
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
            "{} '{}' does not look like a session UUID (e.g. f493397b-...-4d5f15da0311).\n\
             If you renamed the session in the TUI, the name changed but the UUID didn't.\n\
             Use --pid <pid> instead: ccsm attach {} --pid <pid>",
            ErrorCode::Invalid,
            sid,
            sid
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
        && (!current_body.trim().is_empty() || sections.iter().any(|(_, b)| !b.trim().is_empty()))
    {
        sections.push((h, current_body));
    }
    sections
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Acquire the ENV_LOCK, recovering from poison so a single panicked test
    /// doesn't cascade-fail all subsequent env-dependent tests.
    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Run a test with CCSM_DATA_DIR set to a unique temp dir.
    /// Env var is restored after the closure runs.
    /// Uses a global mutex so parallel tests don't clobber each other's env vars.
    fn with_data_dir<F: FnOnce()>(f: F) {
        let _guard = lock_env();
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let prev = std::env::var("CCSM_DATA_DIR").ok();
        unsafe {
            std::env::set_var("CCSM_DATA_DIR", data_dir.to_string_lossy().as_ref());
        }
        f();
        match prev {
            Some(v) => unsafe {
                std::env::set_var("CCSM_DATA_DIR", v);
            },
            None => unsafe {
                std::env::remove_var("CCSM_DATA_DIR");
            },
        }
    }
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// Create a temp workspace with `.claude/sessions.json` pre-populated.
    /// Create a temp directory with a data directory structure for testing.
    /// Returns `(tempdir, data_dir)` where `data_dir` is `tempdir/data/`.
    fn temp_workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        // Start with an empty but valid registry
        let reg = WorkspaceRegistry {
            updated: "test".into(),
            sessions: vec![],
        };
        let reg_path = data_dir.join("sessions.json");
        std::fs::write(&reg_path, serde_json::to_string_pretty(&reg).unwrap()).unwrap();
        (dir, data_dir)
    }

    // ── LockFile tests ─────────────────────────────────────────────

    #[test]
    fn lock_acquire_creates_lockfile() {
        let (_dir, data_dir) = temp_workspace();
        let lock_path = data_dir.join("sessions.json.lock");
        assert!(!lock_path.exists());

        let _lock = LockFile::acquire_for_data_dir(&data_dir).unwrap();
        assert!(lock_path.exists());
    }

    #[test]
    fn lock_drop_releases() {
        let (_dir, data_dir) = temp_workspace();

        // Acquire and drop
        let lock = LockFile::acquire_for_data_dir(&data_dir).unwrap();
        drop(lock);

        // Should be able to acquire again immediately (lock released)
        let _lock2 = LockFile::acquire_for_data_dir(&data_dir).unwrap();
    }

    #[test]
    fn lock_exclusive_blocks_same_process() {
        let (_dir, data_dir) = temp_workspace();

        // Acquire exclusive lock on one fd
        let _lock1 = LockFile::acquire_for_data_dir(&data_dir).unwrap();

        // Try to acquire on a different fd — should fail with try_lock
        let lock_path = data_dir.join("sessions.json.lock");
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
        let (_dir, data_dir) = temp_workspace();

        let lock = LockFile::acquire_for_data_dir(&data_dir).unwrap();
        drop(lock);

        // Now try_lock should succeed
        let lock_path = data_dir.join("sessions.json.lock");
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
        let (_dir, data_dir) = temp_workspace();
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
                group: None,
                depends_on: vec![],
                branch: String::new(),
                use_worktree: false,
                retired_session_ids: vec![],
                consumer: String::new(),
            }],
        };
        std::fs::write(
            data_dir.join("sessions.json"),
            serde_json::to_string_pretty(&reg).unwrap(),
        )
        .unwrap();

        let (loaded, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.sessions[0].name, "test-session");
        assert_eq!(loaded.sessions[0].goal, "test goal");
        assert_eq!(loaded.sessions[0].session_id, "abc-123");
    }

    #[test]
    fn load_locked_holds_lock_during_mutation() {
        let (_dir, data_dir) = temp_workspace();

        let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();

        // While lock is held, try_lock should fail on another fd
        let lock_path = data_dir.join("sessions.json.lock");
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
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        });
        reg.save_to(&data_dir).unwrap();

        // Drop the lock
        drop(_lock);
        drop(reg);

        // Now another lock can be acquired
        let (_reg2, _lock2) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();
        assert_eq!(_reg2.sessions.len(), 1);
        assert_eq!(_reg2.sessions[0].name, "locked-mutation");
    }

    // ── Concurrent mutation tests ──────────────────────────────────

    #[test]
    fn concurrent_mutations_preserve_all_entries() {
        let (_dir, data_dir) = temp_workspace();
        let num_threads = 8;
        let data_dir = Arc::new(data_dir);

        let mut handles = vec![];
        for i in 0..num_threads {
            let d = Arc::clone(&data_dir);
            handles.push(std::thread::spawn(move || {
                let name = format!("thread-{}", i);
                let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&d).unwrap();
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
                    group: None,
                    depends_on: vec![],
                    branch: String::new(),
                    use_worktree: false,
                    retired_session_ids: vec![],
                    consumer: String::new(),
                });
                reg.save_to(&d).unwrap();
                // _lock dropped here
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // All entries should be present — none lost to race
        let reg = WorkspaceRegistry::load_from(&data_dir).unwrap();
        assert_eq!(
            reg.sessions.len(),
            num_threads,
            "expected {} entries, got {} — mutations were lost to a race",
            num_threads,
            reg.sessions.len()
        );

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
        let (_dir, data_dir) = temp_workspace();
        let num_threads = 8;
        let data_dir = Arc::new(data_dir);

        let mut handles = vec![];
        for i in 0..num_threads {
            let d = Arc::clone(&data_dir);
            handles.push(std::thread::spawn(move || {
                let name = format!("unlocked-{}", i);
                let mut reg =
                    WorkspaceRegistry::load_from(&d).unwrap_or_else(|_| WorkspaceRegistry::empty());
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
                    group: None,
                    depends_on: vec![],
                    branch: String::new(),
                    use_worktree: false,
                    retired_session_ids: vec![],
                    consumer: String::new(),
                });
                let _ = reg.save_to(&d);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // Without locks, the file is often corrupted or entries are lost.
        // We just verify the locked version works correctly — this test
        // exists to document the race condition that load_locked prevents.
        let reg = WorkspaceRegistry::load_from(&data_dir).unwrap_or_else(|_| {
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

    // ── Portability tests ──────────────────────────────────────────────

    #[test]
    fn project_slug_uses_uuid() {
        let slug = project_slug("abc-123-def");
        assert_eq!(slug, "ccsm-abc-123-def");
    }

    #[test]
    fn project_slug_is_stable() {
        let id = "some-uuid-that-never-changes";
        assert_eq!(project_slug(id), project_slug(id));
    }

    #[test]
    fn global_data_dir_defaults_to_home_ccsm() {
        let _guard = lock_env();
        let prev = std::env::var("CCSM_DATA_DIR").ok();
        unsafe { std::env::remove_var("CCSM_DATA_DIR") };
        let dir = global_data_dir("test-id");
        let s = dir.to_string_lossy();
        assert!(
            s.contains("/.ccsm/test-id") || s.contains("\\.ccsm\\test-id"),
            "expected /.ccsm/test-id in path, got: {s}"
        );
        match prev {
            Some(v) => unsafe { std::env::set_var("CCSM_DATA_DIR", v) },
            None => unsafe { std::env::remove_var("CCSM_DATA_DIR") },
        }
    }

    #[test]
    fn global_data_dir_respects_env_override() {
        let _guard = lock_env();
        let prev = std::env::var("CCSM_DATA_DIR").ok();
        unsafe {
            std::env::set_var("CCSM_DATA_DIR", "/tmp/ccsm-data");
        }
        let dir = global_data_dir("test-id");
        assert_eq!(dir, std::path::PathBuf::from("/tmp/ccsm-data/test-id"));
        match prev {
            Some(v) => unsafe { std::env::set_var("CCSM_DATA_DIR", v) },
            None => unsafe { std::env::remove_var("CCSM_DATA_DIR") },
        }
    }

    #[test]
    fn strip_stale_worktree_removes_field() {
        let _guard = lock_env();
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        // Write a sessions.json WITH a worktree field (simulating 0.15.0 format)
        let old_json = serde_json::json!({
            "updated": "test",
            "sessions": [{
                "session_id": "",
                "name": "old-session",
                "goal": "test",
                "scope": "",
                "status": "in_progress",
                "pids": [],
                "tags": [],
                "started": "",
                "completed": "",
                "group": null,
                "depends_on": [],
                "branch": "",
                "use_worktree": true,
                "worktree": "/home/user/proj/.claude/worktrees/old-session",
                "retired_session_ids": [],
                "consumer": ""
            }]
        });
        // Set up CCSM_DATA_DIR so global_registry_path resolves to our temp dir
        let prev = std::env::var("CCSM_DATA_DIR").ok();
        unsafe {
            std::env::set_var("CCSM_DATA_DIR", data_dir.to_string_lossy().as_ref());
        }

        let identity = WorkspaceIdentity {
            version: "0.16.0".into(),
            id: "test-id".into(),
        };

        // Write registry with worktree field
        let reg_path = global_registry_path(&identity.id);
        if let Some(parent) = reg_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&reg_path, serde_json::to_string_pretty(&old_json).unwrap()).unwrap();

        // Verify worktree FIELD exists before stripping
        let raw = std::fs::read_to_string(&reg_path).unwrap();
        assert!(
            raw.contains(r#""worktree":"#),
            "worktree field should exist before"
        );

        strip_stale_worktree(&identity).unwrap();

        // Verify worktree FIELD is gone after stripping (use_worktree still exists)
        let cleaned = std::fs::read_to_string(&reg_path).unwrap();
        assert!(
            !cleaned.contains(r#""worktree":"#),
            "worktree field should be stripped"
        );

        match prev {
            Some(v) => unsafe { std::env::set_var("CCSM_DATA_DIR", v) },
            None => unsafe { std::env::remove_var("CCSM_DATA_DIR") },
        }
    }

    #[test]
    fn worktree_path_for_is_deterministic() {
        let ws = std::path::Path::new("/home/user/project");
        let name = "my-session";
        let p = crate::commands::worktree::worktree_path_for(ws, name);
        assert_eq!(
            p,
            std::path::PathBuf::from("/home/user/project/.claude/worktrees/my-session")
        );
    }

    // ── project_slug tests ─────────────────────────────────────────

    #[test]
    fn project_slug_with_full_uuid() {
        let id = "f493397b-1234-4a5b-8901-4d5f15da0311";
        let slug = project_slug(id);
        assert_eq!(slug, "ccsm-f493397b-1234-4a5b-8901-4d5f15da0311");
    }

    #[test]
    fn project_slug_with_empty_id() {
        let slug = project_slug("");
        assert_eq!(slug, "ccsm-");
    }

    // ── global_config_path tests ───────────────────────────────────

    #[test]
    fn global_config_path_within_data_dir() {
        let path = global_config_path("my-workspace");
        let expected = global_data_dir("my-workspace").join("config.toml");
        assert_eq!(path, expected);
        assert_eq!(path.parent().unwrap(), global_data_dir("my-workspace"));
    }

    #[test]
    fn global_config_path_is_predictable() {
        let a = global_config_path("test-id");
        let b = global_config_path("test-id");
        assert_eq!(a, b);
    }

    // ── global_detail_path tests ───────────────────────────────────

    #[test]
    fn global_detail_path_under_sessions_subdir() {
        let _guard = lock_env();
        let prev = std::env::var("CCSM_DATA_DIR").ok();
        unsafe { std::env::remove_var("CCSM_DATA_DIR") };
        let path = global_detail_path("wid", "my-session");
        let expected_parent = global_data_dir("wid").join("sessions");
        assert_eq!(path.parent().unwrap(), expected_parent);
        match prev {
            Some(v) => unsafe { std::env::set_var("CCSM_DATA_DIR", v) },
            None => unsafe { std::env::remove_var("CCSM_DATA_DIR") },
        }
    }

    #[test]
    fn global_detail_path_ends_with_md() {
        let path = global_detail_path("wid", "my-session");
        assert!(path.to_string_lossy().ends_with("my-session.md"));
    }

    #[test]
    fn global_detail_path_diff_names_differ() {
        let a = global_detail_path("wid", "session-a");
        let b = global_detail_path("wid", "session-b");
        assert_ne!(a, b);
        assert!(a.to_string_lossy().contains("session-a"));
        assert!(b.to_string_lossy().contains("session-b"));
    }

    // ── global_worktree_path tests ─────────────────────────────────

    #[test]
    fn global_worktree_path_is_deterministic() {
        let _guard = lock_env();
        let a = global_worktree_path("wid", "session-a");
        let b = global_worktree_path("wid", "session-a");
        assert_eq!(a, b);
    }

    #[test]
    fn global_worktree_path_under_worktrees_subdir() {
        let _guard = lock_env();
        let path = global_worktree_path("wid", "session-a");
        let expected_parent = global_data_dir("wid").join("worktrees");
        assert_eq!(path.parent().unwrap(), expected_parent);
    }

    #[test]
    fn global_worktree_path_diff_ids_differ() {
        let _guard = lock_env();
        let a = global_worktree_path("workspace-a", "my-session");
        let b = global_worktree_path("workspace-b", "my-session");
        assert_ne!(a, b);
    }

    // ── ensure_data_dir tests ──────────────────────────────────────

    #[test]
    fn ensure_data_dir_creates_directory_structure() {
        with_data_dir(|| {
            ensure_data_dir("ensure-dir-test").unwrap();
            let base = global_data_dir("ensure-dir-test");
            assert!(
                base.join("sessions").is_dir(),
                "sessions subdir should exist"
            );
            assert!(
                base.join("session-group").is_dir(),
                "session-group subdir should exist"
            );
            assert!(
                base.join("worktrees").is_dir(),
                "worktrees subdir should exist"
            );
        });
    }

    #[test]
    fn ensure_data_dir_is_idempotent() {
        with_data_dir(|| {
            ensure_data_dir("idempotent-test").unwrap();
            ensure_data_dir("idempotent-test").unwrap();
            let base = global_data_dir("idempotent-test");
            assert!(base.join("sessions").is_dir());
        });
    }

    #[test]
    fn global_detail_path_uses_ensure_data_dir_sessions() {
        with_data_dir(|| {
            ensure_data_dir("int-test").unwrap();
            let detail = global_detail_path("int-test", "integration-check");
            assert!(detail.starts_with(global_data_dir("int-test")));
            assert!(detail.to_string_lossy().contains("/sessions/"));
        });
    }

    // ── edit_distance tests ────────────────────────────────────────

    #[test]
    fn edit_distance_identical() {
        assert_eq!(edit_distance("hello", "hello"), 0);
    }

    #[test]
    fn edit_distance_empty_vs_string() {
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", ""), 3);
    }

    #[test]
    fn edit_distance_insert_delete() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn edit_distance_substitution() {
        assert_eq!(edit_distance("rust", "dust"), 1);
    }

    #[test]
    fn edit_distance_unicode() {
        assert_eq!(edit_distance("café", "cafe"), 1);
    }

    #[test]
    fn edit_distance_completely_different() {
        assert_eq!(edit_distance("abc", "xyz"), 3);
    }

    // ── format_ts tests ────────────────────────────────────────────

    #[test]
    fn format_ts_zero() {
        assert_eq!(format_ts(0), "day0T00:00Z");
    }

    #[test]
    fn format_ts_one_second() {
        assert_eq!(format_ts(1000), "day0T00:00Z");
    }

    #[test]
    fn format_ts_one_minute() {
        assert_eq!(format_ts(60_000), "day0T00:01Z");
    }

    #[test]
    fn format_ts_one_hour() {
        assert_eq!(format_ts(3_600_000), "day0T01:00Z");
    }

    #[test]
    fn format_ts_one_day() {
        assert_eq!(format_ts(86_400_000), "day1T00:00Z");
    }

    #[test]
    fn format_ts_multi_day() {
        assert_eq!(
            format_ts(3 * 86_400_000 + 7_200_000 + 180_000),
            "day3T02:03Z"
        );
    }

    #[test]
    fn format_ts_truncates_seconds() {
        let result = format_ts(3661_000);
        assert_eq!(result, "day0T01:01Z");
        assert!(!result.contains("01Z:"), "no seconds field");
    }

    // ── parse_sections tests ───────────────────────────────────────

    #[test]
    fn parse_sections_empty_string() {
        let sections = parse_sections("");
        assert!(sections.is_empty());
    }

    #[test]
    fn parse_sections_no_headers() {
        let sections = parse_sections("some text\nwithout headers");
        assert!(sections.is_empty());
    }

    #[test]
    fn parse_sections_single_section() {
        let sections = parse_sections("## Goal\ndo the thing\nmore lines");
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].0, "Goal");
        assert!(sections[0].1.contains("do the thing"));
    }

    #[test]
    fn parse_sections_multiple_sections() {
        let md = "\
## Goal
first goal
## Scope
narrow scope
## Status
done";
        let sections = parse_sections(md);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].0, "Goal");
        assert_eq!(sections[1].0, "Scope");
        assert_eq!(sections[2].0, "Status");
    }

    #[test]
    fn parse_sections_trailing_section_with_only_blank_body() {
        let md = "\
## Goal
stuff
## Empty";
        let sections = parse_sections(md);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[1].0, "Empty");
        assert_eq!(sections[1].1, "");
    }

    #[test]
    fn parse_sections_trimmes_header_spaces() {
        let sections = parse_sections("##   My Header  \nbody");
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].0, "My Header");
    }

    // ── session_age_days tests ─────────────────────────────────────

    #[test]
    fn session_age_days_empty() {
        assert_eq!(session_age_days(""), 0);
    }

    #[test]
    fn session_age_days_garbage() {
        assert_eq!(session_age_days("not-a-timestamp"), 0);
    }

    #[test]
    fn session_age_days_today() {
        let now_days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 86400;
        let ts = format!("day{}T12:00:00Z", now_days);
        assert_eq!(session_age_days(&ts), 0);
    }

    #[test]
    fn session_age_days_yesterday() {
        let now_days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 86400;
        let ts = format!("day{}T00:00:00Z", now_days - 1);
        assert_eq!(session_age_days(&ts), 1);
    }

    #[test]
    fn session_age_days_old() {
        let ts = "day0T00:00:00Z";
        let age = session_age_days(ts);
        assert!(age > 19000, "epoch was >19000 days ago, got {age}");
    }

    // ── home_dir tests ─────────────────────────────────────────────

    #[test]
    fn home_dir_uses_home_env() {
        let _guard = lock_env();
        unsafe { std::env::set_var("HOME", "/custom/home") };
        let h = home_dir();
        assert_eq!(h, std::path::PathBuf::from("/custom/home"));
        unsafe { std::env::remove_var("HOME") };
    }

    #[test]
    fn home_dir_fallback_to_tmp() {
        let _guard = lock_env();
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::remove_var("HOME") };
        let h = home_dir();
        assert_eq!(h, std::path::PathBuf::from("/tmp"));
        match prev {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }

    // ── uuid_v4 tests ──────────────────────────────────────────────

    #[test]
    fn uuid_v4_format() {
        let uuid = uuid_v4();
        assert_eq!(uuid.len(), 36);
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        assert!(uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[test]
    fn uuid_v4_version_bits() {
        let uuid = uuid_v4();
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(&parts[2][..1], "4", "version nibble should be 4");
    }

    #[test]
    fn uuid_v4_variant_bits() {
        let uuid = uuid_v4();
        let parts: Vec<&str> = uuid.split('-').collect();
        let first = parts[3].chars().next().unwrap();
        assert!(
            first == '8' || first == '9' || first == 'a' || first == 'b',
            "variant bits should be 10xx, got {first}"
        );
    }

    #[test]
    fn uuid_v4_unique_across_calls() {
        let a = uuid_v4();
        let b = uuid_v4();
        assert_ne!(a, b);
    }

    // ── now_iso tests ──────────────────────────────────────────────

    #[test]
    fn now_iso_format() {
        let ts = now_iso();
        assert!(ts.starts_with("day"), "should start with day, got {ts}");
        assert!(
            ts.contains('T') && ts.contains('Z'),
            "should contain T...Z, got {ts}"
        );
        let parts: Vec<&str> = ts.split('T').collect();
        assert_eq!(parts.len(), 2, "expected dayN<T>HH:MM:SSZ, got {ts}");
        assert_eq!(&parts[1][parts[1].len() - 1..], "Z", "should end with Z");
    }

    #[test]
    fn now_iso_non_empty() {
        assert!(!now_iso().is_empty());
    }

    // ── is_kebab_case tests ────────────────────────────────────────

    #[test]
    fn is_kebab_case_valid() {
        assert!(is_kebab_case("my-session"));
        assert!(is_kebab_case("phase-1"));
        assert!(is_kebab_case("abc123"));
        assert!(is_kebab_case("a"));
    }

    #[test]
    fn is_kebab_case_invalid_uppercase() {
        assert!(!is_kebab_case("My-Session"));
    }

    #[test]
    fn is_kebab_case_invalid_underscore() {
        assert!(!is_kebab_case("my_session"));
    }

    #[test]
    fn is_kebab_case_invalid_spaces() {
        assert!(!is_kebab_case("my session"));
    }

    #[test]
    fn is_kebab_case_empty() {
        assert!(!is_kebab_case(""));
    }

    // ── insert_note tests ──────────────────────────────────────────

    #[test]
    fn insert_note_into_existing_section() {
        let md = "\
# Title

## Progress Log

- old entry
";
        let result = insert_note(md, "- new entry");
        assert!(result.contains("- old entry"));
        assert!(result.contains("- new entry"));
        // new entry should come before old
        let pos_new = result.find("- new entry").unwrap();
        let pos_old = result.find("- old entry").unwrap();
        assert!(pos_new < pos_old, "new entry should be prepended");
    }

    #[test]
    fn insert_note_adds_section_when_missing() {
        let md = "# Title\n\nsome body";
        let result = insert_note(md, "- first note");
        assert!(result.contains("## Progress Log"));
        assert!(result.contains("- first note"));
    }

    #[test]
    fn insert_note_skips_html_comments() {
        let md = "\
## Progress Log
<!-- comment -->
- old entry
";
        let result = insert_note(md, "- new entry");
        assert!(result.contains("<!-- comment -->"));
        assert!(result.contains("- new entry"));
        let pos_new = result.find("- new entry").unwrap();
        let pos_com = result.find("<!--").unwrap();
        assert!(pos_new > pos_com, "new entry should be after comment");
    }

    // ── replace_detail_section tests ───────────────────────────────

    #[test]
    fn replace_existing_section() {
        let md = "\
# Title

## Goal
old body

## Status
done
";
        let result = replace_detail_section(md, "## Goal", "new body");
        assert!(!result.contains("old body"));
        assert!(result.contains("new body"));
        assert!(result.contains("## Status"));
    }

    #[test]
    fn replace_nonexistent_section_appends() {
        let md = "# Title\nsome text\n";
        let result = replace_detail_section(md, "## Goal", "new body");
        assert!(result.contains("## Goal"));
        assert!(result.contains("new body"));
        assert!(result.contains("some text"));
    }

    // ── days_to_date / is_leap tests ───────────────────────────────

    #[test]
    fn days_to_date_epoch() {
        assert_eq!(days_to_date(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_date_known_date() {
        // 365 days after 1970-01-01 = 1971-01-01 (1970 is not leap)
        assert_eq!(days_to_date(365), (1971, 1, 1));
    }

    #[test]
    fn days_to_date_leap_feb29() {
        // 1970-01-01 to 2020-02-29: 50 years through 2019 end + 31 (jan) + 28 (feb 1-28)
        let mut d: u64 = 0;
        for y in 1970..2020 {
            d += if is_leap(y) { 366 } else { 365 };
        }
        d += 31; // Jan
        d += 28; // Feb 1-28
        assert_eq!(days_to_date(d), (2020, 2, 29));
    }

    #[test]
    fn is_leap_known() {
        assert!(is_leap(2000));
        assert!(!is_leap(1900));
        assert!(is_leap(2024));
        assert!(!is_leap(2023));
    }

    // ── validate_session_id tests ──────────────────────────────────

    #[test]
    fn validate_standard_uuid() {
        assert!(validate_session_id("f493397b-1234-4a5b-8901-4d5f15da0311").is_ok());
    }

    #[test]
    fn validate_ses_format() {
        assert!(validate_session_id("ses_abc123DEF").is_ok());
        assert!(validate_session_id("ses_").is_err());
    }

    #[test]
    fn validate_invalid_short() {
        assert!(validate_session_id("abc").is_err());
    }

    #[test]
    fn validate_invalid_wrong_dashes() {
        assert!(validate_session_id("1234-5678-9012-3456").is_err());
    }

    #[test]
    fn validate_empty_string() {
        assert!(validate_session_id("").is_err());
    }

    // ── find_nearest_git_root tests ────────────────────────────────

    #[test]
    fn find_nearest_git_root_finds_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::File::create(dir.path().join(".git")).unwrap();
        let found = find_nearest_git_root(&sub);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), dir.path());
    }

    #[test]
    fn find_nearest_git_root_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let found = find_nearest_git_root(dir.path());
        assert!(found.is_none());
    }

    #[test]
    fn find_nearest_git_root_returns_none_for_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("does-not-exist");
        let found = find_nearest_git_root(&sub);
        assert!(found.is_none());
    }

    // ── GroupRank Display tests ────────────────────────────────────

    #[test]
    fn group_rank_display_free() {
        assert_eq!(GroupRank::Free.to_string(), "free");
    }

    #[test]
    fn group_rank_display_numbered() {
        assert_eq!(GroupRank::Number(42).to_string(), "42");
    }

    // ── SessionStatus::transition_allowed tests ────────────────────

    #[test]
    fn transition_pending_to_in_progress() {
        assert!(SessionStatus::transition_allowed(
            SessionStatus::Pending,
            SessionStatus::InProgress
        ));
    }

    #[test]
    fn transition_in_progress_to_completed() {
        assert!(SessionStatus::transition_allowed(
            SessionStatus::InProgress,
            SessionStatus::Completed
        ));
    }

    #[test]
    fn transition_in_progress_to_blocked() {
        assert!(SessionStatus::transition_allowed(
            SessionStatus::InProgress,
            SessionStatus::Blocked
        ));
    }

    #[test]
    fn transition_blocked_to_abandoned() {
        assert!(SessionStatus::transition_allowed(
            SessionStatus::Blocked,
            SessionStatus::Abandoned
        ));
    }

    #[test]
    fn transition_trashed_to_in_progress() {
        assert!(SessionStatus::transition_allowed(
            SessionStatus::Trashed,
            SessionStatus::InProgress
        ));
    }

    #[test]
    fn transition_any_to_pending() {
        assert!(SessionStatus::transition_allowed(
            SessionStatus::Abandoned,
            SessionStatus::Pending
        ));
        assert!(SessionStatus::transition_allowed(
            SessionStatus::Blocked,
            SessionStatus::Pending
        ));
        assert!(SessionStatus::transition_allowed(
            SessionStatus::Trashed,
            SessionStatus::Pending
        ));
    }

    #[test]
    fn transition_any_to_trashed() {
        assert!(SessionStatus::transition_allowed(
            SessionStatus::Pending,
            SessionStatus::Trashed
        ));
        assert!(SessionStatus::transition_allowed(
            SessionStatus::Completed,
            SessionStatus::Trashed
        ));
    }

    #[test]
    fn transition_identical_is_allowed() {
        assert!(SessionStatus::transition_allowed(
            SessionStatus::Pending,
            SessionStatus::Pending
        ));
        assert!(SessionStatus::transition_allowed(
            SessionStatus::InProgress,
            SessionStatus::InProgress
        ));
    }

    #[test]
    fn transition_pending_to_completed_denied() {
        assert!(!SessionStatus::transition_allowed(
            SessionStatus::Pending,
            SessionStatus::Completed
        ));
    }

    #[test]
    fn transition_completed_to_in_progress_denied() {
        assert!(!SessionStatus::transition_allowed(
            SessionStatus::Completed,
            SessionStatus::InProgress
        ));
    }

    #[test]
    fn transition_abandoned_to_blocked_denied() {
        assert!(!SessionStatus::transition_allowed(
            SessionStatus::Abandoned,
            SessionStatus::Blocked
        ));
    }

    // ── note_timestamp tests ───────────────────────────────────────

    #[test]
    fn note_timestamp_format() {
        let ts = note_timestamp();
        assert_eq!(ts.len(), 17, "expected YYYY-MM-DD HH:MMZ format, got {ts}");
        assert!(
            !ts.contains('T'),
            "note_timestamp uses space separator, got {ts}"
        );
        assert!(ts.contains(' '), "expected space separator in {ts}");
        assert!(ts.ends_with('Z'), "expected timezone Z in {ts}");
    }

    #[test]
    fn note_timestamp_not_empty() {
        assert!(!note_timestamp().is_empty());
    }

    // ── find_project_root with git file case ───────────────────────

    #[test]
    fn find_project_root_with_git_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::File::create(dir.path().join(".git")).unwrap();
        let ccsm = dir.path().join(".ccsm");
        std::fs::write(&ccsm, "version = \"1\"\nid = \"test-id\"\n").unwrap();
        let result = find_project_root(dir.path()).unwrap();
        assert!(result.is_some());
        let (root, identity) = result.unwrap();
        assert_eq!(root, dir.path());
        assert_eq!(identity.id, "test-id");
    }

    #[test]
    fn find_project_root_returns_none_when_missing() {
        // Walk up from `/` — `/.ccsm` doesn't exist and root has no parent
        let result = find_project_root(std::path::Path::new("/")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn find_project_root_walks_up() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&sub).unwrap();
        let ccsm = dir.path().join(".ccsm");
        std::fs::write(&ccsm, "version = \"1\"\nid = \"walk-id\"\n").unwrap();
        let result = find_project_root(&sub).unwrap();
        assert!(result.is_some());
        let (root, identity) = result.unwrap();
        assert_eq!(root, dir.path());
        assert_eq!(identity.id, "walk-id");
    }

    // ── WorkspaceSession / status Display tests ────────────────────

    #[test]
    fn session_status_display_pending() {
        assert_eq!(SessionStatus::Pending.to_string(), "pending");
    }

    #[test]
    fn session_status_display_in_progress() {
        assert_eq!(SessionStatus::InProgress.to_string(), "in_progress");
    }

    #[test]
    fn session_status_display_completed() {
        assert_eq!(SessionStatus::Completed.to_string(), "completed");
    }

    #[test]
    fn session_status_display_blocked() {
        assert_eq!(SessionStatus::Blocked.to_string(), "blocked");
    }

    #[test]
    fn session_status_display_abandoned() {
        assert_eq!(SessionStatus::Abandoned.to_string(), "abandoned");
    }

    #[test]
    fn session_status_display_trashed() {
        assert_eq!(SessionStatus::Trashed.to_string(), "trashed");
    }

    // ── WorkspaceRegistry::trash / recover ─────────────────────────

    #[test]
    fn trash_moves_status_to_trashed() {
        let (_dir, data_dir) = temp_workspace();
        let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();
        reg.sessions.push(WorkspaceSession {
            session_id: "sid-1".into(),
            name: "session-one".into(),
            goal: "g".into(),
            scope: String::new(),
            status: SessionStatus::InProgress,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        });
        assert!(reg.trash("sid-1", "session-one"));
        assert_eq!(reg.sessions[0].status, SessionStatus::Trashed);
    }

    #[test]
    fn trash_nonexistent_returns_false() {
        let (_dir, data_dir) = temp_workspace();
        let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();
        assert!(!reg.trash("nonexistent", "nonexistent"));
    }

    #[test]
    fn recover_moves_status_to_in_progress() {
        let (_dir, data_dir) = temp_workspace();
        let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();
        reg.sessions.push(WorkspaceSession {
            session_id: "sid-2".into(),
            name: "session-two".into(),
            goal: "g".into(),
            scope: String::new(),
            status: SessionStatus::Trashed,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        });
        assert!(reg.recover("sid-2", "session-two"));
        assert_eq!(reg.sessions[0].status, SessionStatus::InProgress);
    }

    #[test]
    fn recover_nonexistent_returns_false() {
        let (_dir, data_dir) = temp_workspace();
        let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();
        assert!(!reg.recover("nonexistent", "nonexistent"));
    }

    // ── Default impl for WorkspaceSession ──────────────────────────

    #[test]
    fn session_default_status_is_pending() {
        assert_eq!(default_status(), SessionStatus::Pending);
    }

    // ── WorkspaceRegistry::seed tests ──────────────────────────────

    #[test]
    fn seed_populates_empty_registry() {
        let mut reg = WorkspaceRegistry::empty();
        let entry = WorkspaceSession {
            session_id: String::new(),
            name: "seed-session".into(),
            goal: "test".into(),
            scope: String::new(),
            status: SessionStatus::Pending,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        };
        reg.seed(vec![entry]);
        assert_eq!(reg.sessions.len(), 1);
        assert_eq!(reg.sessions[0].name, "seed-session");
    }

    #[test]
    fn seed_does_not_overwrite_existing() {
        let mut reg = WorkspaceRegistry::empty();
        let existing = WorkspaceSession {
            session_id: String::new(),
            name: "existing".into(),
            goal: "test".into(),
            scope: String::new(),
            status: SessionStatus::InProgress,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        };
        reg.sessions.push(existing);
        reg.seed(vec![WorkspaceSession {
            session_id: String::new(),
            name: "new-seed".into(),
            goal: "should not appear".into(),
            scope: String::new(),
            status: SessionStatus::Pending,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        }]);
        assert_eq!(reg.sessions.len(), 1);
        assert_eq!(reg.sessions[0].name, "existing");
    }

    // ── WorkspaceRegistry::empty tests ─────────────────────────────

    #[test]
    fn empty_registry_has_no_sessions() {
        let reg = WorkspaceRegistry::empty();
        assert!(reg.sessions.is_empty());
        assert_eq!(reg.updated, "");
    }

    // ── WorkspaceRegistry::save_to / load_from round-trip ──────────

    #[test]
    fn save_and_load_round_trip_preserves_data() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        let mut reg = WorkspaceRegistry::empty();
        reg.sessions.push(WorkspaceSession {
            session_id: "rt-id".into(),
            name: "round-trip".into(),
            goal: "test round trip".into(),
            scope: "scope".into(),
            status: SessionStatus::Completed,
            pids: vec![100],
            tags: vec!["test".into()],
            started: "day0T00:00Z".into(),
            completed: "day1T00:00Z".into(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: "claude".into(),
        });

        reg.save_to(&data_dir).unwrap();
        let loaded = WorkspaceRegistry::load_from(&data_dir).unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.sessions[0].name, "round-trip");
        assert_eq!(loaded.sessions[0].goal, "test round trip");
        assert_eq!(loaded.sessions[0].consumer, "claude");
        assert_eq!(loaded.sessions[0].status, SessionStatus::Completed);
    }

    // ── resolve_data_dir tests ─────────────────────────────────────

    #[test]
    fn resolve_data_dir_returns_path_when_identity_available() {
        with_data_dir(|| {
            let dir = tempfile::tempdir().unwrap();
            let ccsm_path = dir.path().join(".ccsm");
            std::fs::write(
                &ccsm_path,
                format!(
                    "version = \"{}\"\nid = \"test-id\"\n",
                    env!("CARGO_PKG_VERSION")
                ),
            )
            .unwrap();
            unsafe {
                unsafe { std::env::set_current_dir(dir.path()).ok() };
            }

            let result = resolve_data_dir();
            assert!(
                result.is_ok(),
                "resolve_data_dir should succeed: {:?}",
                result.err()
            );
            let path = result.unwrap();
            assert!(path.to_string_lossy().contains("test-id"));
        });
    }

    // ── WorkspaceRegistry::clean tests (by name fallback) ──────────

    #[test]
    fn clean_removes_by_session_id() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();

        reg.sessions.push(WorkspaceSession {
            session_id: "test-clean-id".into(),
            name: "clean-me".into(),
            goal: "g".into(),
            scope: String::new(),
            status: SessionStatus::Pending,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        });

        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).ok();
        reg.clean(
            "test-clean-id",
            "clean-me",
            &home,
            dir.path(),
            crate::consumer::Consumer::OpenCode,
        );
        assert_eq!(reg.sessions.len(), 0);
    }

    #[test]
    fn clean_falls_back_to_name_when_session_id_empty() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();

        reg.sessions.push(WorkspaceSession {
            session_id: String::new(),
            name: "seed-entry".into(),
            goal: "g".into(),
            scope: String::new(),
            status: SessionStatus::Pending,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        });

        reg.clean(
            "",
            "seed-entry",
            &dir.path().join("home"),
            dir.path(),
            crate::consumer::Consumer::OpenCode,
        );
        assert_eq!(reg.sessions.len(), 0);
    }

    // ── clean_all_trashed tests ────────────────────────────────────

    #[test]
    fn clean_all_trashed_removes_all_trashed_entries() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();

        reg.sessions.push(WorkspaceSession {
            session_id: "sid-keep".into(),
            name: "keep".into(),
            goal: "g".into(),
            scope: String::new(),
            status: SessionStatus::Pending,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        });
        reg.sessions.push(WorkspaceSession {
            session_id: "sid-trash".into(),
            name: "trash-me".into(),
            goal: "g".into(),
            scope: String::new(),
            status: SessionStatus::Trashed,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        });

        reg.clean_all_trashed(
            &dir.path().join("home"),
            dir.path(),
            crate::consumer::Consumer::OpenCode,
        );
        assert_eq!(reg.sessions.len(), 1);
        assert_eq!(reg.sessions[0].name, "keep");
    }

    // ── GroupRank tests ────────────────────────────────────────────

    #[test]
    fn group_rank_free_is_default() {
        assert_eq!(GroupRank::default(), GroupRank::Free);
    }

    #[test]
    fn group_rank_number_ordering() {
        assert!(GroupRank::Number(1) < GroupRank::Number(2));
        assert!(GroupRank::Number(5) > GroupRank::Free);
    }

    // ── archive tests ──────────────────────────────────────────────

    #[test]
    fn archive_clears_session_id_and_pids() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();

        reg.sessions.push(WorkspaceSession {
            session_id: "archive-id".into(),
            name: "archive-me".into(),
            goal: "g".into(),
            scope: String::new(),
            status: SessionStatus::Completed,
            pids: vec![42, 43],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        });

        let freed = reg.archive(
            "archive-id",
            "archive-me",
            &dir.path().join("home"),
            dir.path(),
            crate::consumer::Consumer::OpenCode,
        );
        assert_eq!(freed, 0, "no transcript files to free");
        assert!(reg.sessions[0].session_id.is_empty());
        assert!(reg.sessions[0].pids.is_empty());
    }

    #[test]
    fn archive_falls_back_to_name_when_session_id_empty() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let (mut reg, _lock) = WorkspaceRegistry::load_locked_from(&data_dir).unwrap();

        reg.sessions.push(WorkspaceSession {
            session_id: String::new(),
            name: "seed-archive".into(),
            goal: "g".into(),
            scope: String::new(),
            status: SessionStatus::Completed,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        });

        let freed = reg.archive(
            "",
            "seed-archive",
            &dir.path().join("home"),
            dir.path(),
            crate::consumer::Consumer::OpenCode,
        );
        assert_eq!(freed, 0);
        assert!(reg.sessions[0].session_id.is_empty());
    }

    // ── sync_status_line tests (no-op without detail file) ─────────

    #[test]
    fn sync_status_line_noop_without_detail_file() {
        with_data_dir(|| {
            sync_status_line("nonexistent-session");
            // Should not panic
        });
    }

    // ── harvest_from_pid tests ─────────────────────────────────────

    #[test]
    fn harvest_from_pid_errors_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let err = harvest_from_pid(dir.path(), 99999).unwrap_err();
        assert!(err.to_string().contains("[ERR_NOSESSION]"));
    }

    #[test]
    fn harvest_from_pid_parses_session_file() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_dir = dir.path().join(".claude").join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let session_content = serde_json::json!({
            "pid": 12345,
            "sessionId": "abc-123-def",
            "cwd": "/tmp",
            "startedAt": 1000,
            "updatedAt": 2000,
        });
        std::fs::write(sessions_dir.join("12345.json"), session_content.to_string()).unwrap();
        let sid = harvest_from_pid(dir.path(), 12345).unwrap();
        assert_eq!(sid, "abc-123-def");
    }

    #[test]
    fn harvest_from_pid_errors_when_session_id_empty() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_dir = dir.path().join(".claude").join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let session_content = serde_json::json!({
            "pid": 12345,
            "sessionId": "",
            "cwd": "/tmp",
            "startedAt": 1000,
            "updatedAt": 2000,
        });
        std::fs::write(sessions_dir.join("12345.json"), session_content.to_string()).unwrap();
        let err = harvest_from_pid(dir.path(), 12345).unwrap_err();
        assert!(err.to_string().contains("[ERR_NOSESSION]"));
    }

    // ── default_seed tests ─────────────────────────────────────────

    #[test]
    fn default_seed_contains_expected_sessions() {
        let seed = WorkspaceRegistry::default_seed();
        assert!(!seed.is_empty(), "default seed should have entries");
        assert!(seed.iter().any(|s| s.name == "phase-1-pty-embedding"));
        assert!(seed.iter().any(|s| s.name == "phase-2-sidebar"));
    }
}
