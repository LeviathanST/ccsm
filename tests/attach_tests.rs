//! Integration tests: ccsm attach (UUID, --pid, auto-discover, validation).

mod common;
use common::*;

#[test]
fn attach_by_uuid() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "linked-session", "-g", "linked test"]);
    let out = ws.run_ok(&["attach", "linked-session", "f493397b-456a-426d-92e1-4d5f15da0311"]);
    assert!(out.contains("attached"), "attach uuid: {}", out);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "linked-session")
        .unwrap();
    assert_eq!(entry["session_id"], "f493397b-456a-426d-92e1-4d5f15da0311");
}

#[test]
fn attach_rejects_non_uuid() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "bad-attach", "-g", "test"]);
    let err = ws.run_err(&["attach", "bad-attach", "smith-system"]);
    assert!(err.contains("does not look like a session UUID"), "uuid validation: {}", err);
}

#[test]
fn attach_empty_session_id_falls_back_to_autodiscover() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "empty-attach", "-g", "test"]);
    // Empty session_id means "auto-discover" — fails because no live Claude sessions exist
    let err = ws.run_err(&["attach", "empty-attach", ""]);
    assert!(err.contains("no live Claude sessions"), "empty session_id: {}", err);
}

#[test]
fn attach_nonexistent_session() {
    ensure_built();
    let ws = TempWorkspace::new();

    let err = ws.run_err(&["attach", "no-such-session", "f493397b-456a-426d-92e1-4d5f15da0311"]);
    assert!(err.contains("no session named"), "nonexistent: {}", err);
}

#[test]
fn attach_auto_discover_no_sessions() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "orphan-session", "-g", "test"]);
    // No live Claude sessions in temp workspace — auto-discover should fail
    let err = ws.run_err(&["attach", "orphan-session"]);
    assert!(err.contains("no live Claude sessions"), "auto-discover empty: {}", err);
}
