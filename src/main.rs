#[allow(dead_code)]
mod registry;
#[allow(dead_code)]
mod sequence;
#[allow(dead_code)]
mod session;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

// ── CLI (clap) ──────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "ccsm", version, about = "Session registry CLI for Claude Code", long_about = None)]
struct Cli {
    /// Workspace directory (defaults to $PWD)
    #[arg(short = 'w', long)]
    workspace: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List sessions. --active (in_progress+blocked), --summary (counts), --status <s> (filter)
    #[command(visible_alias = "ls", visible_alias = "sessions", visible_alias = "s")]
    List {
        /// Only in_progress + blocked
        #[arg(short = 'a', long)]
        active: bool,
        /// Counts per status only
        #[arg(short = 's', long)]
        summary: bool,
        /// Filter by status. Pass "help" to see what each status means.
        #[arg(short = 'S', long)]
        status: Option<String>,
    },
    /// Show goal, scope, tags, session_id, pids, timestamps for a session
    Show {
        name: String,
        /// Extract one section from the detail file (e.g. "progress-log")
        #[arg(short = 'S', long)]
        section: Option<String>,
    },
    /// Create a pending entry. Use before starting work so the team sees it.
    New {
        /// kebab-case session name
        name: String,
        /// One-sentence goal
        #[arg(short = 'g', long)]
        goal: Option<String>,
    },
    /// pending → in_progress (max 1 per workspace)
    Start { name: String },
    /// in_progress → completed, sets completed timestamp
    Complete { name: String },
    /// in_progress → blocked (waiting on dependency)
    Block { name: String },
    /// in_progress → abandoned (no longer relevant)
    Abandon { name: String },
    /// Reset to pending, clears session_id + pids + timestamps
    Pending { name: String },
    /// Set scope: 2-4 sentences on approach, constraints, what's in/out
    Scope {
        name: String,
        #[arg(num_args = 1..)]
        text: Vec<String>,
    },
    /// Replace tags (space-separated)
    Tag {
        name: String,
        #[arg(num_args = 1..)]
        tags: Vec<String>,
    },
    /// Manually link a Claude session_id. Auto-managed by `resume`.
    Attach {
        name: String,
        session_id: String,
    },
    /// Spawn claude. --resume if session_id set, -n <name>, harvests session_id on exit
    Resume { name: String },
    /// Soft-delete → trashed. Recoverable. Trash first, then `clean` to nuke.
    Trash { name: String },
    /// trashed → in_progress
    Recover { name: String },
    /// Permanently delete transcript + session files + entry. Irreversible.
    Clean { name: String },
    /// Permanently delete ALL trashed entries. Irreversible.
    #[command(visible_alias = "clean-all")]
    CleanAll,
    /// Run multiple mutations in a single lock/load/save cycle.
    /// Each -q starts an operation: -q start foo -q scope foo text -q complete foo
    Sequence {
        #[arg(num_args = 1.., required = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Append a timestamped entry to the session detail file's Progress Log
    Note {
        name: String,
        #[arg(num_args = 1..)]
        text: Vec<String>,
    },
    /// Install session tracking into global CLAUDE.md + skills (run once)
    Setup,
}

// ─────────────────────────────────────────────────────────────────────

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()));

    match cli.command {
        Commands::List { active, summary, status } => run_list(active, summary, status.as_deref()),
        Commands::Show { name, section } => run_show(&name, section.as_deref()),
        Commands::New { name, goal } => run_new(&name, goal.as_deref().unwrap_or("")),
        Commands::Start { name } => run_status(&name, "start"),
        Commands::Complete { name } => run_status(&name, "complete"),
        Commands::Block { name } => run_status(&name, "block"),
        Commands::Abandon { name } => run_status(&name, "abandon"),
        Commands::Pending { name } => run_pending(&name),
        Commands::Scope { name, text } => run_set_field(&name, "scope", &text.join(" ")),
        Commands::Tag { name, tags } => run_set_tags(&name, &tags),
        Commands::Attach { name, session_id } => run_attach(&name, &session_id),
        Commands::Resume { name } => run_resume(&name, &workspace_path(), &home),
        Commands::Trash { name } => run_trash(&name),
        Commands::Recover { name } => run_recover(&name),
        Commands::Clean { name } => run_clean(&name, &home, &workspace_path()),
        Commands::CleanAll => run_clean_all(&home, &workspace_path()),
        Commands::Sequence { args } => run_sequence(&args),
        Commands::Note { name, text } => run_note(&name, &text.join(" ")),
        Commands::Setup => run_setup(&std::env::args().next().unwrap_or_else(|| "ccsm".into())),
    }
}
// ── CLI subcommands ───────────────────────────────────────────────────

