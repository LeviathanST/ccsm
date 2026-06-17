//! Integration tests: ccsm rename (basic, with -g/-s, edge cases).

mod common;
use common::*;

#[test]
fn rename_basic() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "old-name", "-g", "old goal"]);
    ws.run_ok(&["start", "old-name"]);

    let out = ws.run_ok(&["rename", "old-name", "new-name"]);
    assert!(out.contains("renamed"), "rename: {}", out);

    // Verify old name gone, new name exists
    let reg = ws.read_registry();
    let names: Vec<&str> = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"old-name"), "old name should be gone");
    assert!(names.contains(&"new-name"), "new name should exist");
}

#[test]
fn rename_with_goal_and_scope() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "topic-old", "-g", "old goal"]);
    ws.run_ok(&["start", "topic-old"]);

    let out = ws.run_ok(&[
        "rename", "topic-old", "topic-new",
        "-g", "rewritten goal",
        "-s", "rewritten plan",
    ]);
    assert!(out.contains("renamed"), "rename: {}", out);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "topic-new")
        .unwrap();
    assert_eq!(entry["goal"], "rewritten goal");
    assert_eq!(entry["scope"], "rewritten plan");
    assert_eq!(entry["name"], "topic-new");
}

#[test]
fn rename_nonexistent_session() {
    ensure_built();
    let ws = TempWorkspace::new();

    let err = ws.run_err(&["rename", "no-such", "new-name"]);
    assert!(err.contains("no session named"), "nonexistent: {}", err);
}

#[test]
fn rename_rejects_existing_name() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "alpha", "-g", "a"]);
    ws.run_ok(&["new", "beta", "-g", "b", "--force"]);

    let err = ws.run_err(&["rename", "alpha", "beta"]);
    assert!(err.contains("already exists"), "duplicate name: {}", err);
}

#[test]
fn rename_rejects_non_kebab() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "valid-name", "-g", "test"]);
    let err = ws.run_err(&["rename", "valid-name", "Invalid Name"]);
    assert!(err.contains("kebab-case"), "kebab validation: {}", err);
}

#[test]
fn rename_updates_registry_status_and_tags_preserved() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "keep-status", "-g", "status test"]);
    ws.run_ok(&["start", "keep-status"]);
    ws.run_ok(&["tag", "keep-status", "urgent", "bug"]);

    ws.run_ok(&["rename", "keep-status", "kept-status"]);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "kept-status")
        .unwrap();
    assert_eq!(entry["status"], "in_progress");
    let tags: Vec<&str> = entry["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    assert_eq!(tags, vec!["urgent", "bug"]);
}
