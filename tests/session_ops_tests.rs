//! Integration tests: session operations — show, scan, tag, scope, group, note.

mod common;
use common::*;

#[test]
fn show_displays_session_fields() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "my-session", "-g", "test goal"]);
    let out = ws.run_ok(&["show", "my-session"]);

    assert!(out.contains("my-session"), "show should contain name:\n{out}");
    assert!(out.contains("test goal"), "show should contain goal:\n{out}");
    assert!(out.contains("pending"), "show should contain status:\n{out}");
}

#[test]
fn show_nonexistent_session() {
    ensure_built();
    let ws = TempWorkspace::new();

    let err = ws.run_err(&["show", "no-such-session"]);
    assert!(err.contains("no session named"), "nonexistent show:\n{err}");
}

#[test]
fn scan_shows_tabular_output() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "scan-test", "-g", "scan goal"]);
    let out = ws.run_ok(&["scan"]);

    assert!(out.contains("scan-test"), "scan should show session:\n{out}");
    assert!(
        out.contains("scan goal") || out.contains("pending"),
        "scan should show fields:\n{out}"
    );
}

#[test]
fn scan_json_output() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "json-test", "-g", "json goal"]);
    let out = ws.run_ok(&["scan", "--json"]);

    assert!(
        out.contains("json-test"),
        "json scan should contain name:\n{out}"
    );
    assert!(
        out.contains("pending"),
        "json scan should contain status:\n{out}"
    );
    assert!(out.contains("goal"), "json scan should have goal field:\n{out}");

    let parsed: serde_json::Value =
        serde_json::from_str(&out).expect("scan --json should be valid JSON");
    assert!(parsed.is_array(), "scan --json should be an array:\n{parsed}");
}

#[test]
fn tag_sets_and_replaces() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "tag-me", "-g", "tag test"]);
    ws.run_ok(&["tag", "tag-me", "rust", "cli"]);

    let out = ws.run_ok(&["show", "tag-me"]);
    assert!(out.contains("rust"), "show should contain tag rust:\n{out}");
    assert!(out.contains("cli"), "show should contain tag cli:\n{out}");

    // Replace tags
    ws.run_ok(&["tag", "tag-me", "testing"]);
    let out = ws.run_ok(&["show", "tag-me"]);
    assert!(
        out.contains("testing"),
        "show should contain replacement tag:\n{out}"
    );
    assert!(!out.contains("rust"), "old tag 'rust' should be gone:\n{out}");
}

#[test]
fn scope_sets_on_session() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "scope-test", "-g", "scope test"]);
    ws.run_ok(&[
        "scope",
        "scope-test",
        "Test",
        "scope",
        "content",
        "for",
        "this",
        "session",
    ]);

    let out = ws.run_ok(&["show", "scope-test"]);
    assert!(
        out.contains("Test scope content"),
        "scope should appear in show:\n{out}"
    );
}

#[test]
fn group_assigns_and_shows() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "group-test", "-g", "group test"]);
    ws.run_ok(&["group", "group-test", "-g", "my-group"]);

    let out = ws.run_ok(&["show", "group-test"]);
    assert!(
        out.contains("my-group"),
        "show should contain group name:\n{out}"
    );
}

#[test]
fn note_adds_to_session() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "note-test", "-g", "note test"]);
    let out = ws.run_ok(&["note", "note-test", "my test note"]);
    assert!(out.contains("noted"), "note command should confirm:\n{out}");
}
