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
        eprintln!("ccsm: cleaning up child process (pid {})", pid);

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
        eprintln!("ccsm: child pid {} didn't exit gracefully, sending SIGKILL", pid);
        let _ = child.kill();
        let _ = child.wait();
    }
}

// ── Resume subcommand ───────────────────────────────────────────────────

/// `ccsm resume <name>` — promote entry, exec `claude --resume` or fresh.
pub fn run_resume(name: &str, workspace: &Path, home: &Path) -> anyhow::Result<()> {
    let slug = crate::registry::project_slug(workspace);
    let now = crate::registry::now_iso();

    // ── Phase 1: Promote entry (locked) ────────────────────────────
    let (sid, fresh) = {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;

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
                // Collect session names that are plausible typos:
                // - query is a substring of the session name, OR
                // - edit distance ≤ 2 AND query is at least 4 chars (real word)
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
        .join(".claude")
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

    // ── Phase 2: Spawn claude (no lock) ─────────────────────────────
    let mut cmd = std::process::Command::new("claude");
    cmd.current_dir(workspace);
    cmd.env("CCSM_SESSION", name);
    if let Some(ref id) = sid {
        cmd.arg("--resume").arg(id);
        println!("resuming    {}  ← claude --resume {}", name, &id[..id.len().min(8)]);
    } else if fresh {
        println!("starting    {}  ← claude (fresh)", name);
    } else {
        println!("starting    {}  ← claude (new session)", name);
    }
    cmd.arg("-n").arg(name);

    let mut child_guard = ChildGuard::new(cmd.spawn()?);
    let child_pid = child_guard.id();

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
        reg.updated = crate::registry::now_iso();
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
