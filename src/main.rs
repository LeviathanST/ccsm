#[allow(dead_code)]
mod registry;
#[allow(dead_code)]
mod sequence;
#[allow(dead_code)]
mod session;
#[allow(dead_code)]
mod commands;

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use crate::registry::{parse_sections, now_iso as now_iso_ts};

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
        /// Verbose: show full goal + tags (teammate scan mode)
        #[arg(short = 'v', long)]
        verbose: bool,
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
        /// Skip fuzzy duplicate detection
        #[arg(short = 'f', long)]
        force: bool,
    },
    /// pending → in_progress
    Start { name: String },
    /// in_progress → completed, sets completed timestamp
    Complete {
        name: String,
        /// Skip gate checks (detail file completeness etc.)
        #[arg(short = 'f', long)]
        force: bool,
    },
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
        /// Session UUID (from ~/.claude/sessions/<pid>.json). Omit if using --pid.
        session_id: Option<String>,
        /// Harvest session_id from a live session file by PID.
        #[arg(long)]
        pid: Option<u32>,
    },
    /// Rename a session: registry entry, detail file, live session files, and transcript.
    /// Use -g and -s to refresh the topic at the same time.
    Rename {
        /// Current session name
        old: String,
        /// New session name (kebab-case)
        new: String,
        /// New goal (for topic change)
        #[arg(short = 'g', long)]
        goal: Option<String>,
        /// New scope (for topic change)
        #[arg(short = 's', long)]
        scope: Option<String>,
    },
    /// Spawn claude. --resume if session_id set, -n <name>, harvests session_id on exit
    Resume { name: String },
    /// Retire current Claude session, spawn a fresh one for the same ccsm session.
    /// Use when context is bloated (>40%) and the model gets biased.
    Refresh {
        name: String,
        /// Why the refresh (logged to retired_session_ids)
        #[arg(short = 'r', long)]
        reason: Option<String>,
    },
    /// Soft-delete → trashed. Recoverable. Trash first, then `clean` to nuke.
    Trash { name: String },
    /// trashed → in_progress
    Recover { name: String },
    /// Permanently delete transcript + session files + entry. Irreversible.
    Clean { name: String },
    /// Pre-completion gate: check detail file completeness, print self-review checklist.
    /// Exits non-zero if the detail file is hollow. Run before `ccsm complete`.
    Close { name: String },
    /// Stop-hook helper: if working tree is dirty, remind to update session detail.
    /// Auto-discovers the in_progress session. Silent when clean or recently noted.
    NoteCheck,
    /// Permanently delete ALL trashed entries. Irreversible.
    CleanAll,
    /// Archive transcript + session files, keep registry entry as work log
    Archive { name: String },
    /// Archive all completed sessions that still have transcripts
    ArchiveAll,
    /// Scan for health issues: orphaned IDs, dead PIDs, empty fields, cleanup candidates
    Doctor,
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
        /// Cross-session note: auto-prepend "CROSS-SESSION [source]: " to the note
        #[arg(short = 'x', long)]
        cross: Option<String>,
    },
    /// Generate shell completion script (bash, fish, zsh)
    Completions {
        /// Shell: bash, fish, or zsh
        shell: String,
    },
    /// Output a system-reminder block with the active session's goal and scope.
    /// Used by SystemMessage hook to inject session context every turn.
    InjectScope {
        /// Session name (auto-detects in_progress if omitted)
        name: Option<String>,
    },
    /// Check if current work aligns with session scope. Exit 0 = pass, 1 = fail.
    /// Designed for Stop hook before `ccsm complete`.
    GateCheck {
        /// Session name (auto-detects in_progress if omitted)
        name: Option<String>,
        /// Strict mode: fail if scope is empty or unfilled
        #[arg(short = 'S', long)]
        strict: bool,
    },
    /// Install session tracking into global CLAUDE.md + skills (run once)
    Setup,
}

