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
//! - complete removes worktree best-effort, session transitions to completed
//! - pending resets worktree session, preserves use_worktree flag
//! - start --worktree flag sets use_worktree on existing branch session
//! - worktree policy = required auto-creates worktree from config
//! - worktree policy = disabled skips worktree even with --worktree flag
//! - show --json includes worktree path when use_worktree is set
//! - abandon preserves worktree directory (no cleanup)
//! - clean removes session and attempts worktree cleanup

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

// ── Helpers ─────────────────────────────────────────────────────────────

/// Write a `config.toml` into the test workspace's global data directory.
fn set_config(ws: &TempWorkspace, content: &str) {
    let identity = ws.read_identity();
    let id = identity
        .lines()
        .find_map(|l| l.strip_prefix("id = \"").and_then(|s| s.strip_suffix('"')))
        .expect("parse identity id");
    let config_dir = ws.home().join(".ccsm").join(id);
    std::fs::create_dir_all(&config_dir).ok();
    std::fs::write(config_dir.join("config.toml"), content).expect("write config.toml");
}

// ── Worktree completion / cleanup ───────────────────────────────────────

#[test]
fn complete_worktree_session() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&[
        "new",
        "wt-complete",
        "-b",
        "main",
        "--worktree",
        "-g",
        "complete test",
    ]);
    ws.run_ok(&["start", "wt-complete"]);

    // Gate checks require Scope, Tags, and ≥2 progress log entries.
    // Overwrite the detail file so non-force completion passes.
    ws.write_detail(
        "wt-complete",
        "# Session: wt-complete\n\
         \n\
         ## Goal\n\
         \n\
         complete test\n\
         \n\
         ## Scope / Plan\n\
         \n\
         Test scope\n\
         \n\
         ## Tags\n\
         \n\
         test\n\
         \n\
         ## Progress Log\n\
         \n\
         - [2025-01-01 12:00] first entry\n\
         - [2025-01-01 13:00] second entry\n",
    );

    let out = ws.run_ok(&["complete", "wt-complete"]);
    assert!(out.contains("completed"), "should complete: {out}");

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "wt-complete")
        .unwrap();
    assert_eq!(entry["status"], "completed");
    assert_eq!(
        entry["use_worktree"], true,
        "use_worktree should persist after complete"
    );
}

#[test]
fn complete_worktree_session_force() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&[
        "new",
        "wt-force",
        "-b",
        "main",
        "--worktree",
        "-g",
        "force test",
    ]);
    ws.run_ok(&["start", "wt-force"]);

    let out = ws.run_ok(&["complete", "wt-force", "--force"]);
    assert!(out.contains("completed"), "should complete: {out}");

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "wt-force")
        .unwrap();
    assert_eq!(entry["status"], "completed");
}

// ── Worktree pending reset ──────────────────────────────────────────────

#[test]
fn pending_resets_worktree_session() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&[
        "new",
        "wt-pending",
        "-b",
        "main",
        "--worktree",
        "-g",
        "pending test",
    ]);
    ws.run_ok(&["start", "wt-pending"]);
    ws.run_ok(&["pending", "wt-pending"]);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "wt-pending")
        .unwrap();
    assert_eq!(entry["status"], "pending");
    assert_eq!(
        entry["use_worktree"], true,
        "pending should preserve use_worktree flag"
    );
}

// ── start --worktree on branch session without --worktree flag ──────────

#[test]
fn start_with_worktree_flag_sets_use_worktree() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "flag-wt", "-b", "main", "-g", "flag test"]);

    // Verify use_worktree was not set at creation
    let reg_before = ws.read_registry();
    let e = reg_before["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "flag-wt")
        .unwrap();
    assert_eq!(e["use_worktree"], false);

    // Start with --worktree flag
    let out = ws.run_ok(&["start", "--worktree", "flag-wt"]);
    assert!(out.contains("start"), "should succeed: {out}");

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "flag-wt")
        .unwrap();
    assert_eq!(
        entry["use_worktree"], true,
        "start --worktree should set use_worktree"
    );
    assert_eq!(entry["status"], "in_progress");
}

// ── Worktree policy: required ───────────────────────────────────────────

#[test]
fn worktree_policy_required_auto_creates() {
    ensure_built();
    let ws = TempWorkspace::new();
    set_config(&ws, r#"worktrees = "required""#);

    ws.run_ok(&["new", "req-wt", "-b", "main", "-g", "required test"]);

    let out = ws.run_ok(&["start", "req-wt"]);
    assert!(out.contains("start"), "should succeed: {out}");

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "req-wt")
        .unwrap();
    assert_eq!(entry["status"], "in_progress");
    assert_eq!(
        entry["use_worktree"], false,
        "required policy doesn't set registry flag (only --worktree CLI flag does)"
    );
}

