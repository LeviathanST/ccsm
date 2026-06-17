//! Integration tests: ccsm sequence (batch mutations).

mod common;
use common::*;

#[test]
fn sequence_new_start_complete() {
    ensure_built();
    let ws = TempWorkspace::new();

    let out = ws.run_ok(&[
        "sequence",
        "-q", "new", "seq-test", "batch goal",
        "-q", "start", "seq-test",
        "-q", "scope", "seq-test", "batch approach",
        "-q", "tag", "seq-test", "batch", "test",
        "-q", "complete", "seq-test",
    ]);
    assert!(out.contains("created"), "seq new: {}", out);
    assert!(out.contains("start"), "seq start: {}", out);
    assert!(out.contains("complete"), "seq complete: {}", out);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "seq-test")
        .unwrap();
    assert_eq!(entry["status"], "completed");
    assert_eq!(entry["goal"], "batch goal");
    assert_eq!(entry["scope"], "batch approach");
}

#[test]
fn sequence_trash_recover() {
    ensure_built();
    let ws = TempWorkspace::new();

    // New → Start → Trash → Recover in one sequence
    ws.run_ok(&[
        "sequence",
        "-q", "new", "seq-trash", "test",
        "-q", "start", "seq-trash",
        "-q", "trash", "seq-trash",
        "-q", "recover", "seq-trash",
    ]);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "seq-trash")
        .unwrap();
    assert_eq!(entry["status"], "in_progress");
}

#[test]
fn sequence_blocks_duplicate() {
    ensure_built();
    let ws = TempWorkspace::new();

    let err = ws.run_err(&[
        "sequence",
        "-q", "new", "dup", "first",
        "-q", "new", "dup", "second",
    ]);
    assert!(err.contains("already exists"), "duplicate: {}", err);
}

#[test]
fn sequence_empty_is_error() {
    ensure_built();
    let ws = TempWorkspace::new();

    // No args after subcommand — clap catches this as a required args error
    let out = ws.run(&["sequence"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("required"), "empty seq: {}", stderr);
}