fn workspace_path() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn load_workspace_registry() -> anyhow::Result<crate::registry::WorkspaceRegistry> {
    crate::registry::WorkspaceRegistry::load(&workspace_path())
}

/// `ccsm list` — all sessions, one line each.  --active / --summary / --status filter.
fn run_list(active: bool, summary: bool, status_filter: Option<&str>) -> anyhow::Result<()> {
    use crate::registry::SessionStatus;
    let reg = load_workspace_registry()?;

    let filter: Option<SessionStatus> = match status_filter {
        Some("pending") => Some(SessionStatus::Pending),
        Some("in_progress") | Some("in-progress") => Some(SessionStatus::InProgress),
        Some("completed") => Some(SessionStatus::Completed),
        Some("blocked") => Some(SessionStatus::Blocked),
        Some("abandoned") => Some(SessionStatus::Abandoned),
        Some("trashed") => Some(SessionStatus::Trashed),
        Some(other) => {
            eprintln!("unknown status '{}'", other);
            eprintln!();
            eprintln!("Valid statuses:");
            eprintln!("  pending      — planned, not started yet");
            eprintln!("  in_progress  — actively working on (max 1 per workspace)");
            eprintln!("  completed    — finished successfully");
            eprintln!("  blocked      — can't proceed, waiting on a dependency");
            eprintln!("  abandoned    — gave up, no longer relevant");
            eprintln!("  trashed      — soft-deleted, recoverable with `ccsm recover <name>`");
            return Ok(());
        }
        None => None,
    };

    // Summary mode: counts only
    if summary {
        let mut counts = std::collections::BTreeMap::new();
        for s in &reg.sessions {
            if active && !matches!(s.status, SessionStatus::InProgress | SessionStatus::Blocked) {
                continue;
            }
            if filter.is_some() && filter != Some(s.status) {
                continue;
            }
            *counts.entry(s.status).or_insert(0) += 1;
        }
        let total: usize = counts.values().sum();
        let get = |s: SessionStatus| counts.get(&s).copied().unwrap_or(0);
        println!(
            "{} active | {} completed | {} blocked | {} abandoned | {} trashed | {} total",
            get(SessionStatus::InProgress),
            get(SessionStatus::Completed),
            get(SessionStatus::Blocked),
            get(SessionStatus::Abandoned),
            get(SessionStatus::Trashed),
            total,
        );
        return Ok(());
    }

    // List mode
    if reg.sessions.is_empty() {
        println!("(no sessions)");
        return Ok(());
    }
    let mut printed = 0;
    for s in &reg.sessions {
        if active && !matches!(s.status, SessionStatus::InProgress | SessionStatus::Blocked) {
            continue;
        }
        if let Some(fs) = filter {
            if s.status != fs { continue; }
        }
        let goal = if s.goal.is_empty() { "" } else { " — " };
        let goal_text = if s.goal.len() > 80 {
            format!("{}{:.77}...", goal, &s.goal)
        } else {
            format!("{}{}", goal, &s.goal)
        };
        println!("{:12}  {:30}  {}", s.status.to_string(), s.name, goal_text.trim());
        printed += 1;
    }
    if printed == 0 {
        println!("(no matching sessions)");
    }
    Ok(())
}

// ── Mutations (shared helpers) ─────────────────────────────────────────

/// Load the registry, mutate one session by name, save, and report.
fn mutate_session<F>(name: &str, action: &str, f: F) -> anyhow::Result<()>
where
    F: FnOnce(&mut crate::registry::WorkspaceSession),
{
    let workspace = std::env::current_dir()?;
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(&workspace)?;
    {
        let entry = reg
            .sessions
            .iter_mut()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow::anyhow!("no session named '{}'", name))?;
        f(entry);
    }
    reg.updated = now_iso_ts();
    reg.save(&workspace)?;
    // Re-borrow immutably for the status line
    let entry = reg.sessions.iter().find(|s| s.name == name).unwrap();
    println!("{:12}  {}  ← {}", entry.status.to_string(), entry.name, action);
    Ok(())
}