// ─────────────────────────────────────────────────────────────────────

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()));

    match cli.command {
        Commands::List { active, summary, status, verbose } => run_list(active, summary, verbose, status.as_deref()),
        Commands::Show { name, section } => run_show(&name, section.as_deref()),
        Commands::New { name, goal, force } => run_new(&name, goal.as_deref().unwrap_or(""), force),
        Commands::Start { name } => run_status(&name, "start", false),
        Commands::Complete { name, force } => run_status(&name, "complete", force),
        Commands::Block { name } => run_status(&name, "block", false),
        Commands::Abandon { name } => run_status(&name, "abandon", false),
        Commands::Pending { name } => run_pending(&name),
        Commands::Scope { name, text } => run_set_field(&name, "scope", &text.join(" ")),
        Commands::Tag { name, tags } => run_set_tags(&name, &tags),
        Commands::Attach { name, session_id, pid } => run_attach(&name, session_id.as_deref(), pid, &home),
        Commands::Rename { old, new, goal, scope } => run_rename(&old, &new, goal.as_deref(), scope.as_deref(), &home, &workspace_path()),
        Commands::Resume { name } => commands::resume::run_resume(&name, &workspace_path(), &home),
        Commands::Refresh { name, reason } => run_refresh(&name, reason.as_deref(), &workspace_path(), &home),
        Commands::Trash { name } => run_trash(&name),
        Commands::Recover { name } => run_recover(&name),
        Commands::Clean { name } => run_clean(&name, &home, &workspace_path()),
        Commands::Close { name } => run_close(&name),
        Commands::NoteCheck => run_note_check(),
        Commands::CleanAll => run_clean_all(&home, &workspace_path()),
        Commands::Archive { name } => run_archive(&name, &home, &workspace_path()),
        Commands::ArchiveAll => run_archive_all(&home, &workspace_path()),
        Commands::Doctor => commands::doctor::run_doctor(&home, &workspace_path()),
        Commands::Sequence { args } => run_sequence(&args),
        Commands::Note { name, text, cross } => run_note(&name, &text.join(" "), cross.as_deref()),
        Commands::Completions { shell } => run_completions(&shell),
        Commands::InjectScope { name } => run_inject_scope(name.as_deref()),
        Commands::GateCheck { name, strict } => run_gate_check(name.as_deref(), strict),
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

/// `ccsm list` — all sessions, one line each.  --active / --summary / --status filter / --verbose.
fn run_list(active: bool, summary: bool, verbose: bool, status_filter: Option<&str>) -> anyhow::Result<()> {
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
        if let Some(fs) = filter
            && s.status != fs { continue; }
        if verbose {
            // Teammate scan mode: full goal + tags, one line per session
            let goal = if s.goal.is_empty() { "" } else { " — " };
            let tags = if s.tags.is_empty() {
                String::new()
            } else {
                format!("  [{}]", s.tags.join(", "))
            };
            println!("{:12}  {:30}  {}{}{}", s.status.to_string(), s.name, goal, s.goal, tags);
        } else {
            let goal = if s.goal.is_empty() { "" } else { " — " };
            let goal_text = if s.goal.len() > 80 {
                format!("{}{:.77}...", goal, &s.goal)
            } else {
                format!("{}{}", goal, &s.goal)
            };
            println!("{:12}  {:30}  {}", s.status.to_string(), s.name, goal_text.trim());
        }
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
    reg.updated = crate::registry::now_iso();
    reg.save(&workspace)?;
    // Re-borrow immutably for the status line
    let entry = reg.sessions.iter().find(|s| s.name == name).unwrap();
    println!("{:12}  {}  ← {}", entry.status.to_string(), entry.name, action);
    Ok(())
}

// ── Mutation commands ───────────────────────────────────────────────────

/// `ccsm attach <name> [session-id] [--pid <pid>]` — link a Claude session_id to an entry.
///
/// If neither session-id nor --pid is given, auto-discovers the most recently
/// updated live Claude session in this workspace.
fn run_attach(name: &str, session_id: Option<&str>, pid: Option<u32>, home: &std::path::Path) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;

    let resolved_sid = match (session_id.filter(|s| !s.is_empty()), pid) {
        (Some(sid), _) => {
            crate::registry::validate_session_id(sid)?;
            sid.to_string()
        }
        (_, Some(p)) => crate::registry::harvest_from_pid(home, p)?,
        _ => {
            // Auto-discover: prefer name match in session file, fall back to most recent
            let sessions = crate::session::load_all(
                &home.join(".claude").join("sessions"),
                Some(&workspace),
            )?;
            match sessions.as_slice() {
                [] => anyhow::bail!(
                    "no live Claude sessions found in this workspace.\n\
                     Is claude running? Start it first, then `ccsm attach {}`.",
                    name
                ),
                [s] => {
                    if s.session_id.is_empty() {
                        anyhow::bail!(
                            "session file for PID {} has no sessionId yet — wait for Claude to finish starting",
                            s.pid
                        );
                    }
                    eprintln!("auto-detected PID {} ({})", s.pid, s.display_name());
                    s.session_id.clone()
                }
                multiple => {
                    // Try exact name match in session file (from /rename)
                    let by_name: Vec<_> = multiple
                        .iter()
                        .filter(|s| s.name == name)
                        .collect();
                    match by_name.as_slice() {
                        [s] => {
                            eprintln!("auto-detected PID {} (name match: {})", s.pid, s.display_name());
                            s.session_id.clone()
                        }
                        [] => {
                            eprintln!("multiple live sessions in this workspace (none named '{}'):", name);
                            for s in multiple {
                                eprintln!(
                                    "  pid {}  {:16}  {}  {}",
                                    s.pid,
                                    s.display_name(),
                                    s.status,
                                    &s.session_id[..s.session_id.len().min(8)],
                                );
                            }
                            anyhow::bail!("pick one with --pid <pid>.");
                        }
                        _ => {
                            eprintln!("multiple sessions named '{}' — picking most recent:", name);
                            let s = &by_name[0];
                            eprintln!("  pid {}  {}  {}", s.pid, s.status, &s.session_id[..s.session_id.len().min(8)]);
                            s.session_id.clone()
                        }
                    }
                }
            }
        }
    };

    // Verify transcript exists
    let slug = crate::registry::project_slug(&workspace);
    let transcript = home.join(".claude").join("projects").join(&slug).join(format!("{resolved_sid}.jsonl"));
    if !transcript.exists() {
        eprintln!(
            "warning: transcript not found at {}\n  The session_id may be from a different workspace.",
            transcript.display(),
        );
    }

    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(&workspace)?;
    let entry = reg
        .sessions
        .iter_mut()
        .rev()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow::anyhow!("no session named '{}'", name))?;
    entry.session_id = resolved_sid.clone();
    reg.updated = crate::registry::now_iso();
    reg.save(&workspace)?;
    println!("attached    {}  ← session {}", name, &resolved_sid[..resolved_sid.len().min(8)]);
    Ok(())
}

