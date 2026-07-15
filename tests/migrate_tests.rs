//! Integration tests: `ccsm migrate` — auto-chain migration.

mod common;
use common::*;

/// Identity starts as version "1" (legacy) and migrate runs the full
/// chain to bring it to the current binary version.
#[test]
fn migrate_from_v1_to_current() {
    ensure_built();
    let ws = TempWorkspace::new();

    // Identity was created with version "1" by TempWorkspace
    let stderr = ws.run_stderr(&["migrate"]);
    assert!(stderr.contains("normalize pre-semver"), "should run identity normalize: {stderr}");
    assert!(stderr.contains("migrated from v1"), "should report migration: {stderr}");

    // Identity file should now be at current version
    let identity = ws.read_identity();
    assert!(identity.contains(&format!(r#"version = "{}""#, env!("CARGO_PKG_VERSION"))),
        "identity should be at current version, got: {identity}");
}

/// Running migrate when already at current version is a no-op.
#[test]
fn migrate_noop_when_current() {
    ensure_built();
    let ws = TempWorkspace::new();

    // Run migrate once to bring to current
    ws.run_ok(&["migrate"]);

    // Run again — should be no-op
    let stderr = ws.run_stderr(&["migrate"]);
    assert!(stderr.contains("nothing to migrate"), "second run should be no-op: {stderr}");
}

/// Unknown version in non-interactive mode warns and leaves identity untouched.
#[test]
fn migrate_unknown_version_warns() {
    ensure_built();
    let ws = TempWorkspace::new();

    // Overwrite identity with an unknown version
    ws.set_identity_version("0.99.0");

    let stderr = ws.run_stderr(&["migrate"]);
    // The chain runner hits the unknown version and fast-forwards
    assert!(stderr.contains("no data changes"), "should fast-forward: {stderr}");

    // Identity should now be at current (fast-forward bumps it)
    let identity = ws.read_identity();
    assert!(identity.contains(&format!(r#"version = "{}""#, env!("CARGO_PKG_VERSION"))),
        "should fast-forward to current: {identity}");
}

/// Version gap with no chain entry fast-forwards.
#[test]
fn migrate_fast_forwards_gap() {
    ensure_built();
    let ws = TempWorkspace::new();

    // Set to a version after the last chain entry but before current
    // The chain goes up to "0.17.0", so "0.17.5" should fast-forward
    ws.set_identity_version("0.17.5");

    let stderr = ws.run_stderr(&["migrate"]);
    assert!(stderr.contains("fast-forward"), "should fast-forward: {stderr}");

    let identity = ws.read_identity();
    assert!(identity.contains(&format!(r#"version = "{}""#, env!("CARGO_PKG_VERSION"))),
        "identity at current after fast-forward: {identity}");
}

/// Full chain from v1 runs all expected steps and ends at current.
#[test]
fn migrate_full_chain_steps() {
    ensure_built();
    let ws = TempWorkspace::new();

    let stderr = ws.run_stderr(&["migrate"]);
    assert!(stderr.contains("normalize pre-semver"), "step normalize");
    assert!(stderr.contains("rehome data from .ccsm"), "step rehome");
    assert!(stderr.contains("strip stale worktree"), "step strip");
    assert!(stderr.contains("seed config"), "step seed");
}