fn now_iso_ts() -> String {
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

// ── Mutation commands ───────────────────────────────────────────────────

/// `ccsm attach <name> <session-id>` — link a Claude session_id to an entry.
fn run_attach(name: &str, session_id: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(&workspace)?;
    let entry = reg
        .sessions
        .iter_mut()
        .rev()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow::anyhow!("no session named '{}'", name))?;
    entry.session_id = session_id.to_string();
    reg.updated = now_iso_ts();
    reg.save(&workspace)?;
    println!("attached    {}  ← session {}", name, &session_id[..session_id.len().min(8)]);
    Ok(())
}

/// `ccsm trash <name>` — soft-delete: move to Trashed status.
fn run_trash(name: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(&workspace)?;
    // Get session_id for the entry (may be empty for seed entries).
    let sid = reg.sessions.iter().rev()
        .find(|s| s.name == name)
        .map(|s| s.session_id.clone())
        .unwrap_or_default();
    if reg.trash(&sid, name) {
        reg.updated = now_iso_ts();
        reg.save(&workspace)?;
        println!("trashed     {}  ← soft-deleted (recover with `ccsm recover {}`)", name, name);
    } else {
        anyhow::bail!("no session named '{}'", name);
    }
    Ok(())
}

/// `ccsm recover <name>` — untrash: move from Trashed → InProgress.
fn run_recover(name: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(&workspace)?;
    let sid = reg.sessions.iter().rev()
        .find(|s| s.name == name)
        .map(|s| s.session_id.clone())
        .unwrap_or_default();
    if reg.recover(&sid, name) {
        reg.updated = now_iso_ts();
        reg.save(&workspace)?;
        println!("recovered   {}  ← in_progress", name);
    } else {
        anyhow::bail!("no session named '{}'", name);
    }
    Ok(())
}

/// `ccsm clean <name>` — permanently delete transcript, session files, and registry entry.
fn run_clean(name: &str, home: &PathBuf, workspace: &PathBuf) -> anyhow::Result<()> {
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
    let sid = reg.sessions.iter().rev()
        .find(|s| s.name == name)
        .map(|s| s.session_id.clone())
        .unwrap_or_default();
    // Check entry exists before deleting
    if !reg.sessions.iter().any(|s| s.name == name) {
        anyhow::bail!("no session named '{}'", name);
    }
    reg.clean(&sid, name, home, workspace);
    reg.updated = now_iso_ts();
    reg.save(workspace)?;
    println!("cleaned     {}  ← permanently deleted", name);
    Ok(())
}

/// `ccsm clean-all` — permanently delete ALL trashed entries.
fn run_clean_all(home: &PathBuf, workspace: &PathBuf) -> anyhow::Result<()> {
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
    let count = reg.sessions.iter()
        .filter(|s| s.status == crate::registry::SessionStatus::Trashed)
        .count();
    if count == 0 {
        println!("(no trashed sessions)");
        return Ok(());
    }
    reg.clean_all_trashed(home, workspace);
    reg.updated = now_iso_ts();
    reg.save(workspace)?;
    println!("cleaned     {} trashed session{}", count, if count == 1 { "" } else { "s" });
    Ok(())
}

/// `ccsm new <name> [goal]` — create a new session entry.
fn run_new(name: &str, goal: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(&workspace)?;
    if reg.sessions.iter().any(|s| s.name == name) {
        anyhow::bail!("session '{}' already exists", name);
    }
    reg.sessions.push(crate::registry::WorkspaceSession {
        session_id: String::new(),
        name: name.to_string(),
        goal: goal.to_string(),
        scope: String::new(),
        status: crate::registry::SessionStatus::Pending,
        pids: vec![],
        tags: vec![],
        started: String::new(),
        completed: String::new(),
    });
    reg.updated = now_iso_ts();
    reg.save(&workspace)?;

    // Auto-create the detail file from template if it doesn't exist.
    let detail_path = workspace
        .join(".claude")
        .join("sessions")
        .join(format!("{}.md", name));
    if !detail_path.exists() {
        let template = workspace
            .join(".claude")
            .join("session-detail-template.md");
        if template.exists() {
            if let Ok(contents) = std::fs::read_to_string(&template) {
                let populated = contents
                    .replace("{{name}}", name)
                    .replace("{{goal}}", goal)
                    .replace("{{status}}", "pending")
                    .replace("{{scope}}", "(fill in — approach, constraints, what's in/out)")
                    .replace("{{tags}}", "(fill in)")
                    .replace("{{session_id}}", "(auto — ccsm manages)")
                    .replace("{{cwd}}", &workspace.to_string_lossy())
                    .replace("{{pids}}", "(auto — ccsm manages)")
                    .replace("{{kind}}", "(auto)")
                    .replace("{{version}}", "(auto)")
                    .replace("{{waiting_for}}", "(none)")
                    .replace("{{dependencies}}", "(none)")
                    .replace("{{now}}", &now_iso_ts())
                    .replace("{{note}}", "Session created");
                if let Some(parent) = detail_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&detail_path, populated);
            }
        }
    }

    println!("pending     {}  ← created", name);
    Ok(())
}

