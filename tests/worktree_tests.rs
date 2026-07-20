//! Integration tests: ccsm worktree — --worktree flag on new and start.
//!
//! Coverage:
//! - new --worktree sets use_worktree in registry, show displays worktree line
//! - new -b branch --worktree sets both branch and use_worktree
//! - new without --worktree has use_worktree=false, no worktree line in show
//! - start --worktree without branch fails with "no target branch"
//! - start with worktree transitions session to in_progress
//! - start --worktree fails when worktree path already exists
//! - start without worktree flag on a plain session (default behavior)

mod common;
use common::*;

#[test]
fn new_with_worktree_flag() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "my-session", "-g", "test goal", "--worktree"]);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "my-session")
        .unwrap();
    assert_eq!(entry["use_worktree"], true, "use_worktree should be true");
    assert_eq!(entry["goal"], "test goal");

    // show should display worktree line
    let out = ws.run_ok(&["show", "my-session"]);
    assert!(
        out.contains("worktree:"),
        "show should have worktree:\n{out}"
    );
}

#[test]
fn new_with_branch_and_worktree() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&[
        "new",
        "branch-session",
        "-b",
        "main",
        "--worktree",
        "-g",
        "branch test",
    ]);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "branch-session")
        .unwrap();
    assert_eq!(entry["use_worktree"], true);
    assert_eq!(entry["branch"], "main");
    assert_eq!(entry["goal"], "branch test");
}

#[test]
fn new_without_worktree() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "plain-session", "-g", "plain"]);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "plain-session")
        .unwrap();
    assert_eq!(
        entry["use_worktree"], false,
        "use_worktree should default to false"
    );

    // show should NOT display worktree line
    let out = ws.run_ok(&["show", "plain-session"]);
    assert!(
        !out.contains("worktree:"),
        "show should not have worktree:\n{out}"
    );
}

#[test]
fn start_worktree_requires_branch() {
    ensure_built();
    let ws = TempWorkspace::new();

    // Session has --worktree but no -b branch
    ws.run_ok(&["new", "no-branch-wt", "-g", "test", "--worktree"]);
    let err = ws.run_err(&["start", "--worktree", "no-branch-wt"]);
    assert!(
        err.contains("no target branch"),
        "should require branch: {err}"
    );

    // Verify session is still pending (not started)
    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "no-branch-wt")
        .unwrap();
    assert_eq!(entry["status"], "pending");
}

#[test]
fn start_with_worktree() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&[
        "new",
        "wt-session",
        "-b",
        "main",
        "--worktree",
        "-g",
        "worktree test",
    ]);
    let out = ws.run_ok(&["start", "wt-session"]);
    assert!(out.contains("start"), "start should succeed: {out}");

    // Verify status transitioned to in_progress
    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "wt-session")
        .unwrap();
    assert_eq!(entry["status"], "in_progress");
    assert_eq!(entry["use_worktree"], true);
    assert_eq!(entry["branch"], "main");
}

#[test]
fn start_worktree_rejects_duplicate_path() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "dup-wt", "-b", "main", "--worktree", "-g", "dup"]);

    // Manually create the worktree directory to simulate existing worktree
    let wt_path = ws.path().join(".claude").join("worktrees").join("dup-wt");
    std::fs::create_dir_all(&wt_path).expect("create fake worktree path");

    let err = ws.run_err(&["start", "dup-wt"]);
    assert!(
        err.contains("already exists"),
        "should reject duplicate worktree: {err}"
    );
}

#[test]
fn start_without_worktree_flag_is_default() {
    ensure_built();
    let ws = TempWorkspace::new();

    // Session without --worktree, without branch
    ws.run_ok(&["new", "no-wt", "-g", "no worktree"]);
    let out = ws.run_ok(&["start", "no-wt"]);
    assert!(out.contains("start"), "start should succeed: {out}");

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "no-wt")
        .unwrap();
    assert_eq!(entry["status"], "in_progress");
    assert_eq!(entry["use_worktree"], false);
}
