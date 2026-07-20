mod common;
use common::*;

#[test]
fn config_show_defaults() {
    ensure_built();
    let ws = TempWorkspace::new();
    let out = ws.run_ok(&["config"]);
    assert!(
        out.contains("wip_limit"),
        "config should show wip_limit:\n{out}"
    );
    assert!(
        out.contains("branch_tracking"),
        "config should show branch_tracking:\n{out}"
    );
}

#[test]
fn config_set_and_show() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["config", "set", "wip_limit", "7"]);
    let out = ws.run_ok(&["config"]);
    assert!(
        out.contains("wip_limit:"),
        "config should reflect change:\n{out}"
    );
    assert!(out.contains("7"), "config should contain value 7:\n{out}");
}

#[test]
fn config_reset_restores_defaults() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["config", "set", "wip_limit", "99"]);
    ws.run_ok(&["config", "reset"]);
    let out = ws.run_ok(&["config"]);
    assert!(
        out.contains("wip_limit: 0"),
        "should restore default wip_limit=0:\n{out}"
    );
}

#[test]
fn error_code_appears_in_failure() {
    ensure_built();
    let ws = TempWorkspace::new();
    let err = ws.run_err(&["complete", "nonexistent"]);
    assert!(
        err.contains("[ERR_NOSESSION]"),
        "should contain error code:\n{err}"
    );
}

#[test]
fn error_code_in_gate_failure() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "test-session", "-g", "test"]);
    ws.run_ok(&["start", "test-session"]);

    // Gate checks should fail — detail file is empty template
    let err = ws.run_err(&["complete", "test-session"]);
    assert!(
        err.contains("[ERR_GATE]"),
        "gate failure should contain error code:\n{err}"
    );
}

#[test]
fn emoji_not_in_doctor_when_piped() {
    ensure_built();
    let ws = TempWorkspace::new();
    let out = ws.run_ok(&["doctor"]);
    // When piped (no terminal), emoji should be ASCII fallbacks
    assert!(
        !out.contains('⚠'),
        "doctor should not contain emoji when piped:\n{out}"
    );
    assert!(
        !out.contains('💡'),
        "doctor should not contain emoji when piped:\n{out}"
    );
}

#[test]
fn list_uses_table_format() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "my-session", "-g", "test goal"]);
    let out = ws.run_ok(&["list"]);
    assert!(
        out.contains("my-session"),
        "list should show session:\n{out}"
    );
    assert!(out.contains("pending"), "list should show status:\n{out}");
}

#[test]
fn doctor_reports_healthy_workspace() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "test-session", "-g", "This is a long enough goal for testing purposes"]);
    ws.run_ok(&["start", "test-session"]);
    ws.run_ok(&["scope", "test-session", "This is a detailed scope description for the session"]);
    ws.run_ok(&["tag", "test-session", "dev", "testing"]);
    ws.run_ok(&["note", "test-session", "Did some initial work"]);
    ws.run_ok(&["note", "test-session", "Completed more work"]);
    ws.run_ok(&["complete", "test-session"]);
    // First run auto-creates template, sessions dir, session-group dir
    ws.run_ok(&["doctor"]);
    // Second run should be fully healthy
    let out = ws.run_ok(&["doctor"]);
    assert!(
        out.contains("[*] 1 healthy session"),
        "expected healthy count:\n{out}"
    );
    assert!(
        !out.contains("[!]"),
        "should have no warnings:\n{out}"
    );
}

#[test]
fn doctor_warns_missing_detail_file() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "missing-detail", "-g", "Test goal"]);
    // Delete the detail file that ccsm new auto-created
    let identity = ws.read_identity();
    let id = identity
        .lines()
        .find_map(|l| l.strip_prefix("id = \"").and_then(|s| s.strip_suffix('"')))
        .expect("parse identity id");
    let detail_path = ws
        .home()
        .join(".ccsm")
        .join(&id)
        .join("sessions")
        .join("missing-detail.md");
    let _ = std::fs::remove_file(&detail_path);
    let out = ws.run_ok(&["doctor"]);
    assert!(
        out.contains("no detail file"),
        "doctor should mention missing detail file:\n{out}"
    );
    assert!(
        out.contains("missing-detail"),
        "should reference the session name:\n{out}"
    );
}

#[test]
fn doctor_with_corrupt_registry() {
    ensure_built();
    let ws = TempWorkspace::new();
    let identity = ws.read_identity();
    let id = identity
        .lines()
        .find_map(|l| l.strip_prefix("id = \"").and_then(|s| s.strip_suffix('"')))
        .expect("parse identity id");
    let reg_path = ws
        .home()
        .join(".ccsm")
        .join(&id)
        .join("sessions.json");
    std::fs::write(&reg_path, b"not valid json {{{").unwrap();
    let out = ws.run(&["doctor"]);
    assert!(
        out.status.success(),
        "doctor should not crash on corrupt registry"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("registry file is corrupt"),
        "doctor should report corrupt registry:\n{stderr}"
    );
}

#[test]
fn doctor_shows_worktree_warnings() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "--worktree", "wt-session", "-g", "Test goal for worktree"]);
    let out = ws.run_ok(&["doctor"]);
    assert!(
        out.contains("stale worktree"),
        "doctor should mention stale worktree:\n{out}"
    );
    assert!(
        out.contains("wt-session"),
        "should reference the session name:\n{out}"
    );
}