/// `ccsm start|complete|block|abandon <name>` — status transitions.
fn run_status(name: &str, action: &str) -> anyhow::Result<()> {
    use crate::registry::SessionStatus;
    let new_status = match action {
        "start" => SessionStatus::InProgress,
        "complete" => SessionStatus::Completed,
        "block" => SessionStatus::Blocked,
        "abandon" => SessionStatus::Abandoned,
        _ => anyhow::bail!("unknown status action: {}", action),
    };
    mutate_session(name, action, |entry| {
        entry.status = new_status;
        if action == "complete" || action == "abandon" {
            entry.completed = now_iso_ts();
        }
    })
}

/// `ccsm resume <name>` — promote entry, exec `claude --resume` or fresh.
fn run_resume(name: &str, workspace: &PathBuf, home: &PathBuf) -> anyhow::Result<()> {
    let slug = crate::registry::project_slug(workspace);
    let now = now_iso_ts();

    // ── Phase 1: Promote entry, demote others (locked) ──────────────
    let (sid, fresh) = {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;

        for e in reg.sessions.iter_mut() {
            if e.status == crate::registry::SessionStatus::InProgress && e.name != name {
                e.status = crate::registry::SessionStatus::Completed;
                if e.completed.is_empty() {
                    e.completed = now.clone();
                }
            }
        }

        let (sid, is_fresh) = match reg.sessions.iter().rev().position(|e| e.name == name) {
            Some(pos) => {
                let i = reg.sessions.len() - 1 - pos;
                reg.sessions[i].status = crate::registry::SessionStatus::InProgress;
                reg.sessions[i].started.clear();
                if !reg.sessions[i].session_id.is_empty() {
                    let path = home.join(".claude").join("projects")
                        .join(&slug).join(format!("{}.jsonl", reg.sessions[i].session_id));
                    if path.exists() {
                        (Some(reg.sessions[i].session_id.clone()), false)
                    } else {
                        // session_id exists but transcript is gone — corrupted state.
                        // Don't silently fall back to fresh; let the user decide.
                        anyhow::bail!(
                            "session '{}' has session_id '{}' but transcript not found at:\n  {}\n\
                             The transcript may have been deleted or cleaned.\n\
                             To start fresh: ccsm pending {}  (clears session_id, then resume)",
                            name,
                            &reg.sessions[i].session_id[..reg.sessions[i].session_id.len().min(8)],
                            path.display(),
                            name,
                        );
                    }
                } else {
                    (None, false)
                }
            }
            None => {
                let similar: Vec<&str> = reg
                    .sessions
                    .iter()
                    .map(|s| s.name.as_str())
                    .filter(|n| edit_distance(n, name) <= 3)
                    .collect();
                if similar.is_empty() {
                    anyhow::bail!(
                        "no session named '{}'. Use `ccsm new {} -g \"...\"` to create one.",
                        name, name
                    );
                } else {
                    anyhow::bail!(
                        "no session named '{}'. Did you mean: {}?",
                        name,
                        similar.join(", ")
                    );
                }
            }
        };

        reg.updated = now.clone();
        reg.save(workspace)?;
        (sid, is_fresh)
    }; // lock released

    // ── Phase 2: Spawn claude (no lock) ─────────────────────────────
    let mut cmd = std::process::Command::new("claude");
    cmd.current_dir(workspace);
    if let Some(ref id) = sid {
        cmd.arg("--resume").arg(id);
        println!("resuming    {}  ← claude --resume {}", name, &id[..id.len().min(8)]);
    } else if fresh {
        println!("starting    {}  ← claude (fresh)", name);
    } else {
        println!("starting    {}  ← claude (new session)", name);
    }
    cmd.arg("-n").arg(name);

    let mut child = cmd.spawn()?;
    let child_pid = child.id();

    // ── Phase 3: Write pid to registry (locked) ─────────────────────
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
        match reg.sessions.iter_mut().rev().find(|e| e.name == name) {
            Some(entry) => entry.pids = vec![child_pid],
            None => anyhow::bail!(
                "internal error: session '{}' vanished from registry between Phase 1 and Phase 3",
                name
            ),
        }
        reg.updated = now_iso_ts();
        reg.save(workspace)?;
    }

    // ── Phase 4: Poll for session file, harvest session_id ──────────
    let session_file = home.join(".claude").join("sessions").join(format!("{child_pid}.json"));
    let mut found = false;
    for _ in 0..50 {
        if session_file.exists() {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if !found {
        anyhow::bail!(
            "claude did not write a session file at {} within 5s.\n\
             Claude may have failed to start. Check for errors above.",
            session_file.display(),
        );
    }

    // ── Phase 5: Harvest session_id + started (locked) ──────────────
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
        let entry = match reg.sessions.iter_mut().rev().find(|e| e.name == name) {
            Some(e) => e,
            None => anyhow::bail!(
                "internal error: session '{}' vanished from registry between Phase 1 and Phase 5",
                name
            ),
        };

        match std::fs::read_to_string(&session_file) {
            Ok(contents) => match serde_json::from_str::<crate::session::Session>(&contents) {
                Ok(s) => {
                    if entry.session_id.is_empty() {
                        entry.session_id = s.session_id;
                    }
                    if entry.started.is_empty() {
                        entry.started = crate::registry::format_ts(s.started_at);
                    }
                    reg.updated = now_iso_ts();
                }
                Err(e) => {
                    eprintln!(
                        "warning: failed to parse session file {}: {}. \
                         Session tracking may be incomplete.",
                        session_file.display(), e
                    );
                }
            },
            Err(e) => {
                eprintln!(
                    "warning: failed to read session file {}: {}. \
                     Session tracking may be incomplete.",
                    session_file.display(), e
                );
            }
        }
        reg.save(workspace)?;
    }

    // ── Phase 6: Wait for child ─────────────────────────────────────
    let status = child.wait()?;

    // ── Phase 7: Clear stale pids (locked) ──────────────────────────
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
        match reg.sessions.iter_mut().rev().find(|e| e.name == name) {
            Some(entry) => {
                entry.pids.clear();
                reg.updated = now_iso_ts();
            }
            None => {
                eprintln!(
                    "warning: session '{}' not found in registry at cleanup — \
                     may have been removed while claude was running",
                    name
                );
            }
        }
        reg.save(workspace)?;
    }

    if !status.success() {
        anyhow::bail!("claude exited with {status}");
    }
    Ok(())
}

