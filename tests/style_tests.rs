mod common;
use common::*;

#[test]
fn config_show_defaults() {
    ensure_built();
    let ws = TempWorkspace::new();
    let out = ws.run_ok(&["config"]);
    assert!(out.contains("wip_limit"), "config should show wip_limit:\n{out}");
    assert!(out.contains("branch_tracking"), "config should show branch_tracking:\n{out}");
}

#[test]
fn config_set_and_show() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["config", "set", "wip_limit", "7"]);
    let out = ws.run_ok(&["config"]);
    assert!(out.contains("wip_limit:"), "config should reflect change:\n{out}");
    assert!(out.contains("7"), "config should contain value 7:\n{out}");
}

#[test]
fn config_reset_restores_defaults() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["config", "set", "wip_limit", "99"]);
    ws.run_ok(&["config", "reset"]);
    let out = ws.run_ok(&["config"]);
    assert!(out.contains("wip_limit: 0"), "should restore default wip_limit=0:\n{out}");
}

#[test]
fn error_code_appears_in_failure() {
    ensure_built();
    let ws = TempWorkspace::new();
    let err = ws.run_err(&["complete", "nonexistent"]);
    assert!(err.contains("[ERR_NOSESSION]"), "should contain error code:\n{err}");
}

#[test]
fn error_code_in_gate_failure() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "test-session", "-g", "test"]);
    ws.run_ok(&["start", "test-session"]);

    // Gate checks should fail — detail file is empty template
    let err = ws.run_err(&["complete", "test-session"]);
    assert!(err.contains("[ERR_GATE]"), "gate failure should contain error code:\n{err}");
}

#[test]
fn emoji_not_in_doctor_when_piped() {
    ensure_built();
    let ws = TempWorkspace::new();
    let out = ws.run_ok(&["doctor"]);
    // When piped (no terminal), emoji should be ASCII fallbacks
    assert!(!out.contains('⚠'), "doctor should not contain emoji when piped:\n{out}");
    assert!(!out.contains('💡'), "doctor should not contain emoji when piped:\n{out}");
}

#[test]
fn list_uses_table_format() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "my-session", "-g", "test goal"]);
    let out = ws.run_ok(&["list"]);
    assert!(out.contains("my-session"), "list should show session:\n{out}");
    assert!(out.contains("pending"), "list should show status:\n{out}");
}
