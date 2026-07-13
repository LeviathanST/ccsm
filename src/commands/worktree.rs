use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

// ── Path derivation ────────────────────────────────────────────────────

/// Canonical worktree path for a session: `<workspace>/.claude/worktrees/<name>/`
pub fn worktree_path_for(workspace: &Path, name: &str) -> PathBuf {
    workspace
        .join(".claude")
        .join("worktrees")
        .join(name)
}

// ── Git helpers ───────────────────────────────────────────────────────

/// Check whether `workspace` is inside a git repository.
pub fn is_git_repo(workspace: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(workspace)
        .output()
        .ok()
        .map_or(false, |o| o.status.success())
}

/// Check whether `branch` exists locally or as `origin/<branch>`.
/// Uses `git ls-remote` for remote check (fast, no object transfer).
pub fn branch_exists(workspace: &Path, branch: &str) -> bool {
    // Check local branches first (fast)
    if Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")])
        .current_dir(workspace)
        .status()
        .ok()
        .map_or(false, |s| s.success())
    {
        return true;
    }
    // Check remote via ls-remote (fast — just queries refs, no fetch)
    Command::new("git")
        .args(["ls-remote", "--exit-code", "origin", &format!("refs/heads/{branch}")])
        .current_dir(workspace)
        .status()
        .ok()
        .map_or(false, |s| s.success())
}

// ── Gitignore management ──────────────────────────────────────────────

/// Ensure `/.claude/worktrees/` is listed in `.gitignore`.
/// Best-effort — failures are silently ignored.
pub fn ensure_worktree_gitignore(workspace: &Path) {
    let gitignore_path = workspace.join(".gitignore");
    let pattern = "/.claude/worktrees/";

    let contents = match std::fs::read_to_string(&gitignore_path) {
        Ok(c) => c,
        Err(_) => {
            let _ = std::fs::write(&gitignore_path, format!("{pattern}\n"));
            return;
        }
    };

    if contents.lines().any(|l| {
        let t = l.trim();
        t == pattern || t == "/.claude/worktrees" || t == ".claude/worktrees/"
    }) {
        return;
    }

    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&gitignore_path) {
        use std::io::Write;
        let _ = writeln!(file, "{pattern}");
    }
}

// ── Core operations ───────────────────────────────────────────────────

