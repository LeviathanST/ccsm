#[allow(dead_code)]
mod registry;
#[allow(dead_code)]
mod session;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

// ── CLI (clap) ──────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "cc-tui", version, about = "Session registry CLI for Claude Code", long_about = None)]
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
        Commands::Setup => run_setup(&std::env::args().next().unwrap_or_else(|| "cc-tui".into())),
    }
}
// ── CLI subcommands ───────────────────────────────────────────────────

fn workspace_path() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn load_workspace_registry() -> anyhow::Result<crate::registry::WorkspaceRegistry> {
    crate::registry::WorkspaceRegistry::load(&workspace_path())
}

/// `cc-tui list` — all sessions, one line each.  --active / --summary / --status filter.
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
            eprintln!("  trashed      — soft-deleted, recoverable with `cc-tui recover <name>`");
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
    let mut reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
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

/// `cc-tui attach <name> <session-id>` — link a Claude session_id to an entry.
fn run_attach(name: &str, session_id: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let mut reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
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

/// `cc-tui trash <name>` — soft-delete: move to Trashed status.
fn run_trash(name: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let mut reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
    // Get session_id for the entry (may be empty for seed entries).
    let sid = reg.sessions.iter().rev()
        .find(|s| s.name == name)
        .map(|s| s.session_id.clone())
        .unwrap_or_default();
    if reg.trash(&sid, name) {
        reg.updated = now_iso_ts();
        reg.save(&workspace)?;
        println!("trashed     {}  ← soft-deleted (recover with `cc-tui recover {}`)", name, name);
    } else {
        anyhow::bail!("no session named '{}'", name);
    }
    Ok(())
}

/// `cc-tui recover <name>` — untrash: move from Trashed → InProgress.
fn run_recover(name: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let mut reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
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

/// `cc-tui clean <name>` — permanently delete transcript, session files, and registry entry.
fn run_clean(name: &str, home: &PathBuf, workspace: &PathBuf) -> anyhow::Result<()> {
    let mut reg = crate::registry::WorkspaceRegistry::load(workspace)?;
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

/// `cc-tui clean-all` — permanently delete ALL trashed entries.
fn run_clean_all(home: &PathBuf, workspace: &PathBuf) -> anyhow::Result<()> {
    let mut reg = crate::registry::WorkspaceRegistry::load(workspace)?;
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

/// `cc-tui new <name> [goal]` — create a new session entry.
fn run_new(name: &str, goal: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let mut reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
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
    println!("pending     {}  ← created", name);
    Ok(())
}

/// `cc-tui start|complete|block|abandon <name>` — status transitions.
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

/// `cc-tui resume <name>` — promote entry, exec `claude --resume` or fresh.
fn run_resume(name: &str, workspace: &PathBuf, home: &PathBuf) -> anyhow::Result<()> {
    let mut reg = crate::registry::WorkspaceRegistry::load(workspace)?;
    let slug = crate::registry::project_slug(workspace);

    // Promote this entry, demote others
    let now = now_iso_ts();
    for e in reg.sessions.iter_mut() {
        if e.status == crate::registry::SessionStatus::InProgress && e.name != name {
            e.status = crate::registry::SessionStatus::Completed;
            if e.completed.is_empty() { e.completed = now.clone(); }
        }
    }

    // Find entry by name, prefer newest
    let promote_idx = reg.sessions.iter().rev()
        .position(|e| e.name == name)
        .map(|pos| reg.sessions.len() - 1 - pos);

    let sid = match promote_idx {
        Some(i) => {
            reg.sessions[i].status = crate::registry::SessionStatus::InProgress;
            reg.sessions[i].started.clear();
            // Check if transcript exists
            if !reg.sessions[i].session_id.is_empty() {
                let path = home.join(".claude").join("projects")
                    .join(&slug).join(format!("{}.jsonl", reg.sessions[i].session_id));
                if path.exists() {
                    Some(reg.sessions[i].session_id.clone())
                } else {
                    reg.sessions[i].session_id.clear();
                    reg.sessions[i].pids.clear();
                    None
                }
            } else {
                reg.sessions[i].pids.clear();
                None
            }
        }
        None => {
            // Create new entry
            reg.sessions.push(crate::registry::WorkspaceSession {
                session_id: String::new(),
                name: name.to_string(),
                goal: String::new(),
                scope: String::new(),
                status: crate::registry::SessionStatus::InProgress,
                pids: vec![],
                tags: vec![],
                started: String::new(),
                completed: String::new(),
            });
            None
        }
    };

    reg.updated = now;
    reg.save(workspace)?;

    // Exec claude — use spawn() so we can capture the child pid
    // and later harvest the session_id from the session file on disk.
    let mut cmd = std::process::Command::new("claude");
    cmd.current_dir(workspace);
    if let Some(ref id) = sid {
        cmd.arg("--resume").arg(id);
        println!("resuming    {}  ← claude --resume {}", name, &id[..id.len().min(8)]);
    } else {
        println!("starting    {}  ← claude (fresh)", name);
    }
    // Set Claude's session display name to match our registry entry.
    cmd.arg("-n").arg(name);

    let mut child = cmd.spawn()?;
    let child_pid = child.id();

    // Write the pid to the registry entry — session_id is harvested below
    // before claude exits (it cleans up its session file on graceful exit).
    if let Some(idx) = promote_idx.or_else(|| {
        reg.sessions.iter().rev()
            .position(|e| e.name == name)
            .map(|pos| reg.sessions.len() - 1 - pos)
    }) {
        reg.sessions[idx].pids = vec![child_pid];
    } else {
        // New entry was created — find it (last entry with matching name).
        if let Some(entry) = reg.sessions.iter_mut().rev()
            .find(|e| e.name == name)
        {
            entry.pids = vec![child_pid];
        }
    }
    let _ = reg.save(workspace);

    // Poll for the session file — Claude writes it at startup, but
    // deletes it on graceful exit. Harvest the session_id NOW.
    let session_file = home.join(".claude").join("sessions").join(format!("{child_pid}.json"));
    for _ in 0..50 {
        if session_file.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if let Some(entry) = reg.sessions.iter_mut().rev().find(|e| e.name == name) {
        if let Ok(contents) = std::fs::read_to_string(&session_file) {
            if let Ok(s) = serde_json::from_str::<crate::session::Session>(&contents) {
                if entry.session_id.is_empty() {
                    entry.session_id = s.session_id;
                }
                if entry.started.is_empty() {
                    entry.started = crate::registry::format_ts(s.started_at);
                }
                reg.updated = now_iso_ts();
            }
        }
    }
    let _ = reg.save(workspace);

    let status = child.wait()?;

    // Process exited — clear stale pids.
    if let Some(entry) = reg.sessions.iter_mut().rev().find(|e| e.name == name) {
        entry.pids.clear();
        reg.updated = now_iso_ts();
    }
    let _ = reg.save(workspace);

    if !status.success() {
        anyhow::bail!("claude exited with {status}");
    }
    Ok(())
}

/// `cc-tui pending <name>` — reset to pending, clear identity fields.
fn run_pending(name: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let mut reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
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

/// `cc-tui scope <name> <text>` — set the scope field.
fn run_set_field(name: &str, _field: &str, value: &str) -> anyhow::Result<()> {
    mutate_session(name, "scope updated", |entry| {
        entry.scope = value.to_string();
    })
}

/// `cc-tui tag <name> <tags...>` — replace tags.
fn run_set_tags(name: &str, tags: &[String]) -> anyhow::Result<()> {
    let tag_str = tags.join(", ");
    let _ = mutate_session(name, "tagged", |entry| {
        entry.tags = tags.to_vec();
    });
    println!("  tags: {}", tag_str);
    Ok(())
}

/// `cc-tui tag <name> <tags...>` — replace tags.

/// `cc-tui show <name>` — registry fields + detail file section list.
/// `cc-tui show <name> --section <s>` — extract one section from detail file.
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
            println!("\n   `cc-tui show {} --section <name>` to read one section", name);
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



// ── Setup subcommand ──────────────────────────────────────────────────

fn run_setup(bin_path: &str) -> anyhow::Result<()> {
    use std::process::Command;

    let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("setup.sh");

    if !script.exists() {
        anyhow::bail!(
            "setup script not found at {}\n\
             (cc-tui must be run from its source tree with `cargo run setup` \
             or `cargo build && ./target/debug/cc-tui setup`)",
            script.display()
        );
    }

    println!("cc-tui setup ({})\n", bin_path);
    let status = Command::new("bash")
        .arg(&script)
        .status()
        .map_err(|e| anyhow::anyhow!("failed to run setup script: {e}"))?;

    if !status.success() {
        anyhow::bail!("setup script exited with {status}");
    }
    Ok(())
}