/// `ccsm pending <name>` — reset to pending, clear identity fields.
fn run_pending(name: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(&workspace)?;
    let entry = reg
        .sessions
        .iter_mut()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow::anyhow!("no session named '{}'", name))?;
    entry.status = crate::registry::SessionStatus::Pending;
    entry.session_id.clear();
    entry.pids.clear();
    entry.started.clear();
    entry.completed.clear();
    reg.updated = now_iso_ts();
    reg.save(&workspace)?;
    println!("pending     {}  ← reset (identity fields cleared)", name);
    Ok(())
}

/// `ccsm scope <name> <text>` — set the scope field.
fn run_set_field(name: &str, _field: &str, value: &str) -> anyhow::Result<()> {
    mutate_session(name, "scope updated", |entry| {
        entry.scope = value.to_string();
    })
}

/// `ccsm tag <name> <tags...>` — replace tags.
fn run_set_tags(name: &str, tags: &[String]) -> anyhow::Result<()> {
    let tag_str = tags.join(", ");
    let _ = mutate_session(name, "tagged", |entry| {
        entry.tags = tags.to_vec();
    });
    println!("  tags: {}", tag_str);
    Ok(())
}

/// `ccsm show <name>` — registry fields + detail file section list.
/// `ccsm show <name> --section <s>` — extract one section from detail file.
fn run_show(name: &str, section: Option<&str>) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
    let session = reg
        .sessions
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow::anyhow!("no session named '{}'", name))?;

    let detail_path = workspace
        .join(".claude")
        .join("sessions")
        .join(format!("{}.md", name));

    // If --section is given, extract and print just that section.
    if let Some(sec) = section {
        if !detail_path.exists() {
            anyhow::bail!("no detail file for '{}' (expected {})", name, detail_path.display());
        }
        let contents = std::fs::read_to_string(&detail_path)?;
        let sections = parse_sections(&contents);
        let key = sec.to_lowercase().replace('-', " ");
        match sections.iter().find(|(h, _)| h.to_lowercase() == key) {
            Some((header, body)) => {
                println!("## {}\n{}", header, body.trim());
            }
            None => {
                eprintln!("section '{}' not found. Available:", sec);
                for (h, _) in &sections {
                    eprintln!("  --section {}", h.to_lowercase().replace(' ', "-"));
                }
                anyhow::bail!("no such section");
            }
        }
        return Ok(());
    }

    // ── Registry fields ──────────────────────────────────────────
    println!("name:       {}", session.name);
    println!("status:     {}", session.status);
    if !session.goal.is_empty() {
        println!("goal:       {}", session.goal);
    }
    if !session.scope.is_empty() {
        println!("scope:      {}", session.scope);
    }
    if !session.tags.is_empty() {
        println!("tags:       {}", session.tags.join(", "));
    }
    if !session.session_id.is_empty() {
        println!("session_id: {}", session.session_id);
    }
    if session.pids.is_empty() {
        println!("pids:       (none)");
    } else {
        println!("pids:       {}", session.pids.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", "));
    }
    if !session.started.is_empty() {
        println!("started:    {}", session.started);
    }
    if !session.completed.is_empty() {
        println!("completed:  {}", session.completed);
    }

    // ── Detail file sections ─────────────────────────────────────
    if detail_path.exists() {
        let contents = std::fs::read_to_string(&detail_path)?;
        let sections = parse_sections(&contents);
        if sections.is_empty() {
            println!("\n📄 .claude/sessions/{}.md (no sections)", name);
        } else {
            println!("\n📄 .claude/sessions/{}.md", name);
            for (header, body) in &sections {
                // Count non-empty lines as a rough size hint
                let lines = body.lines().filter(|l| !l.trim().is_empty()).count();
                let hint = if lines > 0 {
                    format!(" ({} lines)", lines)
                } else {
                    String::new()
                };
                println!("   ## {}{}", header, hint);
            }
            println!("\n   `ccsm show {} --section <name>` to read one section", name);
        }
    } else {
        println!("\n💡 no detail file — create: cp .claude/session-detail-template.md .claude/sessions/{}.md", name);
    }

    Ok(())
}

