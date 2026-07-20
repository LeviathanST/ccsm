use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ErrorCode;
use anyhow::{Context, Result};

// ── Path derivation ────────────────────────────────────────────────────

/// Canonical worktree path for a session: `<workspace>/.claude/worktrees/<name>/`
pub fn worktree_path_for(workspace: &Path, name: &str) -> PathBuf {
    workspace.join(".claude").join("worktrees").join(name)
}

// ── Git helpers ───────────────────────────────────────────────────────

/// Check whether `workspace` is inside a git repository.
pub fn is_git_repo(workspace: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(workspace)
        .output()
        .ok()
        .is_some_and(|o| o.status.success())
}

/// Check whether `branch` exists locally or as `origin/<branch>`.
/// Uses `git ls-remote` for remote check (fast, no object transfer).
pub fn branch_exists(workspace: &Path, branch: &str) -> bool {
    // Check local branches first (fast)
    if Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(workspace)
        .status()
        .ok()
        .is_some_and(|s| s.success())
    {
        return true;
    }
    // Check remote via ls-remote (fast — just queries refs, no fetch)
    Command::new("git")
        .args([
            "ls-remote",
            "--exit-code",
            "origin",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(workspace)
        .status()
        .ok()
        .is_some_and(|s| s.success())
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

    if let Ok(mut file) = std::fs::OpenOptions::new()
        .append(true)
        .open(&gitignore_path)
    {
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
        "{} not a git repository — worktrees require git.\n\
         Use `git init` or `git clone` first.",
        ErrorCode::Invalid
    );

    let wt_path = worktree_path_for(workspace, name);

    anyhow::ensure!(
        !wt_path.exists(),
        "{} worktree for session '{}' already exists at {}\n\
         Use `ccsm resume {}` to continue.",
        ErrorCode::Exists,
        name,
        wt_path.display(),
        name,
    );

    // ── Ensure branch is up-to-date with main ──────────────────────────
    // Fast path: check local refs only. Slow path: fetch + rebase if behind.
    let is_ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", "origin/main", "HEAD"])
        .current_dir(workspace)
        .status()
        .ok()
        .is_some_and(|s| s.success());

    if !is_ancestor {
        // Only fetch if we can reach the remote (fast ls-remote check)
        let remote_reachable = Command::new("git")
            .args(["ls-remote", "--exit-code", "origin", "HEAD"])
            .current_dir(workspace)
            .status()
            .ok()
            .is_some_and(|s| s.success());

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
                    .args([
                        "rev-list",
                        "--count",
                        "--left-right",
                        &format!("{branch}...origin/main"),
                    ])
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
                    eprintln!(
                        "  branch '{branch}' is {behind} commit(s) behind origin/main — rebasing..."
                    );

                    let is_dirty = Command::new("git")
                        .args(["status", "--porcelain"])
                        .current_dir(workspace)
                        .output()
                        .ok()
                        .map(|o| !o.stdout.is_empty())
                        .unwrap_or(false);

                    let stashed = if is_dirty {
                        anyhow::bail!(
                            "{} branch '{}' has uncommitted changes. Commit or stash first, then run `ccsm start {}` again.",
                            ErrorCode::Invalid,
                            branch,
                            name,
                        );
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
                        let _ = Command::new("git")
                            .args(["rebase", "--abort"])
                            .current_dir(workspace)
                            .output();
                        if stashed {
                            let _ = Command::new("git")
                                .args(["stash", "pop"])
                                .current_dir(workspace)
                                .output();
                        }
                        anyhow::bail!(
                            "{} failed to rebase '{}' onto origin/main.\nResolve conflicts manually, then run `ccsm start` again.",
                            ErrorCode::Gate,
                            branch
                        );
                    }
                    eprintln!("  rebase complete — '{branch}' is now up-to-date with main");

                    if stashed {
                        // Create worktree + pop stash inside it
                        if let Some(parent) = wt_path.parent() {
                            std::fs::create_dir_all(parent).with_context(|| {
                                format!("creating worktree parent: {}", parent.display())
                            })?;
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
                            let _ = Command::new("git")
                                .args(["stash", "pop"])
                                .current_dir(workspace)
                                .output();
                            anyhow::bail!(
                                "{} failed to create worktree after rebase.",
                                ErrorCode::Gate
                            );
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
                            eprintln!(
                                "  warning: stash had conflicts in worktree. Resolve manually (git stash list)."
                            );
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
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(workspace)
        .status()
        .ok()
        .is_some_and(|s| s.success());

    let result = if branch_exists {
        Command::new("git")
            .args(["worktree", "add", &wt_path.to_string_lossy(), branch])
            .current_dir(workspace)
            .output()
            .context("failed to run `git worktree add`")?
    } else {
        eprintln!("  creating new branch '{}' from HEAD...", branch);
        Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                branch,
                &wt_path.to_string_lossy(),
                "HEAD",
            ])
            .current_dir(workspace)
            .output()
            .context("failed to run `git worktree add -b`")?
    };

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let _ = std::fs::remove_dir_all(&wt_path);
        anyhow::bail!(
            "{} git worktree add failed for branch '{}' at {}:\n{}",
            ErrorCode::Gate,
            branch,
            wt_path.display(),
            stderr.trim(),
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
            "{} failed to remove worktree for '{}' at {}:\n{}\n\
             Use `ccsm complete {} --force` to force remove.",
            ErrorCode::Gate,
            name,
            wt_path.display(),
            stderr.trim(),
            name,
        );
    }

    // Clean up leftover directory if any
    if wt_path.exists() {
        let _ = std::fs::remove_dir_all(&wt_path);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init_git_repo(dir: &Path) {
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "t"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    fn init_repo_with_commit(dir: &Path) {
        init_git_repo(dir);
        std::fs::write(dir.join("readme"), "hi").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn worktree_path_for_basic() {
        let p = worktree_path_for(Path::new("/ws"), "my-session");
        assert_eq!(p, PathBuf::from("/ws/.claude/worktrees/my-session"));
    }

    #[test]
    fn is_git_repo_true_when_in_repo() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());
        assert!(is_git_repo(dir.path()));
    }

    #[test]
    fn is_git_repo_false_when_not_in_repo() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_git_repo(dir.path()));
    }

    #[test]
    fn branch_exists_local_main() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        // main should be found via local refs
        assert!(branch_exists(dir.path(), "main"));
    }

    #[test]
    fn branch_exists_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        // The remote check will fail too (no origin), so this should be false
        assert!(!branch_exists(dir.path(), "nope"));
    }

    #[test]
    fn ensure_worktree_gitignore_creates() {
        let dir = tempfile::tempdir().unwrap();
        ensure_worktree_gitignore(dir.path());
        let c = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(c.contains("/.claude/worktrees/"));
    }

    #[test]
    fn ensure_worktree_gitignore_appends() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "existing\n").unwrap();
        ensure_worktree_gitignore(dir.path());
        let c = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(c.contains("existing"));
        assert!(c.contains("/.claude/worktrees/"));
    }

    #[test]
    fn create_worktree_fails_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let e = create_worktree(dir.path(), "s", "b")
            .unwrap_err()
            .to_string();
        assert!(e.contains("git repository"), "{e}");
    }

    #[test]
    fn remove_worktree_noop_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        assert!(remove_worktree(dir.path(), "phantom", false).is_ok());
    }

    // ── Branch exists (remote path) ────────────────────────────────────

    #[test]
    fn branch_exists_remote_only() {
        let bare = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(bare.path())
            .output()
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());

        // Configure repo and push main to bare
        Command::new("git")
            .args(["remote", "add", "origin", bare.path().to_str().unwrap()])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "origin", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Push a branch that only exists on remote
        Command::new("git")
            .args(["push", "origin", "main:refs/heads/remote-only"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Verify it's NOT found locally
        assert!(
            !Command::new("git")
                .args(["show-ref", "--verify", "--quiet", "refs/heads/remote-only"])
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );

        // But IS found via ls-remote
        assert!(branch_exists(dir.path(), "remote-only"));
    }

    // ── Ensure gitignore (no-op) ───────────────────────────────────────

    #[test]
    fn ensure_worktree_gitignore_noop_when_pattern_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "/.claude/worktrees/\n").unwrap();
        let before = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        ensure_worktree_gitignore(dir.path());
        let after = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn ensure_worktree_gitignore_noop_variants() {
        for pattern in ["/.claude/worktrees", ".claude/worktrees/"] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(".gitignore"), format!("{pattern}\n")).unwrap();
            let before = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
            ensure_worktree_gitignore(dir.path());
            let after = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
            assert_eq!(before, after, "pattern={pattern}");
        }
    }

    // ── Create worktree ────────────────────────────────────────────────

    #[test]
    fn create_worktree_fails_when_path_exists() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        let wt_path = dir.path().join(".claude").join("worktrees").join("exists");
        std::fs::create_dir_all(&wt_path).unwrap();
        let e = create_worktree(dir.path(), "exists", "main")
            .unwrap_err()
            .to_string();
        assert!(e.contains("already exists"), "{e}");
    }

    #[test]
    fn create_worktree_success_existing_branch() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        // Create a second branch so we can check it out in a worktree
        // (git won't check out 'main' in a second worktree since it's active)
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let result = create_worktree(dir.path(), "test-session", "feature");
        assert!(
            result.is_ok(),
            "create_worktree failed: {:?}",
            result.as_ref().unwrap_err()
        );
        let path = result.unwrap();
        assert!(path.exists(), "worktree directory should exist");
        assert!(
            path.join("readme").exists(),
            "worktree should contain repo files"
        );
        // Cleanup
        remove_worktree(dir.path(), "test-session", true).unwrap();
    }

    #[test]
    fn create_worktree_success_new_branch() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        let result = create_worktree(dir.path(), "new-branch-session", "new-feature");
        assert!(
            result.is_ok(),
            "create_worktree with new branch failed: {:?}",
            result.as_ref().unwrap_err()
        );
        let path = result.unwrap();
        assert!(path.exists());
        // Verify the branch was created locally
        assert!(
            Command::new("git")
                .args(["show-ref", "--verify", "--quiet", "refs/heads/new-feature"])
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
        remove_worktree(dir.path(), "new-branch-session", true).unwrap();
    }

    // ── Remove worktree ────────────────────────────────────────────────

    #[test]
    fn remove_worktree_removes_existing() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let wt = create_worktree(dir.path(), "remove-me", "feature").unwrap();
        assert!(wt.exists());
        remove_worktree(dir.path(), "remove-me", false).unwrap();
        assert!(!wt.exists());
    }

    #[test]
    fn remove_worktree_force_flag() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let wt = create_worktree(dir.path(), "force-rm", "feature").unwrap();
        assert!(wt.exists());
        remove_worktree(dir.path(), "force-rm", true).unwrap();
        assert!(!wt.exists());
    }

    #[test]
    fn remove_worktree_fails_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        // Create the worktree path manually in a non-git directory
        let wt_path = dir.path().join(".claude").join("worktrees").join("phantom");
        std::fs::create_dir_all(&wt_path).unwrap();
        let e = remove_worktree(dir.path(), "phantom", false)
            .unwrap_err()
            .to_string();
        assert!(e.contains("failed to remove worktree"), "{e}");
    }

    // ── Worktree add failure path ─────────────────────────────────────

    #[test]
    fn create_worktree_fails_invalid_branch_name() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        // Branch name with space is rejected by git
        let e = create_worktree(dir.path(), "bad-branch", "bad name")
            .unwrap_err()
            .to_string();
        assert!(e.contains("git worktree add failed"), "{e}");
    }

    // ── Remote rebase path helpers ────────────────────────────────────

    /// Set up a scenario where `dir`'s `main` branch is 1 commit behind
    /// `origin/main`. Also creates a local `feature` branch at the same
    /// commit as `main`. Returns the working dir and the bare repo.
    fn setup_behind_remote() -> (tempfile::TempDir, tempfile::TempDir) {
        let bare = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init", "--bare", "-b", "main"])
            .current_dir(bare.path())
            .output()
            .unwrap();

        // Repo A: init with commit
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        Command::new("git")
            .args(["remote", "add", "origin", bare.path().to_str().unwrap()])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create feature branch locally and push it
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", "feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Repo B: advance main on remote
        let dir_b = tempfile::tempdir().unwrap();
        Command::new("git")
            .args([
                "clone",
                bare.path().to_str().unwrap(),
                dir_b.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        for cfg in [
            ["config", "user.email", "t@t"],
            ["config", "user.name", "t"],
        ] {
            Command::new("git")
                .args(cfg)
                .current_dir(dir_b.path())
                .output()
                .unwrap();
        }
        std::fs::write(dir_b.path().join("extra"), "x").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir_b.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "extra"])
            .current_dir(dir_b.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "origin", "main"])
            .current_dir(dir_b.path())
            .output()
            .unwrap();

        // Update A's remote tracking ref
        Command::new("git")
            .args(["fetch", "origin", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        (dir, bare)
    }

    #[test]
    fn create_worktree_remote_rebase_success() {
        let (dir, _bare) = setup_behind_remote();
        let result = create_worktree(dir.path(), "remote-succ", "feature");
        assert!(
            result.is_ok(),
            "rebase + success path failed: {:?}",
            result.as_ref().unwrap_err()
        );
        let path = result.unwrap();
        assert!(path.exists());
        remove_worktree(dir.path(), "remote-succ", true).unwrap();
    }

    #[test]
    fn create_worktree_remote_rebase_worktree_add_fails() {
        let (dir, _bare) = setup_behind_remote();
        // "main" is already checked out in the parent repo, so
        // git worktree add fails after the rebase finishes
        let e = create_worktree(dir.path(), "remote-err", "main")
            .unwrap_err()
            .to_string();
        assert!(e.contains("git worktree add failed"), "{e}");
    }
}
