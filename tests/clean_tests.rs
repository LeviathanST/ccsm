//! Integration tests: ccsm clean, clean-all, archive, archive-all.

mod common;
use common::*;

#[test]
fn clean_removes_entry() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "clean-me", "-g", "disposable"]);
    ws.run_ok(&["start", "clean-me"]);

    // Attach a fake UUID so clean tries to delete files from it
    ws.run_ok(&["attach", "clean-me", "f493397b-456a-426d-92e1-4d5f15da0311"]);

    let out = ws.run_ok(&["clean", "clean-me"]);
    assert!(out.contains("cleaned"), "clean: {}", out);

    let reg = ws.read_registry();
    let found = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "clean-me");
    assert!(!found, "entry should be removed");
}

#[test]
fn clean_nonexistent_session() {
    ensure_built();
    let ws = TempWorkspace::new();

    let err = ws.run_err(&["clean", "no-such"]);
    assert!(err.contains("no session named"), "nonexistent: {}", err);
}

#[test]
fn clean_all_trashed_only() {
    ensure_built();
    let ws = TempWorkspace::new();

    // Create two, trash both
    ws.run_ok(&["new", "trash-1", "-g", "t1", "--force"]);
    ws.run_ok(&["new", "trash-2", "-g", "t2", "--force"]);
    ws.run_ok(&["trash", "trash-1"]);
    ws.run_ok(&["trash", "trash-2"]);

    let out = ws.run_ok(&["clean-all"]);
    assert!(out.contains("cleaned"), "clean-all: {}", out);

    let reg = ws.read_registry();
    assert!(reg["sessions"].as_array().unwrap().is_empty(), "all should be removed");
}

#[test]
fn clean_all_empty_is_ok() {
    ensure_built();
    let ws = TempWorkspace::new();

    let out = ws.run_ok(&["clean-all"]);
    assert!(out.contains("no trashed"), "empty clean-all: {}", out);
}