/// `ccsm rename <old> <new>` — rename a session across registry, detail file,
/// live session files, and transcript.
fn run_rename(old: &str, new: &str, goal: Option<&str>, scope: Option<&str>, home: &std::path::Path, workspace: &std::path::Path) -> anyhow::Result<()> {
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;

    // Validate: old must exist, new must not
    let idx = reg
        .sessions
        .iter()
        .position(|s| s.name == old)
        .ok_or_else(|| anyhow::anyhow!("no session named '{}'", old))?;
    if reg.sessions.iter().any(|s| s.name == new) {
        anyhow::bail!("session '{}' already exists", new);
    }
    if !crate::registry::is_kebab_case(new) {
        anyhow::bail!(
            "'{}' is not kebab-case. Session names must be lowercase letters, digits, and hyphens.",
            new
        );
    }

    let sid = reg.sessions[idx].session_id.clone();
    let slug = crate::registry::project_slug(workspace);

    // 1. Append rename entries to transcript (if session_id is set)
    if !sid.is_empty() {
        let transcript = home
            .join(".claude")
            .join("projects")
            .join(&slug)
            .join(format!("{sid}.jsonl"));
        if transcript.exists() {
            let rename_line = format!(
                "{{\"type\":\"custom-title\",\"customTitle\":\"{}\",\"sessionId\":\"{}\"}}\n\
                 {{\"type\":\"agent-name\",\"agentName\":\"{}\",\"sessionId\":\"{}\"}}\n",
                new, sid, new, sid
            );
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&transcript)
                .with_context(|| format!("opening transcript for append: {}", transcript.display()))?;
            file.write_all(rename_line.as_bytes())
                .with_context(|| format!("appending rename to transcript: {}", transcript.display()))?;
            file.flush()
                .with_context(|| format!("flushing transcript: {}", transcript.display()))?;
            eprintln!("  transcript  appended custom-title + agent-name: {}", new);
        } else {
            eprintln!(
                "  transcript  not found — session may not have been spawned yet (skipping)"
            );
        }
    }

    // 2. Update live session files (best-effort, ephemeral)
    let sessions_dir = home.join(".claude").join("sessions");
    if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else { continue };
            if let Ok(session) = serde_json::from_str::<crate::session::Session>(&contents) {
                let ws = workspace.to_string_lossy().to_string();
                if session.name == old && session.cwd.starts_with(&ws) {
                    // Rewrite with updated name
                    let updated = contents.replace(
                        &format!("\"name\":\"{}\"", old),
                        &format!("\"name\":\"{}\"", new),
                    );
                    let _ = std::fs::write(&path, updated);
                    eprintln!("  session file  pid {}  name → {}", session.pid, new);
                }
            }
        }
    }

    // 3. Rename detail file
    let detail_old = workspace
        .join(".claude")
        .join("sessions")
        .join(format!("{old}.md"));
    let detail_new = workspace
        .join(".claude")
        .join("sessions")
        .join(format!("{new}.md"));
    if detail_old.exists() {
        std::fs::rename(&detail_old, &detail_new).with_context(|| {
            format!(
                "renaming detail file: {} → {}",
                detail_old.display(),
                detail_new.display()
            )
        })?;
        eprintln!("  detail file  {}.md → {}.md", old, new);
    }

    // 4. Snapshot old values for logging
    let old_goal = reg.sessions[idx].goal.clone();
    let old_scope = reg.sessions[idx].scope.clone();
    let has_topic_change = goal.is_some() || scope.is_some();

    // 5. Update detail file content — replace header, goal, scope
    if detail_new.exists()
        && let Ok(contents) = std::fs::read_to_string(&detail_new) {
            let mut updated = contents
                .replace(&format!("# Session: {}", old), &format!("# Session: {}", new));
            if let Some(g) = goal {
                updated = crate::registry::replace_detail_section(&updated, "## Goal", g);
            }
            if let Some(s) = scope {
                updated = crate::registry::replace_detail_section(&updated, "## Scope / Plan", s);
            }
            let _ = std::fs::write(&detail_new, &updated);
            eprintln!("  detail file  updated header");
        }

    // 6. Update registry entry
    reg.sessions[idx].name = new.to_string();
    if let Some(g) = goal {
        reg.sessions[idx].goal = g.to_string();
    }
    if let Some(s) = scope {
        reg.sessions[idx].scope = s.to_string();
    }
    reg.updated = crate::registry::now_iso();
    reg.save(workspace)?;

    // 7. Log the rename to progress log (include old values when topic changed)
    if detail_new.exists() {
        let ts = crate::registry::note_timestamp();
        let mut note_parts = vec![format!("Renamed from '{}' to '{}'", old, new)];
        if has_topic_change {
            note_parts.push(format!("Old goal: {}", old_goal));
            if !old_scope.is_empty() {
                note_parts.push(format!("Old scope: {}", old_scope));
            }
        }
        let note_line = format!("- [{}] {}\n", ts, note_parts.join(" | "));
        if let Ok(contents) = std::fs::read_to_string(&detail_new) {
            let updated = crate::registry::insert_note(&contents, &note_line);
            let _ = std::fs::write(&detail_new, updated);
        }
    }

    println!("renamed     {} → {}", old, new);
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
        reg.updated = crate::registry::now_iso();
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
        reg.updated = crate::registry::now_iso();
        reg.save(&workspace)?;
        println!("recovered   {}  ← in_progress", name);
    } else {
        anyhow::bail!("no session named '{}'", name);
    }
    Ok(())
}

