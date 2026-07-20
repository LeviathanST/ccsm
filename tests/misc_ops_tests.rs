mod common;
use common::*;

#[test]
fn misc_trash_recover() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "trash-test", "-g", "test"]);
    ws.run_ok(&["start", "trash-test"]);

    let out = ws.run_ok(&["trash", "trash-test"]);
    assert!(out.contains("trashed"), "trash: {}", out);

    let out = ws.run_ok(&["list", "--status", "trashed"]);
    assert!(out.contains("trash-test"), "trashed list: {}", out);

    let out = ws.run_ok(&["show", "trash-test"]);
    assert!(out.contains("trashed"), "show after trash: {}", out);

    let out = ws.run_ok(&["recover", "trash-test"]);
    assert!(out.contains("recovered"), "recover: {}", out);

    let out = ws.run_ok(&["list", "--active"]);
    assert!(out.contains("trash-test"), "recovered active: {}", out);
}

#[test]
fn misc_close_gate() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "close-me", "-g", "test goal"]);
    ws.run_ok(&["start", "close-me"]);

    ws.write_detail(
        "close-me",
        "\
## Scope / Plan

Test scope for close gate.

## Tags

test, close

## Live Session Data

session_id: auto
started: auto
pids: none

## Progress Log

- [2026-07-20 10:00Z] Session created
",
    );
    ws.run_ok(&["note", "close-me", "implemented the feature"]);

    let out = ws.run_ok(&["close", "close-me"]);
    assert!(out.contains("Self-review"), "close output: {}", out);
}

#[test]
fn misc_close_fails_on_hollow_detail() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "hollow", "-g", "test"]);
    ws.run_ok(&["start", "hollow"]);

    let err = ws.run_err(&["close", "hollow"]);
    assert!(err.contains("gate"), "close should fail: {}", err);
}

#[test]
fn misc_depend() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "dep-a", "-g", "first"]);
    ws.run_ok(&["new", "dep-b", "-g", "second", "--force"]);
    ws.run_ok(&["group", "dep-a", "--group", "my-group"]);
    ws.run_ok(&["group", "dep-b", "--group", "my-group"]);
    ws.run_ok(&["depend", "dep-b", "--on", "dep-a"]);

    let out = ws.run_ok(&["depend", "dep-b"]);
    assert!(out.contains("dep-a"), "dep list: {}", out);

    // Registry should have the dependency recorded
    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "dep-b")
        .unwrap();
    let deps = entry["depends_on"].as_array().unwrap();
    assert_eq!(deps.len(), 1, "should have 1 dependency");
    assert_eq!(deps[0], "dep-a", "dependency should be dep-a");
}

#[test]
fn misc_checklist_with_feat_template() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "feat-session", "-c", "feat", "-g", "feature work"]);

    let out = ws.run_ok(&["checklist", "feat-session"]);
    assert!(
        out.contains("pending"),
        "checklist should have pending items: {}",
        out
    );
    assert!(
        out.contains("4 items"),
        "checklist should have 4 items: {}",
        out
    );
}

#[test]
fn misc_check_item() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "check-session", "-c", "feat", "-g", "test"]);

    // Check item #1 as done
    let out = ws.run_ok(&["check", "check-session", "1", "-s", "done"]);
    assert!(out.contains("done"), "check done: {}", out);

    // Verify via checklist listing
    let out = ws.run_ok(&["checklist", "check-session"]);
    assert!(out.contains("1 done"), "one done: {}", out);
    assert!(out.contains("3 pending"), "three still pending: {}", out);
}

#[test]
fn misc_completions() {
    ensure_built();
    let ws = TempWorkspace::new();

    let out = ws.run_ok(&["completions", "bash"]);
    assert!(
        out.contains("ccsm"),
        "completions should contain 'ccsm': {}",
        out
    );
    assert!(
        out.contains("complete"),
        "completions should contain 'complete': {}",
        out
    );
}

