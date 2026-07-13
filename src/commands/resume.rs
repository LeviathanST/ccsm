use std::path::Path;

// ── ChildGuard — RAII cleanup for spawned processes ─────────────────────

/// Wraps a [`std::process::Child`] and ensures it is cleaned up on drop.
/// Sends SIGTERM first (allowing graceful shutdown and state save), then
/// SIGKILL after a 5s grace period if the child hasn't exited.
///
/// Call [`ChildGuard::wait`] to block until the child exits normally —
/// this disarms the guard so drop becomes a no-op.
struct ChildGuard {
    child: Option<std::process::Child>,
}

impl ChildGuard {
    fn new(child: std::process::Child) -> Self {
        Self { child: Some(child) }
    }

    fn id(&self) -> u32 {
        self.child.as_ref().expect("ChildGuard consumed").id()
    }

    /// Wait for the child to exit normally.  Disarms the guard on success
    /// so [`Drop`] won't try to kill an already-exited process.
    fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let status = self.child.as_mut().expect("ChildGuard consumed").wait()?;
        self.child = None; // disarm — child already reaped
        Ok(status)
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else { return };
        let pid = child.id();
        eprintln!("ccsm: cleaning up child process (pid {pid})");

        // SIGTERM — gives the child a chance to save state
        unsafe { libc::kill(pid as i32, libc::SIGTERM); }

        // Wait up to 5s for graceful exit
        for _ in 0..50 {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(_) => break,
            }
        }

        // SIGKILL — force kill if still alive
        eprintln!("ccsm: child pid {pid} didn't exit gracefully, sending SIGKILL");
        let _ = child.kill();
        let _ = child.wait();
    }
}

// ── Resume subcommand ───────────────────────────────────────────────────