/// `ccsm clean <name>` — permanently delete transcript, session files, and registry entry.
fn run_clean(name: &str, home: &std::path::Path, workspace: &std::path::Path) -> anyhow::Result<()> {
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
    reg.updated = crate::registry::now_iso();
    reg.save(workspace)?;
    println!("cleaned     {}  ← permanently deleted", name);
    Ok(())
}

/// `ccsm clean-all` — permanently delete ALL trashed entries.
fn run_clean_all(home: &std::path::Path, workspace: &std::path::Path) -> anyhow::Result<()> {
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
    let count = reg.sessions.iter()
        .filter(|s| s.status == crate::registry::SessionStatus::Trashed)
        .count();
    if count == 0 {
        println!("(no trashed sessions)");
        return Ok(());
    }
    reg.clean_all_trashed(home, workspace);
    reg.updated = crate::registry::now_iso();
    reg.save(workspace)?;
    println!("cleaned     {} trashed session{}", count, if count == 1 { "" } else { "s" });
    Ok(())
}

/// `ccsm archive <name>` — delete transcript + session files, keep registry entry.
fn run_archive(name: &str, home: &std::path::Path, workspace: &std::path::Path) -> anyhow::Result<()> {
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
    let sid = reg.sessions.iter().rev()
        .find(|s| s.name == name)
        .map(|s| s.session_id.clone())
        .unwrap_or_default();

    if !reg.sessions.iter().any(|s| s.name == name) {
        anyhow::bail!("no session named '{}'", name);
    }

    // Check not active
    if let Some(s) = reg.sessions.iter().find(|s| s.name == name)
        && s.status == crate::registry::SessionStatus::InProgress {
            anyhow::bail!(
                "cannot archive active session '{}'. Complete or abandon it first.",
                name
            );
        }

    let freed = reg.archive(&sid, name, home, workspace);
    reg.updated = crate::registry::now_iso();
    reg.save(workspace)?;

    if freed > 0 {
        println!("archived    {}  ← freed {} MB", name, freed / 1_000_000);
    } else {
        println!("archived    {}  ← already archived (no transcript)", name);
    }
    Ok(())
}

/// `ccsm archive-all` — archive all completed sessions with transcripts.
fn run_archive_all(home: &std::path::Path, workspace: &std::path::Path) -> anyhow::Result<()> {
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
    let candidates: Vec<(String, String)> = reg
        .sessions
        .iter()
        .filter(|s| s.status == crate::registry::SessionStatus::Completed && !s.session_id.is_empty())
        .map(|s| (s.session_id.clone(), s.name.clone()))
        .collect();

    if candidates.is_empty() {
        println!("(no completed sessions with transcripts to archive)");
        return Ok(());
    }

    let mut total_freed: u64 = 0;
    for (sid, name) in &candidates {
        total_freed += reg.archive(sid, name, home, workspace);
    }
    reg.updated = crate::registry::now_iso();
    reg.save(workspace)?;

    println!(
        "archived    {} session{}  ← freed {} MB",
        candidates.len(),
        if candidates.len() == 1 { "" } else { "s" },
        total_freed / 1_000_000,
    );
    Ok(())
}

/// `ccsm new <name> [goal]` — create a new session entry.
fn run_new(name: &str, goal: &str, force: bool) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(&workspace)?;

    // Exact duplicate
    if reg.sessions.iter().any(|s| s.name == name) {
        anyhow::bail!("session '{}' already exists", name);
    }

    // Fuzzy duplicate — catch typos before creating garbage (skip with --force)
    if !force {
        let similar: Vec<&str> = reg
            .sessions
            .iter()
            .map(|s| s.name.as_str())
            .filter(|n| {
                // Substring: only flag if the overlap is significant (≥40% of longer name)
                let shorter = n.len().min(name.len());
                let longer = n.len().max(name.len());
                let significant_overlap = shorter >= 4 && (shorter as f64 / longer as f64) >= 0.4;
                (significant_overlap && (n.contains(name) || name.contains(*n)))
                    || (name.len() >= 4 && crate::registry::edit_distance(n, name) <= 2)
                    || crate::registry::edit_distance(n, name) <= 1
            })
            .take(3)
            .collect();
        if !similar.is_empty() {
            anyhow::bail!(
                "session '{}' looks similar to existing: {}. Use --force to create anyway, or `ccsm resume <name>` to continue an existing session.",
                name,
                similar.join(", "),
            );
        }
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
                retired_session_ids: vec![],
    });
    reg.updated = crate::registry::now_iso();
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
        if template.exists()
            && let Ok(contents) = std::fs::read_to_string(&template) {
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
                    .replace("{{now}}", &crate::registry::now_iso())
                    .replace("{{note}}", "Session created");
                if let Some(parent) = detail_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&detail_path, populated);
            }
    }

    println!("pending     {}  ← created", name);
    Ok(())
}

