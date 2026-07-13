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
pub fn branch_exists(workspace: &Path, branch: &str) -> bool {
    // Check local branches
    if Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")])
        .current_dir(workspace)
        .status()
        .ok()
        .map_or(false, |s| s.success())
    {
        return true;
    }
    // Check remote tracking branches
    Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &format!("refs/remotes/origin/{branch}")])
        .current_dir(workspace)
        .status()
        .ok()
        .map_or(false, |s| s.success())
}

/// Run `git worktree list --porcelain` and return structured data.
/// Each worktree entry is `(path, branch)` where branch may be "detached".
fn list_worktree_entries(workspace: &Path) -> Result<Vec<(PathBuf, String)>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(workspace)
        .output()
        .context("failed to run `git worktree list`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree list failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch = String::new();

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            // Flush previous entry
            if let Some(ref p) = current_path {
                entries.push((p.clone(), std::mem::take(&mut current_branch)));
            }
            current_path = Some(PathBuf::from(path));
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.trim().is_empty() {
            // Blank line = end of entry
            if let Some(ref p) = current_path {
                entries.push((p.clone(), std::mem::take(&mut current_branch)));
                current_path = None;
            }
        }
    }
    // Flush last entry
    if let Some(p) = current_path {
        entries.push((p, current_branch));
    }

    Ok(entries)
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
            // No .gitignore yet — create one
            let _ = std::fs::write(&gitignore_path, format!("{pattern}\n"));
            return;
        }
    };

    // Check if pattern already present
    if contents.lines().any(|l| l.trim() == pattern || l.trim() == "/.claude/worktrees" || l.trim() == ".claude/worktrees/") {
        return;
    }

    // Append pattern
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
         Use `ccsm resume {}` to continue, or `ccsm worktree remove {}` to clean up.",
        name,
        wt_path.display(),
        name,
        name,
    );

    // ── Ensure branch is up-to-date with main ──────────────────────
    // Fetch latest origin/main and rebase the branch if needed so the
    // worktree starts from current state.
    let fetch_result = Command::new("git")
        .args(["fetch", "origin", "main"])
        .current_dir(workspace)
        .output()
        .context("failed to run `git fetch origin main`")?;

    if fetch_result.status.success() {
        // Check if branch is behind origin/main
        let behind_output = Command::new("git")
            .args([
                "rev-list", "--count", "--left-right",
                &format!("{branch}...origin/main"),
            ])
            .current_dir(workspace)
            .output();

        if let Ok(output) = behind_output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = stdout.trim().split('\t').collect();
            // rev-list --left-right --count: <ahead>\t<behind>
            let behind: i64 = parts.get(1).and_then(|s| s.trim().parse().ok()).unwrap_or(0);

            if behind > 0 {
                eprintln!("  branch '{branch}' is {behind} commit(s) behind origin/main — rebasing...");
                let rebase_result = Command::new("git")
                    .args(["rebase", "origin/main"])
                    .current_dir(workspace)
                    .output()
                    .context("failed to run `git rebase origin/main`")?;

                if !rebase_result.status.success() {
                    let stderr = String::from_utf8_lossy(&rebase_result.stderr);
                    // Abort the rebase so we don't leave the tree in a bad state
                    let _ = Command::new("git")
                        .args(["rebase", "--abort"])
                        .current_dir(workspace)
                        .output();
                    anyhow::bail!(
                        "failed to rebase '{branch}' onto origin/main:\n{}\n\
                         Resolve conflicts manually, then run `ccsm start` again.",
                        stderr.trim(),
                    );
                }
                eprintln!("  rebase complete — '{branch}' is now up-to-date with main");
            }
        }
    } else {
        eprintln!("  warning: could not fetch origin/main — worktree may be stale");
    }

    // Create parent directory
    if let Some(parent) = wt_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating worktree parent: {}", parent.display()))?;
    }

    // Ensure branch exists
    if !branch_exists(workspace, branch) {
        // Try fetching from origin
        let fetch_result = Command::new("git")
            .args(["fetch", "origin", branch])
            .current_dir(workspace)
            .output()
            .context("failed to run `git fetch origin`")?;

        if !fetch_result.status.success() {
            let stderr = String::from_utf8_lossy(&fetch_result.stderr);
            anyhow::bail!(
                "branch '{}' does not exist locally or on origin.\n\
                 Create it with: git checkout -b {}\n\
                 Or push it with: git push origin {}\n\
                 Fetch stderr: {}",
                branch, branch, branch, stderr.trim(),
            );
        }

        if !branch_exists(workspace, branch) {
            anyhow::bail!(
                "branch '{}' was fetched from origin but still not found locally.\n\
                 Try: git checkout -b {} origin/{}",
                branch, branch, branch,
            );
        }
    }

    // Create worktree via `git worktree add`
    let result = Command::new("git")
        .args(["worktree", "add", &wt_path.to_string_lossy(), branch])
        .current_dir(workspace)
        .output()
        .context("failed to run `git worktree add`")?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        // Clean up the empty parent dir we created
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
             Use `ccsm worktree remove {} --force` to force remove.",
            name, wt_path.display(), stderr.trim(), name,
        );
    }

    // Clean up leftover directory if any
    if wt_path.exists() {
        let _ = std::fs::remove_dir_all(&wt_path);
    }

    Ok(())
}