// ── Worktree policy: disabled ───────────────────────────────────────────

#[test]
fn worktree_policy_disabled_skips_worktree() {
    ensure_built();
    let ws = TempWorkspace::new();
    set_config(&ws, r#"worktrees = "disabled""#);

    ws.run_ok(&[
        "new",
        "disabled-wt",
        "-b",
        "main",
        "--worktree",
        "-g",
        "disabled test",
    ]);
    let out = ws.run_ok(&["start", "disabled-wt"]);
    assert!(out.contains("start"), "should succeed: {out}");

    // Worktree path should NOT have been created
    let wt_path = ws
        .path()
        .join(".claude")
        .join("worktrees")
        .join("disabled-wt");
    assert!(
        !wt_path.exists(),
        "disabled policy should skip worktree creation"
    );

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "disabled-wt")
        .unwrap();
    assert_eq!(entry["status"], "in_progress");
    assert_eq!(
        entry["use_worktree"], true,
        "registry flag preserved even when disabled"
    );
}

// ── show --json includes worktree path ──────────────────────────────────

#[test]
fn show_json_includes_worktree_field() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&[
        "new",
        "json-wt",
        "-b",
        "main",
        "--worktree",
        "-g",
        "json test",
    ]);
    let out = ws.run_ok(&["show", "--json", "json-wt"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let expected_path = ws
        .path()
        .join(".claude")
        .join("worktrees")
        .join("json-wt")
        .to_string_lossy()
        .to_string();
    assert_eq!(parsed["worktree"], expected_path);

    // Session without --worktree should have empty worktree field
    ws.run_ok(&["new", "json-plain", "-g", "plain"]);
    let out2 = ws.run_ok(&["show", "--json", "json-plain"]);
    let parsed2: serde_json::Value = serde_json::from_str(&out2).expect("valid JSON");
    assert_eq!(
        parsed2["worktree"], "",
        "plain session should have empty worktree"
    );
}

// ── list --json includes branch ─────────────────────────────────────────

#[test]
fn list_json_includes_branch() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&[
        "new",
        "list-branch",
        "-b",
        "feature-x",
        "--worktree",
        "-g",
        "list test",
    ]);
    let out = ws.run_ok(&["list", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let entry = parsed
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "list-branch")
        .expect("session in list");
    assert_eq!(entry["branch"], "feature-x");

    // Session without branch should have empty branch field
    ws.run_ok(&["new", "list-no-branch", "-g", "no branch"]);
    let out2 = ws.run_ok(&["list", "--json"]);
    let parsed2: serde_json::Value = serde_json::from_str(&out2).expect("valid JSON");
    let entry2 = parsed2
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "list-no-branch")
        .expect("session in list");
    assert_eq!(
        entry2["branch"], "",
        "session without branch should have empty branch"
    );
}

// ── abandon preserves worktree path ─────────────────────────────────────

#[test]
fn abandon_preserves_worktree_path() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&[
        "new",
        "wt-abandon",
        "-b",
        "main",
        "--worktree",
        "-g",
        "abandon test",
    ]);
    ws.run_ok(&["start", "wt-abandon"]);

    // Manually create the worktree directory to simulate one existing
    let wt_path = ws
        .path()
        .join(".claude")
        .join("worktrees")
        .join("wt-abandon");
    std::fs::create_dir_all(&wt_path).expect("create worktree path");

    let out = ws.run_ok(&["abandon", "wt-abandon"]);
    assert!(out.contains("abandoned"), "abandon should succeed: {out}");

    // Worktree directory should still exist (abandon doesn't clean it)
    assert!(
        wt_path.exists(),
        "abandon should NOT remove worktree directory"
    );

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "wt-abandon")
        .unwrap();
    assert_eq!(entry["status"], "abandoned");
    assert_eq!(
        entry["use_worktree"], true,
        "use_worktree should persist after abandon"
    );
}

// ── clean removes worktree session best-effort ──────────────────────────

#[test]
fn clean_removes_worktree_session() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&[
        "new",
        "wt-clean",
        "-b",
        "main",
        "--worktree",
        "-g",
        "clean test",
    ]);
    ws.run_ok(&["start", "wt-clean"]);

    let out = ws.run_ok(&["clean", "wt-clean"]);
    assert!(
        out.contains("permanently deleted"),
        "clean should succeed: {out}"
    );

    let reg = ws.read_registry();
    let found = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "wt-clean");
    assert!(!found, "session should be removed from registry");
}