/// `ccsm start|complete|block|abandon <name>` — status transitions.
fn run_status(name: &str, action: &str, force: bool) -> anyhow::Result<()> {
    use crate::registry::SessionStatus;

    // Pre-completion gate: refuse to complete if detail file is hollow
    if action == "complete" && !force {
        if let Err(e) = run_gate_checks(name) {
            eprintln!(
                "✗ cannot complete: gate checks failed.\n\
                 \n  → ccsm close {} to see what's needed\n\
                 → ccsm complete {} --force to bypass\n\
                 \n{e}",
                name, name,
            );
            anyhow::bail!("gate checks failed — fix issues or use --force");
        }
        // Gate passed — print the self-review checklist
        println!();
        println!(
            "\
🔍 Self-review:
  [ ] Tests pass?
  [ ] All changes committed and pushed?
  [ ] Scope fulfilled? Anything left undocumented?
  [ ] Dependencies resolved?
  [ ] Detail file tags and progress log are current?"
        );
    }

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
            entry.completed = crate::registry::now_iso();
        }
    })
}

/// `ccsm refresh <name> [--reason]` — retire current Claude session, spawn fresh.
///
/// Use when the context window is bloated (>40%) and the model gets biased.
/// Moves the current session_id to `retired_session_ids` with timestamp and reason,
/// then spawns a fresh `claude` (no --resume) with `CCSM_SESSION` injected so the
/// new agent knows which ccsm session it serves.
fn run_refresh(name: &str, reason: Option<&str>, workspace: &PathBuf, home: &PathBuf) -> anyhow::Result<()> {
    let now = now_iso_ts();
    let reason_text = reason.unwrap_or("context refresh");

    // ── Phase 1: Retire current session_id, save, auto-note (locked) ──
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
        let entry = reg
            .sessions
            .iter_mut()
            .rev()
            .find(|e| e.name == name)
            .ok_or_else(|| anyhow::anyhow!("no session named '{}'", name))?;

        if entry.status != crate::registry::SessionStatus::InProgress {
            anyhow::bail!(
                "session '{}' is {} — only in_progress sessions can be refreshed",
                name, entry.status,
            );
        }

        // Retire current session_id if one exists
        if !entry.session_id.is_empty() {
            entry.retired_session_ids.push(crate::registry::RetiredSession {
                id: entry.session_id.clone(),
                retired_at: now.clone(),
                reason: reason_text.to_string(),
            });
        }

        // Clear identity fields — fresh session repopulates them
        entry.session_id.clear();
        entry.pids.clear();
        entry.started.clear();

        reg.updated = now.clone();
        reg.save(workspace)?;
    } // lock released

    // ── Phase 2: Auto-note to progress log ──────────────────────────
    let retired_count = {
        let reg = crate::registry::WorkspaceRegistry::load(workspace)?;
        reg.sessions
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.retired_session_ids.len())
            .unwrap_or(0)
    };

    let note_text = if retired_count <= 1 {
        format!("Refreshed session — fresh context (reason: {})", reason_text)
    } else {
        format!(
            "Refreshed session ({}th refresh) — fresh context (reason: {})",
            retired_count, reason_text,
        )
    };
    // Best-effort note — don't fail the whole operation if this errors
    let _ = run_note(name, &note_text, None);

    // ── Phase 3: Spawn fresh claude (no --resume) ────────────────────
    let mut cmd = std::process::Command::new("claude");
    cmd.current_dir(workspace);
    cmd.env("CCSM_SESSION", name);
    cmd.arg("-n").arg(name);

    if retired_count <= 1 {
        println!("refreshing  {}  ← claude (fresh, 1 refresh)", name);
    } else {
        println!("refreshing  {}  ← claude (fresh, {} refreshes)", name, retired_count);
    }

    let mut child = cmd.spawn()?;
    let child_pid = child.id();

    // ── Phase 4: Write pid to registry (locked) ──────────────────────
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
        match reg.sessions.iter_mut().rev().find(|e| e.name == name) {
            Some(entry) => entry.pids = vec![child_pid],
            None => anyhow::bail!(
                "internal error: session '{}' vanished from registry between Phase 1 and Phase 4",
                name
            ),
        }
        reg.updated = now_iso_ts();
        reg.save(workspace)?;
    }

    // ── Phase 5: Poll for session file, harvest session_id ───────────
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

    // ── Phase 6: Harvest session_id + started (locked) ───────────────
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
        let entry = match reg.sessions.iter_mut().rev().find(|e| e.name == name) {
            Some(e) => e,
            None => anyhow::bail!(
                "internal error: session '{}' vanished from registry between Phase 1 and Phase 6",
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

    // ── Phase 7: Wait for child ──────────────────────────────────────
    let status = child.wait()?;

    // ── Phase 8: Clear stale pids (locked) ───────────────────────────
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
    reg.updated = crate::registry::now_iso();
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
        let sections = crate::registry::parse_sections(&contents);
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
    if !session.retired_session_ids.is_empty() {
        println!("retired:    {} session{}", session.retired_session_ids.len(),
            if session.retired_session_ids.len() == 1 { "" } else { "s" });
        for r in &session.retired_session_ids {
            println!("  {}  {}", r.retired_at, r.reason);
        }
    }

    // ── Detail file sections ─────────────────────────────────────
    if detail_path.exists() {
        let contents = std::fs::read_to_string(&detail_path)?;
        let sections = crate::registry::parse_sections(&contents);
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
    let now = crate::registry::now_iso();
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
/// With --cross <source>, prepends "CROSS-SESSION [source]: " to the note.
fn run_note(name: &str, text: &str, cross: Option<&str>) -> anyhow::Result<()> {
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
    let ts = crate::registry::note_timestamp();

    let formatted = match cross {
        Some(source) => format!("CROSS-SESSION [{}]: {}", source, text),
        None => text.to_string(),
    };

    let new_entry = format!("- [{}] {}\n", ts, formatted);
    let display = if cross.is_some() { &formatted } else { text };

    let new_contents = crate::registry::insert_note(&contents, &new_entry);
    std::fs::write(&detail_path, new_contents)?;

    println!("noted       {}  ← [{}] {}", name, ts, display);
    Ok(())
}

// ── Helpers (now in registry.rs) ────────────────────────────────────────
// note_timestamp, days_to_date, is_leap, edit_distance, insert_note
// are imported from crate::registry.

// ── Doctor subcommand ──────────────────────────────────────────────────

/// Shared gate-check logic: returns Ok(()) if all hard checks pass,
/// Err with a human-readable message listing each failure otherwise.
/// Used by both `ccsm close` and the `ccsm complete` internal gate.
/// `ccsm note-check` — Stop-hook helper. If the in_progress session hasn't been
/// noted recently, emit a reminder. Time-based only — no git diff, no false positives
/// from stale uncommitted changes.
/// Auto-discovers the in_progress session. Silent when recently noted or no active session.
fn run_note_check() -> anyhow::Result<()> {
    use crate::registry::SessionStatus;

    let workspace = std::env::current_dir()?;

    // Find in_progress session. CCSM_SESSION env var is authoritative if set.
    let reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
    let session = {
        let env_session = std::env::var("CCSM_SESSION").ok();
        let s = if let Some(ref n) = env_session {
            reg.sessions.iter().find(|s| s.name == *n && s.status == SessionStatus::InProgress)
        } else {
            None
        };
        match s.or_else(|| reg.sessions.iter().find(|s| s.status == SessionStatus::InProgress)) {
            Some(s) => s,
            None => return Ok(()), // no active session → silent
        }
    };

    // Check detail file note recency
    let detail_path = workspace
        .join(".claude")
        .join("sessions")
        .join(format!("{}.md", session.name));
    if !detail_path.exists() {
        return Ok(()); // no detail file → skip
    }

    let Ok(contents) = std::fs::read_to_string(&detail_path) else {
        return Ok(());
    };
    let sections = parse_sections(&contents);
    let pl_body = sections
        .iter()
        .find(|(h, _)| h.to_lowercase().contains("progress"))
        .map(|(_, b)| b.as_str())
        .unwrap_or("");

    // Find most recent `- [YYYY-MM-DD HH:MMZ]` entry
    let last_ts = pl_body
        .lines()
        .filter_map(|l| {
            let t = l.trim_start();
            if t.starts_with("- [") {
                t.get(3..19) // "YYYY-MM-DD HH:MMZ"
            } else {
                None
            }
        })
        .last();

    let stale = match last_ts {
        Some(ts) => {
            // Parse the timestamp and compare to current time
            // Format: "YYYY-MM-DD HH:MMZ"
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let note_secs = parse_note_timestamp(ts).unwrap_or(0);
            now_secs.saturating_sub(note_secs) > 120 // > 2 min
        }
        None => true, // no notes at all
    };

    if stale {
        eprintln!(
            "\
⚡ If this turn modified/updated/changed anything, update the session detail.
  → ccsm note {} \"<what you changed and why>\"",
            session.name,
        );
    }

    Ok(())
}

/// Parse "YYYY-MM-DD HH:MMZ" into seconds since epoch.
fn parse_note_timestamp(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split(&['-', ' ', ':', 'Z'][..]).collect();
    if parts.len() < 6 {
        return None;
    }
    let year: i64 = parts[0].parse().ok()?;
    let month: i64 = parts[1].parse().ok()?;
    let day: i64 = parts[2].parse().ok()?;
    let hour: i64 = parts[3].parse().ok()?;
    let min: i64 = parts[4].parse().ok()?;
    // Simple days-since-epoch (approximate, good enough for 60-min staleness check)
    let mut days = 0i64;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    let mdays: [i64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    for m in 0..(month - 1) as usize {
        days += mdays[m];
    }
    days += day - 1;
    let secs = (days * 86400) as u64 + (hour as u64 * 3600) + (min as u64 * 60);
    Some(secs)
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn run_gate_checks(name: &str) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let reg = crate::registry::WorkspaceRegistry::load(&workspace)?;
    if !reg.sessions.iter().any(|s| s.name == name) {
        anyhow::bail!("no session named '{}'", name);
    }

    let detail_path = workspace
        .join(".claude")
        .join("sessions")
        .join(format!("{}.md", name));

    let mut failures: Vec<String> = Vec::new();

    if !detail_path.exists() {
        failures.push(format!(
            "  no detail file → cp .claude/session-detail-template.md {}",
            detail_path.display(),
        ));
        return Err(format_err(&failures, name));
    }

    let contents = std::fs::read_to_string(&detail_path)?;
    let sections = parse_sections(&contents);

    // Scope/Plan
    let scope_body = sections
        .iter()
        .find(|(h, _)| h.to_lowercase().contains("scope"))
        .map(|(_, b)| b.as_str())
        .unwrap_or("");
    if scope_body.trim().is_empty() || scope_body.contains("(fill in") {
        failures.push("  Scope/Plan is empty or still template".into());
    }

    // Tags
    let tags_body = sections
        .iter()
        .find(|(h, _)| h.to_lowercase() == "tags")
        .map(|(_, b)| b.as_str())
        .unwrap_or("");
    if tags_body.trim().is_empty() || tags_body.contains("(fill in") {
        failures.push("  Tags is empty or still template".into());
    }

    // Progress Log ≥ 2 entries
    let pl_body = sections
        .iter()
        .find(|(h, _)| h.to_lowercase().contains("progress"))
        .map(|(_, b)| b.as_str())
        .unwrap_or("");
    let note_count = pl_body
        .lines()
        .filter(|l| l.trim_start().starts_with("- ["))
        .count();
    if note_count < 2 {
        failures.push(format!(
            "  Progress Log has {} substantive entr{} (need ≥ 2)",
            note_count,
            if note_count == 1 { "y" } else { "ies" },
        ));
    }

    // Live Session Data not template
    let lsd_body = sections
        .iter()
        .find(|(h, _)| h.to_lowercase().contains("live session"))
        .map(|(_, b)| b.trim())
        .unwrap_or("");
    if lsd_body.contains("(auto") || lsd_body.is_empty() {
        failures.push("  Live Session Data still has template placeholders".into());
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format_err(&failures, name))
    }
}

fn format_err(failures: &[String], name: &str) -> anyhow::Error {
    let mut msg = String::from("✗ gate failures:\n");
    for f in failures {
        msg.push_str(f);
        msg.push('\n');
    }
    msg.push_str(&format!(
        "  → edit .claude/sessions/{}.md",
        name,
    ));
    anyhow::anyhow!("{}", msg)
}

/// `ccsm close <name>` — pre-completion gate. Run before `ccsm complete`.
///
/// **Hard checks** (exit non-zero on violation): detail file, scope, tags,
/// progress log, live session data.
/// **Self-review checklist** (always printed on pass).
fn run_close(name: &str) -> anyhow::Result<()> {
    run_gate_checks(name)?;

    println!(
        "\
🔍 Self-review:
  [ ] Tests pass?
  [ ] All changes committed and pushed?
  [ ] Scope fulfilled? Anything left undocumented?
  [ ] Dependencies resolved?
  [ ] Detail file tags and progress log are current?"
    );

    Ok(())
}

// ── Completions subcommand ─────────────────────────────────────────────

/// `ccsm completions <shell>` — generate shell completion script to stdout.
fn run_completions(shell: &str) -> anyhow::Result<()> {
    use clap::CommandFactory;
    use clap_complete::{Shell, generate};

    let mut cmd = Cli::command();
    let bin_name = "ccsm";

    match shell {
        "bash" => generate(Shell::Bash, &mut cmd, bin_name, &mut std::io::stdout()),
        "fish" => generate(Shell::Fish, &mut cmd, bin_name, &mut std::io::stdout()),
        "zsh" => generate(Shell::Zsh, &mut cmd, bin_name, &mut std::io::stdout()),
        other => {
            anyhow::bail!(
                "unknown shell '{}'. Supported: bash, fish, zsh",
                other
            );
        }
    }
    Ok(())
}

// ── InjectScope subcommand ───────────────────────────────────────────────

/// `ccsm inject-scope [--name <name>]` — output a `<system-reminder>` block
/// with the active session's goal and scope.  Designed for the SystemMessage
/// hook so the agent sees its current task constraints on every turn.
fn run_inject_scope(name: Option<&str>) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let reg = crate::registry::WorkspaceRegistry::load(&workspace)?;

    let session = match name {
        Some(n) => reg
            .sessions
            .iter()
            .find(|s| s.name == n)
            .ok_or_else(|| anyhow::anyhow!("no session named '{}'", n))?,
        None => {
            // CCSM_SESSION env var is injected at spawn time — it's the
            // deterministic source of truth for which session this agent
            // belongs to.  The in_progress scan is a fallback only.
            let env_session = std::env::var("CCSM_SESSION").ok();
            if let Some(ref n) = env_session {
                if let Some(s) = reg.sessions.iter().find(|s| s.name == *n) {
                    s
                } else {
                    // CCSM_SESSION name not found in this workspace registry —
                    // agent might be in a different workspace. Fall through.
                    let active: Vec<_> = reg
                        .sessions
                        .iter()
                        .filter(|s| s.status == crate::registry::SessionStatus::InProgress)
                        .collect();
                    match active.as_slice() {
                        [] => return Ok(()),
                        [s] => *s,
                        multiple => {
                            eprintln!(
                                "warning: CCSM_SESSION={} not found, {} in_progress — injecting first: {}",
                                n,
                                multiple.len(),
                                multiple[0].name
                            );
                            multiple[0]
                        }
                    }
                }
            } else {
                let active: Vec<_> = reg
                    .sessions
                    .iter()
                    .filter(|s| s.status == crate::registry::SessionStatus::InProgress)
                    .collect();
                match active.as_slice() {
                    [] => return Ok(()),
                    [s] => *s,
                    multiple => {
                        eprintln!(
                            "warning: {} in_progress sessions — injecting first: {}",
                            multiple.len(),
                            multiple[0].name
                        );
                        multiple[0]
                    }
                }
            }
        }
    };

    println!("<system-reminder>");
    println!("ACTIVE SESSION: {}", session.name);
    if !session.goal.is_empty() {
        println!("GOAL: {}", session.goal);
    }
    if !session.scope.is_empty() && !session.scope.contains("(fill in") {
        println!("SCOPE: {}", session.scope);
    }
    println!(
        "CONSTRAINT: Work within this scope. If you need to do something outside it, ask first."
    );
    println!("</system-reminder>");
    Ok(())
}

// ── GateCheck subcommand ──────────────────────────────────────────────────

/// `ccsm gate-check [--name <name>] [--strict]` — validate session readiness
/// before completing.  Designed for the Stop hook.
///
/// Checks: scope/goal presence, git diff surface, untracked files.
/// Exit 0 = pass, 1 = fail (for hook integration).
fn run_gate_check(name: Option<&str>, strict: bool) -> anyhow::Result<()> {
    let workspace = std::env::current_dir()?;
    let reg = crate::registry::WorkspaceRegistry::load(&workspace)?;

    let session = match name {
        Some(n) => reg
            .sessions
            .iter()
            .find(|s| s.name == n)
            .ok_or_else(|| anyhow::anyhow!("no session named '{}'", n))?,
        None => {
            // CCSM_SESSION env var is injected at spawn time.
            let env_session = std::env::var("CCSM_SESSION").ok();
            if let Some(ref n) = env_session {
                if let Some(s) = reg.sessions.iter().find(|s| s.name == *n) {
                    s
                } else {
                    // CCSM_SESSION name not in this workspace's registry —
                    // fall through to in_progress scan.
                    let active: Vec<_> = reg
                        .sessions
                        .iter()
                        .filter(|s| s.status == crate::registry::SessionStatus::InProgress)
                        .collect();
                    match active.as_slice() {
                        [] => {
                            println!("GATE: NO_ACTIVE_SESSION — nothing to gate");
                            return Ok(());
                        }
                        [s] => *s,
                        multiple => {
                            println!("GATE: MULTIPLE_ACTIVE — {} in_progress sessions. Pass --name.", multiple.len());
                            for s in multiple {
                                println!("  - {}", s.name);
                            }
                            return Ok(());
                        }
                    }
                }
            } else {
                let active: Vec<_> = reg
                    .sessions
                    .iter()
                    .filter(|s| s.status == crate::registry::SessionStatus::InProgress)
                    .collect();
                match active.as_slice() {
                    [] => {
                        println!("GATE: NO_ACTIVE_SESSION — nothing to gate");
                        return Ok(());
                    }
                    [s] => *s,
                    multiple => {
                        println!("GATE: MULTIPLE_ACTIVE — {} in_progress sessions. Pass --name.", multiple.len());
                        for s in multiple {
                            println!("  - {}", s.name);
                        }
                        return Ok(());
                    }
                }
            }
        }
    };

    let mut fail = false;

    // ── Scope ──────────────────────────────────────────────────────────
    let scope_empty = session.scope.is_empty() || session.scope.contains("(fill in");
    if scope_empty {
        if strict {
            println!("GATE: FAIL — scope is empty or unfilled");
            fail = true;
        } else {
            println!(
                "GATE: WARN — scope is empty. Set: ccsm scope {} \"...\"",
                session.name
            );
        }
    }

    // ── Goal ──────────────────────────────────────────────────────────
    if session.goal.is_empty() {
        if strict {
            println!("GATE: FAIL — goal is empty");
            fail = true;
        } else {
            println!("GATE: WARN — goal is empty");
        }
    }

    // ── Git diff ──────────────────────────────────────────────────────
    if let Ok(output) = std::process::Command::new("git")
        .args(["diff", "--stat", "HEAD"])
        .current_dir(&workspace)
        .output()
        && !output.stdout.is_empty() {
            let stat = String::from_utf8_lossy(&output.stdout);
            println!("GATE: CHANGED FILES");
            let lines: Vec<&str> = stat.lines().collect();
            for line in lines.iter().take(20) {
                println!("  {}", line);
            }
            if lines.len() > 20 {
                println!("  ... and {} more lines", lines.len() - 20);
            }
        }

    // ── Untracked files ───────────────────────────────────────────────
    if let Ok(output) = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(&workspace)
        .output()
        && !output.stdout.is_empty() {
            let text = String::from_utf8_lossy(&output.stdout);
            let files: Vec<&str> = text.lines().collect();
            if !files.is_empty() {
                println!("GATE: UNTRACKED ({})", files.len());
                for f in files.iter().take(10) {
                    println!("  ? {}", f);
                }
            }
        }

    if fail {
        std::process::exit(1);
    }
    println!("GATE: PASS — '{}' is ready for review", session.name);
    Ok(())
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