/// `ccsm resume <name> [--worktree]` — promote entry, spawn agent (claude/pi/cmd) with resume or fresh.
/// With `--worktree`, creates a git worktree first and resumes inside it.
pub fn run_resume(name: &str, workspace: &Path, home: &Path, consumer: crate::consumer::Consumer, flag_worktree: bool) -> anyhow::Result<()> {
    let now = crate::registry::now_iso();
    let bin = consumer.binary();

    // ── Phase 1: Promote entry (locked) ────────────────────────────
    let (sid, fresh) = {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;

        let (sid, is_fresh) = match reg.sessions.iter().rev().position(|e| e.name == name) {
            Some(pos) => {
                let i = reg.sessions.len() - 1 - pos;
                let entry = &mut reg.sessions[i];

                entry.status = crate::registry::SessionStatus::InProgress;
                entry.started.clear();

                // ── Cross-consumer detection ────────────────────
                let current = consumer.to_string();
                if !entry.consumer.is_empty() && entry.consumer != current && !entry.session_id.is_empty() {
                    // Session has an id but consumer doesn't match
                    let found = consumer.find_session_file_for(home, workspace, &entry.session_id);
                    let location = if found.is_some() {
                        format!("found by {current}")
                    } else {
                        format!("stored by {} and not accessible from {}", entry.consumer, current)
                    };
                    eprintln!(
                        "⚠  session '{}' was created by {} but you are running as {}",
                        name, entry.consumer, current
                    );
                    eprintln!("   Session file is {location}.");
                    eprint!("Start a fresh {current} session instead? [y/N] ");
                    use std::io::{self, Write};
                    let _ = io::stdout().flush();
                    let mut input = String::new();
                    let ok = io::stdin().read_line(&mut input)
                        .map(|_| matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
                        .unwrap_or(false);
                    if !ok {
                        eprintln!();
                        eprintln!("   Aborted. To resume anyway without fresh:");
                        eprintln!("     ccsm {}  (use the original agent)", entry.consumer);
                        eprintln!("   To start fresh:");
                        eprintln!("     ccsm pending {}  (clears session identity)", name);
                        anyhow::bail!("resume aborted by user");
                    }
                    eprintln!();
                    // Clear session_id — starts fresh with current consumer
                    entry.session_id.clear();
                    entry.consumer = current;
                    (None, false)
                } else if !entry.consumer.is_empty() && entry.consumer != current {
                    // No session_id yet but consumer mismatch — warn and continue
                    eprintln!(
                        "⚠  session '{}' was created by {} but you are running as {}",
                        name, entry.consumer, current
                    );
                    eprintln!("   Starting a fresh session for {}.", current);
                    entry.consumer = current;
                    (None, false)
                } else {
                    // Consumer matches or unset — normal flow
                    if entry.consumer.is_empty() {
                        entry.consumer = current;
                    }
                    if !entry.session_id.is_empty() {
                        let found = consumer.find_session_file_for(home, workspace, &entry.session_id);
                        if found.is_some() {
                            (Some(entry.session_id.clone()), false)
                        } else {
                            anyhow::bail!(
                                "session '{}' has session_id '{}' but session file not found.\n\
                                 The session may have been deleted or cleaned.\n\
                                 To start fresh: ccsm pending {}  (clears session_id, then resume)",
                                name,
                                &entry.session_id[..entry.session_id.len().min(8)],
                                name,
                            );
                        }
                    } else {
                        (None, false)
                    }
                }
            }
            None => {
                let similar: Vec<&str> = reg
                    .sessions
                    .iter()
                    .map(|s| s.name.as_str())
                    .filter(|n| {
                        n.contains(name)
                            || (name.len() >= 4 && crate::registry::edit_distance(n, name) <= 2)
                            || crate::registry::edit_distance(n, name) <= 1
                    })
                    .take(5)
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

    // ── Nudge: check if session has a checklist section ──────────────
    let detail_path = workspace
        .join(".ccsm")
        .join("sessions")
        .join(format!("{}.md", name));
    if detail_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&detail_path) {
            let has_checklist = contents
                .lines()
                .any(|l| l.trim_start().to_lowercase().starts_with("## checklist"));
            if !has_checklist {
                eprintln!(
                    "💡 multi-step? `ccsm checklist {} --init` to add sub-task tracking",
                    name,
                );
            }
        }
    }

    // ── Phase 1b: Create worktree if --worktree flag or config required ─
    let config = crate::config::Config::load(workspace);
    let should_create_worktree = match config.worktrees {
        crate::config::WorktreePolicy::Required => true,
        crate::config::WorktreePolicy::Optional => flag_worktree,
        crate::config::WorktreePolicy::Disabled => false,
    };

    if should_create_worktree {
        eprintln!("  preparing worktree...");
        let (branch, existing_wt) = {
            let reg = crate::registry::WorkspaceRegistry::load(workspace)?;
            let session = reg.sessions.iter()
                .find(|s| s.name == name)
                .ok_or_else(|| anyhow::anyhow!("session '{}' not found", name))?;
            let branch = session.branch.clone();
            anyhow::ensure!(!branch.is_empty(),
                "session '{}' has no target branch. Set one with `ccsm new -b <branch>`.", name,
            );
            // Check registry first, then canonical filesystem path
            let existing = if !session.worktree.is_empty() {
                let p = std::path::PathBuf::from(&session.worktree);
                if p.is_dir() { Some(p) } else { None }
            } else {
                let canonical = crate::commands::worktree::worktree_path_for(workspace, name);
                if canonical.is_dir() { Some(canonical) } else { None }
            };
            (branch, existing)
        };

        let wt_path = match existing_wt {
            Some(path) => {
                eprintln!("📁 worktree already exists: {}", path.display());
                path
            }
            None => crate::commands::worktree::create_worktree(workspace, name, &branch)?,
        };

        // Store worktree path + enable use_worktree (locked)
        {
            let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
            if let Some(entry) = reg.sessions.iter_mut().rev().find(|e| e.name == name) {
                entry.worktree = wt_path.to_string_lossy().to_string();
                entry.use_worktree = true;
                reg.updated = crate::registry::now_iso();
                reg.save(workspace)?;
            }
        }
        eprintln!("📁 worktree: {}", wt_path.display());
    }

    // ── Phase 2: Determine worktree directory (no lock) ──────────────
    let worktree_dir: Option<std::path::PathBuf> = {
        let reg = crate::registry::WorkspaceRegistry::load(workspace)?;
        reg.sessions.iter()
            .find(|s| s.name == name)
            .and_then(|s| {
                if !s.worktree.is_empty() {
                    let wt = std::path::PathBuf::from(&s.worktree);
                    if wt.is_dir() { Some(wt) } else { None }
                } else { None }
            })
    };

    // ── Phase 3: Spawn agent (no lock) ──────────────────────────────
    let mut cmd = std::process::Command::new(bin);
    if let Some(ref wt) = worktree_dir {
        cmd.current_dir(wt);
        cmd.env("CCSM_WORKTREE", wt);
        eprintln!("📁 worktree: {}", wt.display());
    } else {
        cmd.current_dir(workspace);
    }
    cmd.env("CCSM_SESSION", name);

    match consumer {
        crate::consumer::Consumer::Claude => {
            if let Some(ref id) = sid {
                cmd.arg("--resume").arg(id);
                println!("resuming    {}  ← {bin} --resume {}", name, &id[..id.len().min(8)]);
            } else if fresh {
                println!("starting    {}  ← {bin} (fresh)", name);
            } else {
                println!("starting    {}  ← {bin} (new session)", name);
            }
            cmd.arg("-n").arg(name);
        }
        crate::consumer::Consumer::Pi => {
            if let Some(ref id) = sid {
                cmd.arg("--session").arg(id);
                println!("resuming    {}  ← pi --session {}", name, &id[..id.len().min(8)]);
            } else {
                // Fresh ccsm entry → start a new Pi session
                println!("starting    {}  ← pi (fresh session)", name);
            }
            cmd.arg("-n").arg(name);
        }
    }

    let mut child_guard = ChildGuard::new(cmd.spawn()?);
    let child_pid = child_guard.id();

    // ── Phase 4: Write pid to registry (locked) ─────────────────────
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
        match reg.sessions.iter_mut().rev().find(|e| e.name == name) {
            Some(entry) => entry.pids = vec![child_pid],
            None => anyhow::bail!(
                "internal error: session '{}' vanished from registry between Phase 1 and Phase 4",
                name
            ),
        }
        reg.updated = crate::registry::now_iso();
        reg.save(workspace)?;
    }

    // ── Phase 5: Harvest session_id (consumer-specific) ─────────────
    if consumer.is_claude() {
        // Claude: poll for PID-based session file
        let session_file = consumer.live_session_file(home, child_pid)
            .ok_or_else(|| anyhow::anyhow!("consumer does not support PID-based session files"))?;
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
                "{bin} did not write a session file at {} within 5s.\n\
                 {bin} may have failed to start. Check for errors above.",
                session_file.display(),
            );
        }

        // Harvest session_id + started (locked)
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
                        reg.updated = crate::registry::now_iso();
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

        // Sync detail file status line with harvested started time
        crate::registry::sync_status_line(workspace, name);
    } else {
        // Pi: session_id already set via --session — no PID file to harvest
        eprintln!("  (session tracking active)");
    }

    // ── Phase 6: Wait for child ─────────────────────────────────────
    let status = child_guard.wait()?;

    // ── Phase 7: Clear stale pids (locked) ──────────────────────────
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
        match reg.sessions.iter_mut().rev().find(|e| e.name == name) {
            Some(entry) => {
                entry.pids.clear();
                reg.updated = crate::registry::now_iso();
            }
            None => {
                eprintln!(
                    "warning: session '{}' not found in registry at cleanup — \
                     may have been removed while {bin} was running",
                    name,
                );
            }
        }
        reg.save(workspace)?;
    }

    if !status.success() {
        anyhow::bail!("{bin} exited with {status}");
    }
    Ok(())
}
