//! End-to-end tests for OpenCode V2 workflow.
//!
//! Tests the combined plugin flow:
//!   1. `ccsm attach <name> <session_id>` — links OpenCode session to ccsm registry
//!   2. `ccsm inject-scope <name>` — resolves session from registry by name
//!   3. `ccsm inject-scope` from outside workspace (simulating server process) — documents
//!      why the plugin must pass `cwd` to `execFileSync`
//!
//! These tests verify the exact flow the V2 plugin (`plugins/opencode/ccsm-plugin.ts`)
//! uses: session.created → attach → context hook → inject-scope.

mod common;
use common::*;
use std::process::Command;

#[test]
fn inject_scope_outside_workspace_fails_no_identity() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "outside-test", "-g", "test goal"]);
    ws.run_ok(&["start", "outside-test"]);
    ws.run_ok(&["attach", "outside-test", "ses_v2test123456"]);

    // Run `inject-scope outside-test` from a temp dir outside the workspace
    let outside = tempfile::tempdir().expect("outside tempdir");
    let out = Command::new(ccsm_binary())
        .args(["inject-scope", "outside-test"])
        .current_dir(outside.path())
        .env_remove("CCSM_SESSION")
        .env_remove("CCSM_WORKTREE")
        .env_remove("CCSM_WORKSPACE")
        .env("HOME", ws.home())
        .output()
        .expect("inject-scope outside workspace");

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    // Should fail — no .ccsm identity in the outside directory
    assert!(!out.status.success(),
        "inject-scope from outside workspace should fail, stdout: {stdout}, stderr: {stderr}");
    assert!(stderr.contains(".ccsm identity"),
        "error should mention missing .ccsm identity: {stderr}");
}

#[test]
fn inject_scope_from_workspace_works_with_name() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "v2-test", "-g", "V2 workflow test goal"]);
    ws.run_ok(&["start", "v2-test"]);

    // Simulate plugin: attach a V2-style session ID
    ws.run_ok(&["attach", "v2-test", "ses_v2abcdef123456"]);

    // Verify registry has the session_id
    let reg = ws.read_registry();
    let sessions = reg["sessions"].as_array().unwrap();
    let entry = sessions.iter().find(|s| s["name"] == "v2-test").unwrap();
    assert_eq!(entry["session_id"], "ses_v2abcdef123456",
        "session should have V2 session_id after attach");
    assert_eq!(entry["status"], "in_progress");

    // Simulate plugin: inject-scope by name from workspace CWD
    let out = Command::new(ccsm_binary())
        .args(["inject-scope", "v2-test"])
        .current_dir(ws.path())
        .env_remove("CCSM_SESSION")
        .env_remove("CCSM_WORKTREE")
        .env_remove("CCSM_WORKSPACE")
        .env("HOME", ws.home())
        .output()
        .expect("inject-scope with name");

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert!(out.status.success(),
        "inject-scope with --name should succeed: {stderr}");
    assert!(stdout.contains("<system-reminder>"),
        "output should contain system-reminder tags: {stdout}");
    assert!(stdout.contains("ACTIVE SESSION: v2-test"),
        "output should contain session name: {stdout}");
    assert!(stdout.contains("GOAL: V2 workflow test goal"),
        "output should contain goal: {stdout}");
}

#[test]
fn inject_scope_after_attach_caches_session_id() {
    ensure_built();
    let ws = TempWorkspace::new();

    // Create two sessions, attach different IDs to each
    ws.run_ok(&["new", "session-a", "-g", "goal A"]);
    ws.run_ok(&["new", "session-b", "-g", "goal B", "--force"]);

    ws.run_ok(&["start", "session-a"]);
    ws.run_ok(&["attach", "session-a", "ses_v2aaaa000001"]);

    ws.run_ok(&["start", "session-b"]);
    ws.run_ok(&["attach", "session-b", "ses_v2bbbb000002"]);

    // Inject-scope for session-a should show goal A, not goal B
    let out_a = Command::new(ccsm_binary())
        .args(["inject-scope", "session-a"])
        .current_dir(ws.path())
        .env_remove("CCSM_SESSION")
        .env_remove("CCSM_WORKTREE")
        .env_remove("CCSM_WORKSPACE")
        .env("HOME", ws.home())
        .output()
        .expect("inject-scope session-a");
    let stdout_a = String::from_utf8_lossy(&out_a.stdout).to_string();
    assert!(stdout_a.contains("GOAL: goal A"),
        "session-a should show goal A: {stdout_a}");

    // Verify both session IDs are in registry
    let reg = ws.read_registry();
    let sessions = reg["sessions"].as_array().unwrap();
    let a = sessions.iter().find(|s| s["name"] == "session-a").unwrap();
    let b = sessions.iter().find(|s| s["name"] == "session-b").unwrap();
    assert_eq!(a["session_id"], "ses_v2aaaa000001");
    assert_eq!(b["session_id"], "ses_v2bbbb000002");
}