/// Parse a markdown string into `(header, body)` pairs for each `## Section`.
/// Stops at the next `## ` or end of file.
fn parse_sections(md: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut current_header: Option<String> = None;
    let mut current_body = String::new();

    for line in md.lines() {
        if line.starts_with("## ") {
            // Save previous section
            if let Some(h) = current_header.take() {
                sections.push((h, std::mem::take(&mut current_body)));
            }
            current_header = Some(line[3..].trim().to_string());
        } else if current_header.is_some() {
            if !current_body.is_empty() {
                current_body.push('\n');
            }
            current_body.push_str(line);
        }
    }
    // Save final section
    if let Some(h) = current_header {
        if !current_body.trim().is_empty() || sections.iter().any(|(_, b)| !b.trim().is_empty()) {
            sections.push((h, current_body));
        }
    }
    sections
}



// ── Sequence subcommand ────────────────────────────────────────────────

/// `ccsm sequence -q <cmd> <args...> -q <cmd> <args...> ...`
///
/// Runs multiple mutations in a single lock/load/save cycle.
/// Each `-q` flag starts a new operation group.
fn run_sequence(args: &[String]) -> anyhow::Result<()> {
    // Split on "-q" markers into operation groups.
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut in_group = false;

    for arg in args {
        if arg == "-q" {
            if in_group && !current.is_empty() {
                groups.push(std::mem::take(&mut current));
            }
            in_group = true;
        } else if in_group {
            current.push(arg.clone());
        }
    }
    if in_group && !current.is_empty() {
        groups.push(current);
    }

    if groups.is_empty() {
        anyhow::bail!("expected at least one -q <command> ... group");
    }

    // Phase 1: Parse all operations (no lock — fail-fast on bad input)
    let ops: Vec<crate::sequence::SeqOp> = groups
        .iter()
        .map(|g| crate::sequence::SeqOp::parse(g))
        .collect::<anyhow::Result<Vec<_>>>()?;

    // Phase 2: Execute all operations in memory (single lock)
    let workspace = std::env::current_dir()?;
    let now = now_iso_ts();
    let outputs = {
        let (mut reg, _lock) =
            crate::registry::WorkspaceRegistry::load_locked(&workspace)?;

        let mut outputs = Vec::new();
        for op in &ops {
            let lines = crate::sequence::apply_op(&mut reg, op, &now)?;
            outputs.extend(lines);
        }

        reg.updated = now;
        reg.save(&workspace)?;
        outputs
    }; // lock released

    // Phase 3: Print all output
    for line in &outputs {
        println!("{}", line);
    }

    Ok(())
}

