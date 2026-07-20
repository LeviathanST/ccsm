//! Integration tests: session lifecycle (new → start → note → complete).

mod common;
use common::*;

#[test]
fn lifecycle_new_start_complete() {
    ensure_built();
    let ws = TempWorkspace::new();

    // Create a session
    let out = ws.run_ok(&["new", "my-session", "-g", "test goal"]);
    assert!(out.contains("created"), "new: {}", out);

    // Start it
    let out = ws.run_ok(&["start", "my-session"]);
    assert!(out.contains("start"), "start: {}", out);

    // Verify status shows in_progress
    let out = ws.run_ok(&["list", "--active"]);
    assert!(out.contains("my-session"), "active list: {}", out);
    assert!(out.contains("in_progress"), "active list: {}", out);

    // Populate detail file to satisfy gate checks
    ws.write_detail(
        "my-session",
        "\
## Scope / Plan

Test scope — verify lifecycle.

## Tags

test, lifecycle

## Live Session Data

session_id: auto
started: day20622T10:00Z
pids: none

## Progress Log

- [2026-06-18 10:00Z] Session created
",
    );
    let out = ws.run_ok(&["note", "my-session", "implemented the thing"]);
    assert!(out.contains("noted"), "note: {}", out);

    // Complete
    let out = ws.run_ok(&["complete", "my-session"]);
    assert!(out.contains("complete"), "complete: {}", out);

    // Verify status is completed
    let out = ws.run_ok(&["list", "--status", "completed"]);
    assert!(out.contains("my-session"), "completed list: {}", out);

    // Show — check goal and status
    let out = ws.run_ok(&["show", "my-session"]);
    assert!(out.contains("completed"), "show status: {}", out);
    assert!(out.contains("test goal"), "show goal: {}", out);
}

#[test]
fn lifecycle_start_block_abandon() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "blocked-session", "-g", "blocked test"]);
    ws.run_ok(&["start", "blocked-session"]);
    ws.run_ok(&["block", "blocked-session"]);

    let out = ws.run_ok(&["list", "--status", "blocked"]);
    assert!(out.contains("blocked-session"), "blocked list: {}", out);

    // Abandon
    ws.run_ok(&["abandon", "blocked-session"]);

    let out = ws.run_ok(&["list", "--status", "abandoned"]);
    assert!(out.contains("blocked-session"), "abandoned list: {}", out);
}

#[test]
fn lifecycle_trash_recover() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "trash-me", "-g", "trash test"]);
    ws.run_ok(&["start", "trash-me"]);

    // Trash
    let out = ws.run_ok(&["trash", "trash-me"]);
    assert!(out.contains("trashed"), "trash: {}", out);

    let out = ws.run_ok(&["list", "--status", "trashed"]);
    assert!(out.contains("trash-me"), "trashed list: {}", out);

    // Recover
    let out = ws.run_ok(&["recover", "trash-me"]);
    assert!(out.contains("recovered"), "recover: {}", out);

    let out = ws.run_ok(&["list", "--active"]);
    assert!(out.contains("trash-me"), "recovered active: {}", out);
}

#[test]
fn lifecycle_pending_clears_identity() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "reset-me", "-g", "reset test"]);
    ws.run_ok(&["start", "reset-me"]);

    // Manually attach a fake session_id to simulate a linked session
    ws.run_ok(&["attach", "reset-me", "f493397b-456a-426d-92e1-4d5f15da0311"]);

    // Pending — should clear identity fields
    let out = ws.run_ok(&["pending", "reset-me"]);
    assert!(out.contains("reset"), "pending: {}", out);

    // Verify session_id cleared
    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "reset-me")
        .unwrap();
    assert_eq!(entry["session_id"], "", "session_id should be cleared");
    assert!(
        entry["pids"].as_array().unwrap().is_empty(),
        "pids should be cleared"
    );
    assert_eq!(entry["status"], "pending");
}

#[test]
fn new_rejects_duplicate() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "unique-session", "-g", "first"]);
    let err = ws.run_err(&["new", "unique-session", "-g", "second"]);
    assert!(err.contains("already exists"), "duplicate error: {}", err);
}

#[test]
fn new_force_skips_fuzzy_duplicate() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "test", "-g", "original"]);
    // Without --force, "test2" is blocked because "test" is similar
    let err = ws.run_err(&["new", "test2", "-g", "similar"]);
    assert!(err.contains("looks similar"), "fuzzy error: {}", err);

    // With --force, it works
    let out = ws.run_ok(&["new", "test2", "-g", "similar", "--force"]);
    assert!(out.contains("created"), "force create: {}", out);
}

#[test]
fn list_summary_and_status_filter() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "session-a", "-g", "one"]);
    ws.run_ok(&["new", "session-b", "-g", "two", "--force"]);
    ws.run_ok(&["start", "session-a"]);

    // Summary
    let out = ws.run_ok(&["list", "--summary"]);
    assert!(out.contains("1 active"), "summary active: {}", out);
    assert!(out.contains("2 total"), "summary total: {}", out);

    // --status filter with invalid status (writes to stderr, exits 0)
    let out = ws.run(&["list", "--status", "bogus"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown status"),
        "invalid status: {}",
        stderr
    );

    // --status help (writes to stderr)
    let out = ws.run(&["list", "--status", "help"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("pending"), "status help: {}", stderr);
}