#[test]
fn misc_gate_check_strict_fails_without_scope() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "no-scope", "-g", "test goal"]);

    let out = ws.run(&["gate-check", "no-scope", "--strict"]);
    assert!(!out.status.success(), "gate-check --strict should fail");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("GATE: FAIL"),
        "gate-check output: {}",
        stdout
    );
}

#[test]
fn misc_archive_completed_session() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "archive-me", "-g", "test"]);
    ws.run_ok(&["start", "archive-me"]);

    ws.write_detail(
        "archive-me",
        "\
## Scope / Plan

Test scope.

## Tags

test, archive

## Live Session Data

session_id: auto
started: auto
pids: none

## Progress Log

- [2026-07-20 10:00Z] Session created
",
    );
    ws.run_ok(&["note", "archive-me", "did the work"]);

    ws.run_ok(&["complete", "archive-me", "--force"]);

    let out = ws.run_ok(&["archive", "archive-me"]);
    assert!(out.contains("archived"), "archive output: {}", out);
}


#[test]
fn misc_depend_clear() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "clear-a", "-g", "test"]);
    ws.run_ok(&["new", "clear-b", "-g", "test", "--force"]);
    ws.run_ok(&["depend", "clear-b", "--on", "clear-a"]);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "clear-b")
        .unwrap();
    assert_eq!(entry["depends_on"].as_array().unwrap().len(), 1);

    ws.run_ok(&["depend", "clear-b", "--clear"]);

    let reg = ws.read_registry();
    let entry = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "clear-b")
        .unwrap();
    let deps = entry["depends_on"].as_array().unwrap();
    assert!(deps.is_empty(), "deps should be cleared after --clear");
}

#[test]
fn misc_clean_all_clears_trashed() {
    ensure_built();
    let ws = TempWorkspace::new();

    ws.run_ok(&["new", "trash-for-clean", "-g", "test"]);
    ws.run_ok(&["start", "trash-for-clean"]);
    ws.run_ok(&["trash", "trash-for-clean"]);

    let out = ws.run_ok(&["clean-all"]);
    assert!(out.contains("cleaned"), "clean-all output: {}", out);

    let reg = ws.read_registry();
    let all: Vec<_> = reg["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["status"] == "trashed")
        .collect();
    assert!(all.is_empty(), "all trashed sessions should be removed");
}

#[test]
fn misc_doctor_healthy() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "healthy-session", "-g", "test"]);
    let out = ws.run_ok(&["doctor"]);
    assert!(out.contains("ccsm"), "doctor output: {out}");
}

#[test]
fn misc_branch_set_and_show() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "branch-test", "-g", "test"]);
    ws.run_ok(&["branch", "branch-test", "feature-x"]);
    let out = ws.run_ok(&["show", "branch-test"]);
    assert!(out.contains("feature-x"), "branch show: {out}");
}

#[test]
fn misc_branch_clear() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "branch-clr", "-g", "test"]);
    ws.run_ok(&["branch", "branch-clr", "feature-y"]);
    ws.run_ok(&["branch", "branch-clr", "--clear"]);
    let out = ws.run_ok(&["show", "branch-clr"]);
    assert!(!out.contains("feature-y"), "branch cleared: {out}");
}

#[test]
fn misc_next_shows_session() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "next-session", "-g", "test goal"]);
    ws.run_ok(&["group", "next-session", "-g", "my-group"]);
    let out = ws.run_ok(&["next", "my-group"]);
    assert!(out.contains("next-session"), "next should show: {out}");
}

#[test]
fn misc_note_check_succeeds() {
    ensure_built();
    let ws = TempWorkspace::new();
    ws.run_ok(&["new", "nc-session", "-g", "test"]);
    ws.run_ok(&["start", "nc-session"]);
    ws.run_ok(&["note", "nc-session", "test note"]);
    let out = ws.run(&["note-check"]);
    assert!(out.status.success(), "note-check should pass: {}",
        String::from_utf8_lossy(&out.stderr));
}