// ── Note subcommand ────────────────────────────────────────────────────

/// Append a timestamped entry to `.claude/sessions/<name>.md` Progress Log.
fn run_note(name: &str, text: &str) -> anyhow::Result<()> {
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("note text is required. Usage: ccsm note <name> <text>");
    }

    let workspace = std::env::current_dir()?;

    // Verify session exists in registry
    let reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
    if !reg.sessions.iter().any(|s| s.name == name) {
        anyhow::bail!("no session named '{}'", name);
    }

    let detail_path = workspace
        .join(".claude")
        .join("sessions")
        .join(format!("{}.md", name));

    if !detail_path.exists() {
        anyhow::bail!(
            "no detail file for '{}'. Create one:\n  cp .claude/session-detail-template.md .claude/sessions/{}.md",
            name, name
        );
    }

    let contents = std::fs::read_to_string(&detail_path)?;
    let ts = note_timestamp();
    let new_entry = format!("- [{}] {}\n", ts, text);

    let new_contents = insert_note(&contents, &new_entry);
    std::fs::write(&detail_path, new_contents)?;

    println!("noted       {}  ← [{}] {}", name, ts, text);
    Ok(())
}

/// Simple UTC timestamp: `YYYY-MM-DD HH:MMZ` without external date crates.
fn note_timestamp() -> String {
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
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

/// Simple Levenshtein distance — used to suggest corrections for typos.
fn edit_distance(a: &str, b: &str) -> usize {
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

/// Insert `new_entry` into the Progress Log section of `contents`.
/// Prepends (newest at top) — inserts right after the `## Progress Log`
/// header, past any blank lines or HTML comments.
fn insert_note(contents: &str, new_entry: &str) -> String {
    let lines: Vec<&str> = contents.lines().collect();

    if let Some(hdr) = lines.iter().position(|l| l.trim() == "## Progress Log") {
        // Find insertion point: skip past blank lines and HTML comments
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
        for i in 0..ins {
            out.push_str(lines[i]);
            out.push('\n');
        }
        out.push_str(new_entry);
        if ins < lines.len() { out.push('\n'); }
        for i in ins..lines.len() {
            out.push_str(lines[i]);
            out.push('\n');
        }
        out
    } else {
        // No Progress Log section — append one
        let mut out = contents.to_string();
        if !out.ends_with('\n') { out.push('\n'); }
        out.push('\n');
        out.push_str("## Progress Log\n\n");
        out.push_str(new_entry);
        out.push('\n');
        out
    }
}

// ── Setup subcommand ──────────────────────────────────────────────────

fn run_setup(bin_path: &str) -> anyhow::Result<()> {
    use std::process::Command;

    let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("setup.sh");

    if !script.exists() {
        anyhow::bail!(
            "setup script not found at {}\n\
             (ccsm must be run from its source tree with `cargo run setup` \
             or `cargo build && ./target/debug/ccsm setup`)",
            script.display()
        );
    }

    println!("ccsm setup ({})\n", bin_path);
    let status = Command::new("bash")
        .arg(&script)
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run setup script: {e}"))?;

    if !status.success() {
        anyhow::bail!("setup script exited with {status}");
    }
    Ok(())
}
