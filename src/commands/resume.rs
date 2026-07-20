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
        let Some(mut child) = self.child.take() else {
            return;
        };
        let pid = child.id();
        eprintln!("ccsm: cleaning up child process (pid {pid})");

        // SIGTERM — gives the child a chance to save state
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }

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
pub fn run_resume(
    name: &str,
    workspace: &Path,
    home: &Path,
    consumer: crate::consumer::Consumer,
    flag_worktree: bool,
) -> anyhow::Result<()> {
    let now = crate::registry::now_iso();
    let bin = consumer.binary();

    // ── Phase 1: Promote entry (locked) ────────────────────────────
    let (sid, fresh) = {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked()?;

        let (sid, is_fresh) = match reg.sessions.iter().rev().position(|e| e.name == name) {
            Some(pos) => {
                let i = reg.sessions.len() - 1 - pos;
                let entry = &mut reg.sessions[i];

                entry.status = crate::registry::SessionStatus::InProgress;
                entry.started.clear();

                // ── Cross-consumer detection ────────────────────
                let current = consumer.to_string();
                if !entry.consumer.is_empty()
                    && entry.consumer != current
                    && !entry.session_id.is_empty()
                {
                    // Session has an id but consumer doesn't match
                    let found = consumer.find_session_file_for(home, workspace, &entry.session_id);
                    let location = if found.is_some() {
                        format!("found by {current}")
                    } else {
                        format!(
                            "stored by {} and not accessible from {}",
                            entry.consumer, current
                        )
                    };
                    eprintln!(
                        "{} session '{}' was created by {} but you are running as {}",
                        crate::style::emoji("⚠", "[!]"),
                        name,
                        entry.consumer,
                        current
                    );
                    eprintln!("   Session file is {location}.");
                    eprintln!(
                        "   To resume: ccsm {}  (use the original agent)",
                        entry.consumer
                    );
                    eprintln!(
                        "   To start fresh: ccsm pending {}  (clears session_id)",
                        name
                    );
                    anyhow::bail!(
                        "switch to {} to resume this session, or `ccsm pending` to start fresh",
                        entry.consumer
                    );
                } else if !entry.consumer.is_empty() && entry.consumer != current {
                    // No session_id yet but consumer mismatch — warn and continue
                    eprintln!(
                        "{} session '{}' was created by {} but you are running as {}",
                        crate::style::emoji("⚠", "[!]"),
                        name,
                        entry.consumer,
                        current
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
                        let found =
                            consumer.find_session_file_for(home, workspace, &entry.session_id);
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
                        name,
                        name
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
        reg.save()?;
        (sid, is_fresh)
    }; // lock released

    // ── Nudge: check if session has a checklist section ──────────────
    let detail_path = match crate::registry::resolve_identity() {
        Ok(ctx) => crate::registry::global_detail_path(&ctx.id, name),
        Err(_) => {
            // Can't resolve workspace identity — skip nudge
            return Ok(());
        }
    };
    if detail_path.exists()
        && let Ok(contents) = std::fs::read_to_string(&detail_path)
    {
        let has_checklist = contents
            .lines()
            .any(|l| l.trim_start().to_lowercase().starts_with("## checklist"));
        if !has_checklist {
            eprintln!(
                "{} multi-step? `ccsm checklist {} --init` to add sub-task tracking",
                crate::style::emoji("💡", "[i]"),
                name,
            );
        }
    }

    // ── Phase 1b: Create worktree if --worktree flag or config required ─
    let config = crate::config::Config::load();
    let should_create_worktree = match config.worktrees {
        crate::config::WorktreePolicy::Required => true,
        crate::config::WorktreePolicy::Optional => flag_worktree,
        crate::config::WorktreePolicy::Disabled => false,
    };

    if should_create_worktree {
        eprintln!("  preparing worktree...");
        let (branch, existing_wt) = {
            let reg = crate::registry::WorkspaceRegistry::load()?;
            let session = reg
                .sessions
                .iter()
                .find(|s| s.name == name)
                .ok_or_else(|| anyhow::anyhow!("session '{}' not found", name))?;
            let branch = session.branch.clone();
            anyhow::ensure!(
                !branch.is_empty(),
                "session '{}' has no target branch. Set one with `ccsm new -b <branch>`.",
                name,
            );
            // Worktree path is derived deterministically from workspace + name
            let canonical = crate::commands::worktree::worktree_path_for(workspace, name);
            let existing = if canonical.is_dir() {
                Some(canonical)
            } else {
                None
            };
            (branch, existing)
        };

        let wt_path = match existing_wt {
            Some(path) => {
                eprintln!(
                    "{} worktree already exists: {}",
                    crate::style::emoji("📁", "[dir]"),
                    path.display()
                );
                path
            }
            None => crate::commands::worktree::create_worktree(workspace, name, &branch)?,
        };

        // Enable use_worktree (locked)
        {
            let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked()?;
            if let Some(entry) = reg.sessions.iter_mut().rev().find(|e| e.name == name) {
                entry.use_worktree = true;
                reg.updated = crate::registry::now_iso();
                reg.save()?;
            }
        }
        eprintln!(
            "{} worktree: {}",
            crate::style::emoji("📁", "[dir]"),
            wt_path.display()
        );
    }

    // ── Phase 2: Determine worktree directory (no lock) ──────────────
    let worktree_dir: Option<std::path::PathBuf> = {
        let wt = crate::commands::worktree::worktree_path_for(workspace, name);
        if wt.is_dir() { Some(wt) } else { None }
    };

    // ── Phase 3: Spawn agent (no lock) ──────────────────────────────

    // When a worktree is active, wrap in `sh -c "cd <wt> && exec $bin <args>"`.
    // This ensures claude's session file records the worktree as cwd, so on
    // subsequent --resume the agent lands in the worktree, not the workspace root.
    let mut cmd = if let Some(ref wt) = worktree_dir {
        eprintln!(
            "{} worktree: {}",
            crate::style::emoji("📁", "[dir]"),
            wt.display()
        );
        let wt_str = wt.to_string_lossy();
        let inner = match consumer {
            crate::consumer::Consumer::Claude => {
                let (flag, label) = if let Some(ref id) = sid {
                    (
                        format!("--resume {}", id),
                        format!(
                            "resuming    {}  ← {bin} --resume {}",
                            name,
                            &id[..id.len().min(8)]
                        ),
                    )
                } else if fresh {
                    (
                        String::new(),
                        format!("starting    {}  ← {bin} (fresh)", name),
                    )
                } else {
                    (
                        String::new(),
                        format!("starting    {}  ← {bin} (new session)", name),
                    )
                };
                println!("{}", label);
                format!("cd '{}' && exec {} {} -n '{}'", wt_str, bin, flag, name)
            }
            crate::consumer::Consumer::Pi => {
                let (flag, label) = if let Some(ref id) = sid {
                    (
                        format!("--session {}", id),
                        format!(
                            "resuming    {}  ← pi --session {}",
                            name,
                            &id[..id.len().min(8)]
                        ),
                    )
                } else {
                    (
                        String::new(),
                        format!("starting    {}  ← pi (fresh session)", name),
                    )
                };
                println!("{}", label);
                format!("cd '{}' && exec {} {} -n '{}'", wt_str, bin, flag, name)
            }
            crate::consumer::Consumer::OpenCode => {
                let (flag, label) = if let Some(ref id) = sid {
                    (
                        format!("-s {}", id),
                        format!(
                            "resuming    {}  ← opencode -s {}",
                            name,
                            &id[..id.len().min(8)]
                        ),
                    )
                } else {
                    (
                        String::new(),
                        format!("starting    {}  ← opencode (fresh)", name),
                    )
                };
                println!("{}", label);
                format!("cd '{}' && exec opencode {}", wt_str, flag)
            }
        };
        let mut s = std::process::Command::new("sh");
        s.arg("-c").arg(&inner);
        s
    } else {
        let mut c = std::process::Command::new(bin);
        c.current_dir(workspace);
        match consumer {
            crate::consumer::Consumer::Claude => {
                if let Some(ref id) = sid {
                    c.arg("--resume").arg(id);
                    println!(
                        "resuming    {}  ← {bin} --resume {}",
                        name,
                        &id[..id.len().min(8)]
                    );
                } else if fresh {
                    println!("starting    {}  ← {bin} (fresh)", name);
                } else {
                    println!("starting    {}  ← {bin} (new session)", name);
                }
                c.arg("-n").arg(name);
            }
            crate::consumer::Consumer::Pi => {
                if let Some(ref id) = sid {
                    c.arg("--session").arg(id);
                    println!(
                        "resuming    {}  ← pi --session {}",
                        name,
                        &id[..id.len().min(8)]
                    );
                } else {
                    println!("starting    {}  ← pi (fresh session)", name);
                }
                c.arg("-n").arg(name);
            }
            crate::consumer::Consumer::OpenCode => {
                if let Some(ref id) = sid {
                    c.arg("-s").arg(id);
                    println!(
                        "resuming    {}  ← opencode -s {}",
                        name,
                        &id[..id.len().min(8)]
                    );
                } else {
                    println!("starting    {}  ← opencode (fresh)", name);
                }
                // OpenCode TUI has no name/title flag at top level
            }
        }
        c
    };
    cmd.env("CCSM_SESSION", name);
    if let Some(ref wt) = worktree_dir {
        cmd.env("CCSM_WORKTREE", wt);
    }

    let mut child_guard = ChildGuard::new(cmd.spawn()?);
    let child_pid = child_guard.id();

    // ── Phase 4: Write pid to registry (locked) ─────────────────────
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked()?;
        match reg.sessions.iter_mut().rev().find(|e| e.name == name) {
            Some(entry) => entry.pids = vec![child_pid],
            None => anyhow::bail!(
                "internal error: session '{}' vanished from registry between Phase 1 and Phase 4",
                name
            ),
        }
        reg.updated = crate::registry::now_iso();
        reg.save()?;
    }

    // ── Phase 5: Harvest session_id (Claude eager, OpenCode deferred) ──
    let harvest_before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    if consumer.is_claude() {
        // Claude creates a PID-based session file eagerly — poll for it now.
        let session_file = consumer
            .live_session_file(home, child_pid)
            .ok_or_else(|| anyhow::anyhow!("consumer does not support PID-based session files"))?;
        let mut found = false;
        let mut spinner = crate::style::Spinner::new("waiting for agent session file...");
        for _ in 0..50 {
            if session_file.exists() {
                found = true;
                break;
            }
            spinner.advance();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        spinner.done();
        if !found {
            anyhow::bail!(
                "{bin} did not write a session file at {} within 5s.\n\
                 {bin} may have failed to start. Check for errors above.",
                session_file.display(),
            );
        }

        // Override session file's cwd to worktree if one is active.
        // Claude's --resume restores CWD from this file — we want the
        // agent to land in the worktree, not the workspace root.
        if let Some(ref wt) = worktree_dir
            && let Ok(contents) = std::fs::read_to_string(&session_file)
            && let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&contents)
            && let Some(obj) = json.as_object_mut()
        {
            let wt_str = wt.to_string_lossy().to_string();
            if obj.get("cwd").is_none_or(|v| v.as_str() != Some(&wt_str)) {
                obj.insert("cwd".into(), serde_json::Value::String(wt_str));
                if let Ok(updated) = serde_json::to_string(&json) {
                    let _ = std::fs::write(&session_file, &updated);
                }
            }
        }

        // Harvest session_id + started (locked)
        {
            let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked()?;
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
                            session_file.display(),
                            e
                        );
                    }
                },
                Err(e) => {
                    eprintln!(
                        "warning: failed to read session file {}: {}. \
                         Session tracking may be incomplete.",
                        session_file.display(),
                        e
                    );
                }
            }
            reg.save()?;
        }

        // Sync detail file status line with harvested started time
        crate::registry::sync_status_line(name);
    } else if consumer.is_pi() {
        eprintln!("  (session tracking will populate on next `ccsm attach` call)");
    }
    // OpenCode: session is created lazily (on first user message), so harvesting
    // happens after the child exits (see Phase 6b).

    // ── Phase 6: Wait for child ─────────────────────────────────────
    let status = child_guard.wait()?;

    // ── Phase 6b: OpenCode harvest (deferred — session exists after user interaction) ──
    if consumer.is_opencode() {
        let db_path = crate::consumer::opencode_db_path(home);
        let run_dir = worktree_dir
            .as_ref()
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_else(|| workspace.to_string_lossy().to_string());
        let new_sid =
            crate::consumer::opencode_find_session_since(&db_path, &run_dir, harvest_before)
                .or_else(|| {
                    if run_dir != workspace.to_string_lossy().as_ref() {
                        crate::consumer::opencode_find_session_since(
                            &db_path,
                            &workspace.to_string_lossy(),
                            harvest_before,
                        )
                    } else {
                        None
                    }
                });

        if let Some(harvested_id) = new_sid {
            let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked()?;
            let entry = match reg.sessions.iter_mut().rev().find(|e| e.name == name) {
                Some(e) => e,
                None => anyhow::bail!(
                    "internal error: session '{}' vanished from registry between Phase 1 and Phase 6b",
                    name
                ),
            };
            if entry.session_id.is_empty() {
                entry.session_id.clone_from(&harvested_id);
            }
            if entry.started.is_empty() {
                entry.started = crate::registry::now_iso();
            }
            reg.updated = crate::registry::now_iso();
            reg.save()?;
            crate::registry::sync_status_line(name);
            if let Err(e) = crate::consumer::opencode_update_title(&db_path, &harvested_id, name) {
                eprintln!("  warning: failed to rename opencode session: {e}");
            }
            let src = if sid.is_none() {
                "fresh"
            } else {
                "fresh (resume ignored)"
            };
            eprintln!("  opencode  {src} session tracked");
        } else if let Some(ref existing_sid) = sid {
            let current_title = crate::consumer::opencode_get_title(&db_path, existing_sid);
            if current_title.as_deref() != Some(name) {
                if let Err(e) = crate::consumer::opencode_update_title(&db_path, existing_sid, name)
                {
                    eprintln!("  warning: failed to sync opencode title: {e}");
                } else {
                    eprintln!("  opencode DB  synced title → {name}");
                }
            }
            eprintln!("  (session resumed)");
        } else {
            eprintln!("  opencode  exited without creating a session");
        }
    }

    // ── Phase 7: Clear stale pids (locked) ──────────────────────────
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked()?;
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
        reg.save()?;
    }

    if !status.success() {
        anyhow::bail!("{bin} exited with {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_stores_child_and_id_returns_pid() {
        let child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        let guard = ChildGuard::new(child);
        assert_eq!(guard.id(), pid);
    }

    #[test]
    fn test_wait_returns_exit_status_and_disarms() {
        let child = std::process::Command::new("true").spawn().unwrap();
        let mut guard = ChildGuard::new(child);
        let status = guard.wait().unwrap();
        assert!(status.success());
        assert!(guard.child.is_none(), "guard should be disarmed after wait");
    }

    #[test]
    fn test_drop_kills_child_if_not_waited() {
        let child = std::process::Command::new("sleep").arg("10").spawn().unwrap();
        let pid = child.id();
        {
            let _guard = ChildGuard::new(child);
        }
        let rc = unsafe { libc::kill(pid as i32, 0) };
        assert_eq!(rc, -1, "child should be dead after guard drops");
    }
}