// ── Standalone subcommand handlers ────────────────────────────────────

/// `ccsm worktree create <name>` — create a worktree for a session.
pub fn run_create(workspace: &Path, name: &str) -> Result<()> {
    let reg = crate::registry::WorkspaceRegistry::load(workspace)?;
    let session = reg
        .sessions
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow::anyhow!("no session named '{}'", name))?;

    anyhow::ensure!(
        !session.branch.is_empty(),
        "session '{}' has no target branch. Set one with `ccsm new -b <branch>`.",
        name,
    );

    anyhow::ensure!(
        session.use_worktree || crate::config::Config::load(workspace).worktrees == crate::config::WorktreePolicy::Required,
        "session '{}' is not configured for worktree use.\n\
         Enable it with: ccsm new {} --worktree -b <branch>\n\
         Or set worktrees = \"required\" in .ccsm/config.toml",
        name, name,
    );

    let wt_path = create_worktree(workspace, name, &session.branch)?;

    // Store worktree path in registry (locked)
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
        if let Some(entry) = reg.sessions.iter_mut().rev().find(|e| e.name == name) {
            entry.worktree = wt_path.to_string_lossy().to_string();
            reg.updated = crate::registry::now_iso();
            reg.save(workspace)?;
        }
    }

    println!("worktree created: {}", wt_path.display());
    println!("  → ccsm resume {}  to start working in the worktree", name);
    Ok(())
}

/// `ccsm worktree remove <name> [--force]` — remove a worktree for a session.
pub fn run_remove(workspace: &Path, name: &str, force: bool) -> Result<()> {
    remove_worktree(workspace, name, force)?;

    // Clear worktree path in registry (locked)
    {
        let (mut reg, _lock) = crate::registry::WorkspaceRegistry::load_locked(workspace)?;
        if let Some(entry) = reg.sessions.iter_mut().rev().find(|e| e.name == name) {
            entry.worktree.clear();
            reg.updated = crate::registry::now_iso();
            reg.save(workspace)?;
        }
    }

    println!("worktree removed for '{}'", name);
    Ok(())
}

/// `ccsm worktree list` — list all ccsm-managed worktrees.
pub fn run_list(workspace: &Path) -> Result<()> {
    anyhow::ensure!(
        is_git_repo(workspace),
        "not a git repository",
    );

    let entries = list_worktree_entries(workspace)?;

    // Filter to ccsm-managed worktrees (under .claude/worktrees/)
    let ccsm_prefix = workspace.join(".claude").join("worktrees");
    let ccsm_entries: Vec<_> = entries
        .into_iter()
        .filter(|(path, _)| path.starts_with(&ccsm_prefix))
        .collect();

    if ccsm_entries.is_empty() {
        println!("(no ccsm-managed worktrees)");
        return Ok(());
    }

    for (path, branch) in &ccsm_entries {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        println!("{:30}  {}  ({})", name, branch, path.display());
    }

    Ok(())
}