/// Create a git worktree for a session at `.claude/worktrees/<name>/`
/// on the given `branch`.
///
/// Returns the worktree path on success.
pub fn create_worktree(workspace: &Path, name: &str, branch: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        is_git_repo(workspace),
        "not a git repository — worktrees require git.\n\
         Use `git init` or `git clone` first."
    );

    let wt_path = worktree_path_for(workspace, name);

    anyhow::ensure!(
        !wt_path.exists(),
        "worktree for session '{}' already exists at {}\n\
         Use `ccsm resume {}` to continue.",
        name, wt_path.display(), name,
    );

    // ── Ensure branch is up-to-date with main ──────────────────────────
    // Fast path: check local refs only. Slow path: fetch + rebase if behind.
    let is_ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", "origin/main", "HEAD"])
        .current_dir(workspace)
        .status()
        .ok()
        .map_or(false, |s| s.success());

    if !is_ancestor {
        // Only fetch if we can reach the remote (fast ls-remote check)
        let remote_reachable = Command::new("git")
            .args(["ls-remote", "--exit-code", "origin", "HEAD"])
            .current_dir(workspace)
            .status()
            .ok()
            .map_or(false, |s| s.success());

        if remote_reachable {
            let fetch_ok = Command::new("git")
                .args(["fetch", "origin", "main"])
                .current_dir(workspace)
                .output()
                .ok()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if fetch_ok {
                // Re-check behind status after fetch
                let behind = Command::new("git")
                    .args(["rev-list", "--count", "--left-right", &format!("{branch}...origin/main")])
                    .current_dir(workspace)
                    .output()
                    .ok()
                    .and_then(|o| {
                        let s = String::from_utf8_lossy(&o.stdout);
                        let parts: Vec<&str> = s.trim().split('\t').collect();
                        parts.get(1).and_then(|v| v.trim().parse::<i64>().ok())
                    })
                    .unwrap_or(0);

                if behind > 0 {
                    eprintln!("  branch '{branch}' is {behind} commit(s) behind origin/main — rebasing...");

                    let is_dirty = Command::new("git")
                        .args(["status", "--porcelain"])
                        .current_dir(workspace)
                        .output()
                        .ok()
                        .map(|o| !o.stdout.is_empty())
                        .unwrap_or(false);

                    let stashed = if is_dirty {
                        eprint!("  branch has uncommitted changes. Move them to the worktree? [y/N] ");
                        use std::io::{self, Write};
                        let _ = io::stdout().flush();
                        let mut input = String::new();
                        let ok = io::stdin().read_line(&mut input)
                            .map(|_| matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
                            .unwrap_or(false);
                        if !ok {
                            anyhow::bail!("aborted. Commit or stash changes first, then run `ccsm start {}` again.", name);
                        }
                        let stash_result = Command::new("git")
                            .args(["stash", "push", "-m", "ccsm-auto-stash"])
                            .current_dir(workspace)
                            .output()
                            .context("failed to run `git stash`")?;
                        if !stash_result.status.success() {
                            anyhow::bail!("failed to stash changes:\n{}", String::from_utf8_lossy(&stash_result.stderr).trim());
                        }
                        eprintln!("  changes stashed — rebasing...");
                        true
                    } else {
                        false
                    };

                    let rebase_ok = Command::new("git")
                        .args(["rebase", "origin/main"])
                        .current_dir(workspace)
                        .output()
                        .ok()
                        .map(|o| o.status.success())
                        .unwrap_or(false);

                    if !rebase_ok {
                        let _ = Command::new("git").args(["rebase", "--abort"]).current_dir(workspace).output();
                        if stashed {
                            let _ = Command::new("git").args(["stash", "pop"]).current_dir(workspace).output();
                        }
                        anyhow::bail!("failed to rebase '{}' onto origin/main.\nResolve conflicts manually, then run `ccsm start` again.", branch);
                    }
                    eprintln!("  rebase complete — '{branch}' is now up-to-date with main");

                    if stashed {
                        // Create worktree + pop stash inside it
                        if let Some(parent) = wt_path.parent() {
                            std::fs::create_dir_all(parent)
                                .with_context(|| format!("creating worktree parent: {}", parent.display()))?;
                        }
                        let add_ok = Command::new("git")
                            .args(["worktree", "add", &wt_path.to_string_lossy(), branch])
                            .current_dir(workspace)
                            .output()
                            .ok()
                            .map(|o| o.status.success())
                            .unwrap_or(false);
                        if !add_ok {
                            let _ = std::fs::remove_dir_all(&wt_path);
                            let _ = Command::new("git").args(["stash", "pop"]).current_dir(workspace).output();
                            anyhow::bail!("failed to create worktree after rebase.");
                        }
                        eprintln!("  restoring stashed changes into worktree...");
                        let pop_ok = Command::new("git")
                            .args(["stash", "pop"])
                            .current_dir(&wt_path)
                            .output()
                            .ok()
                            .map(|o| o.status.success())
                            .unwrap_or(false);
                        if !pop_ok {
                            eprintln!("  warning: stash had conflicts in worktree. Resolve manually (git stash list).");
                        } else {
                            eprintln!("  changes moved to worktree successfully");
                        }
                        ensure_worktree_gitignore(workspace);
                        return Ok(wt_path);
                    }
                }
            } else {
                eprintln!("  warning: fetch failed — worktree may be stale");
            }
        }
        // If remote unreachable, just proceed without fetching
    }

    // Create parent directory
    if let Some(parent) = wt_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating worktree parent: {}", parent.display()))?;
    }

    // Create worktree — auto-create branch from HEAD if it doesn't exist
    let branch_exists = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")])
        .current_dir(workspace)
        .status()
        .ok()
        .map_or(false, |s| s.success());

    let result = if branch_exists {
        Command::new("git")
            .args(["worktree", "add", &wt_path.to_string_lossy(), branch])
            .current_dir(workspace)
            .output()
            .context("failed to run `git worktree add`")?
    } else {
        eprintln!("  creating new branch '{}' from HEAD...", branch);
        Command::new("git")
            .args(["worktree", "add", "-b", branch, &wt_path.to_string_lossy(), "HEAD"])
            .current_dir(workspace)
            .output()
            .context("failed to run `git worktree add -b`")?
    };


    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let _ = std::fs::remove_dir_all(&wt_path);
        anyhow::bail!(
            "git worktree add failed for branch '{}' at {}:\n{}",
            branch, wt_path.display(), stderr.trim(),
        );
    }

    // Ensure worktree is gitignored
    ensure_worktree_gitignore(workspace);

    Ok(wt_path)
}

/// Remove a git worktree for a session. Idempotent — succeeds if the
/// worktree doesn't exist.
///
/// When `force` is true, uses `git worktree remove --force` to handle
/// dirty worktrees.
pub fn remove_worktree(workspace: &Path, name: &str, force: bool) -> Result<()> {
    let wt_path = worktree_path_for(workspace, name);

    if !wt_path.exists() {
        return Ok(()); // Idempotent
    }

    let wt_str = wt_path.to_string_lossy().to_string();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&wt_str);

    let result = Command::new("git")
        .args(&args)
        .current_dir(workspace)
        .output()
        .with_context(|| format!("failed to run `git worktree remove` for '{}'", name))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        anyhow::bail!(
            "failed to remove worktree for '{}' at {}:\n{}\n\
             Use `ccsm complete {} --force` to force remove.",
            name, wt_path.display(), stderr.trim(), name,
        );
    }

    // Clean up leftover directory if any
    if wt_path.exists() {
        let _ = std::fs::remove_dir_all(&wt_path);
    }

    Ok(())
}
