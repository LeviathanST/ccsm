//! Integration tests: `ccsm inject-scope` — worktree path resolution,
//! branch check, CCSM_WORKTREE env var behavior.
//!
//! Coverage:
//! - Basic output with CCSM_SESSION set
//! - CCSM_WORKTREE env var overrides derived worktree path
//! - WORKTREE BOUNDARY section emitted when worktree is active
//! - "No live session" when CCSM_SESSION is absent
//! - --name flag overrides CCSM_SESSION

mod common;
use common::*;

use std::process::Command;

/// Build the inject-scope command with a clean environment, clearing any
/// inherited env vars that might leak from the parent process (CCSM_SESSION,
/// CCSM_WORKTREE, CCSM_WORKSPACE).
fn inject_scope_cmd(ws: &TempWorkspace, envs: &[(&str, &str)]) -> Command {
    let mut cmd = Command::new(ccsm_binary());
    cmd.current_dir(ws.path())
        .env_remove("CCSM_SESSION")
        .env_remove("CCSM_WORKTREE")
        .env_remove("CCSM_WORKSPACE")
        .env("HOME", ws.home());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd
}

fn run_inject_scope(ws: &TempWorkspace, envs: &[(&str, &str)]) -> (String, String, bool) {
    let mut cmd = inject_scope_cmd(ws, envs);
    cmd.arg("inject-scope");
    let out = cmd.output().expect("inject-scope execution failed");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

fn run_inject_scope_with_name(ws: &TempWorkspace, name: &str, envs: &[(&str, &str)]) -> (String, String, bool) {
    let mut cmd = inject_scope_cmd(ws, envs);
    cmd.args(["inject-scope", name]);
    let out = cmd.output().expect("inject-scope execution failed");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

fn setup_session(ws: &TempWorkspace) {
    ws.run_ok(&["new", "test-session", "-g", "test goal", "-b", "main"]);
    ws.run_ok(&["start", "test-session"]);
}

#[test]
fn inject_scope_basic_output() {
    ensure_built();
    let ws = TempWorkspace::new();
    setup_session(&ws);

    let (stdout, stderr, success) = run_inject_scope(&ws, &[("CCSM_SESSION", "test-session")]);

    assert!(success, "inject-scope should succeed: {stderr}");
    assert!(stdout.contains("ACTIVE SESSION: test-session"), "stdout: {stdout}");
    assert!(stdout.contains("GOAL: test goal"), "stdout: {stdout}");
    assert!(stdout.contains("<system-reminder>"), "stdout: {stdout}");
    assert!(stdout.contains("</system-reminder>"), "stdout: {stdout}");
}

#[test]
fn inject_scope_no_session() {
    ensure_built();
    let ws = TempWorkspace::new();
    setup_session(&ws);

    let (stdout, _stderr, success) = run_inject_scope(&ws, &[]);

    assert!(success, "inject-scope should return Ok even without session");
    assert!(stdout.is_empty() || !stdout.contains("ACTIVE SESSION"),
        "no session output should appear:\n{stdout}");
}

#[test]
fn inject_scope_name_flag() {
    ensure_built();
    let ws = TempWorkspace::new();
    setup_session(&ws);

    let (stdout, stderr, success) = run_inject_scope_with_name(&ws, "test-session", &[]);

    assert!(success, "inject-scope --name should succeed: {stderr}");
    assert!(stdout.contains("ACTIVE SESSION: test-session"), "stdout: {stdout}");
}

#[test]
fn inject_scope_worktree_env_var() {
    ensure_built();
    let ws = TempWorkspace::new();
    setup_session(&ws);

    let wt_path = ws.path().join(".claude").join("worktrees").join("test-session");
    let (stdout, _stderr, success) = run_inject_scope(&ws, &[
        ("CCSM_SESSION", "test-session"),
        ("CCSM_WORKTREE", &wt_path.to_string_lossy()),
    ]);

    assert!(success);
    assert!(stdout.contains("WORKTREE BOUNDARY"), "WORKTREE BOUNDARY section should be present:\n{stdout}");
    assert!(stdout.contains(&*wt_path.to_string_lossy()),
        "WORKTREE path should match CCSM_WORKTREE env var:\n{stdout}");
    assert!(stdout.contains("DON'T:"), "DON'T section should be present:\n{stdout}");
    assert!(stdout.contains("DO:"), "DO section should be present:\n{stdout}");
    assert!(stdout.contains("ASK FIRST"), "ask-constraint should be present:\n{stdout}");
}

#[test]
fn inject_scope_worktree_env_var_overrides_derived() {
    ensure_built();
    let ws = TempWorkspace::new();
    setup_session(&ws);

    // Set CCSM_WORKTREE to a completely different path — inject-scope
    // MUST use the env var, not derive from CWD or session name.
    let override_path = "/tmp/custom-worktree-path";
    let (stdout, _stderr, success) = run_inject_scope(&ws, &[
        ("CCSM_SESSION", "test-session"),
        ("CCSM_WORKTREE", override_path),
    ]);

    assert!(success);
    assert!(stdout.contains(override_path),
        "WORKTREE path should be the CCSM_WORKTREE value, not derived:\n{stdout}");
}

#[test]
fn inject_scope_worktree_line_omitted_when_no_worktree() {
    ensure_built();
    let ws = TempWorkspace::new();
    // Create session without --worktree flag; don't create the dir
    ws.run_ok(&["new", "no-wt-session", "-g", "no worktree"]);
    ws.run_ok(&["start", "no-wt-session"]);

    let (stdout, _stderr, success) = run_inject_scope(&ws, &[("CCSM_SESSION", "no-wt-session")]);

    assert!(success);
    assert!(!stdout.contains("WORKTREE BOUNDARY"),
        "WORKTREE BOUNDARY section should be absent when no worktree exists:\n{stdout}");
}

#[test]
fn inject_scope_branch_check() {
    ensure_built();
    let ws = TempWorkspace::new();
    setup_session(&ws);

    // During setup we set -b main — test reflects default behavior
    let (stdout, _stderr, success) = run_inject_scope(&ws, &[("CCSM_SESSION", "test-session")]);

    assert!(success);
    // In test temp workspace, git is not initialized, so branch check
    // either shows "detached HEAD" or is skipped silently.
    // We just verify inject-scope doesn't crash on uninit git.
    assert!(stdout.contains("ACTIVE SESSION"), "stdout: {stdout}");
}
