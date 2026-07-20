## Parallel Test Isolation for Env-Dependent Tests

Symptom:
Tests that set process-global env vars (`CCSM_DATA_DIR`) pass in isolation but fail when run in parallel (default `cargo test` behavior). The `global_data_dir()` function reads `CCSM_DATA_DIR`, so concurrent tests clobber each other's temp directories.

Cause:
`std::env::set_var` is process-global — parallel tests race on the same env var key. Each test sets a different temp path, but another test reads the wrong value mid-flight.

Fix:
Use a static `Mutex<()>` to serialize env-dependent test sections. The `with_data_dir` helper acquires the lock before setting the env var and restores the previous value on drop.

```rust
use std::sync::Mutex;
static ENV_LOCK: Mutex<()> = Mutex::new(());
```

```rust
fn with_data_dir<F: FnOnce()>(f: F) {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let prev = std::env::var("CCSM_DATA_DIR").ok();
    unsafe { std::env::set_var("CCSM_DATA_DIR", data_dir.to_string_lossy().as_ref()); }
    f();
    match prev {
        Some(v) => unsafe { std::env::set_var("CCSM_DATA_DIR", v); },
        None => unsafe { std::env::remove_var("CCSM_DATA_DIR"); },
    }
}
```

Alternatively, pass `--test-threads=1` to `cargo test` for the specific binary, but that serializes ALL tests including non-env-dependent ones. The Mutex approach is more targeted.

Evidence:
- `ensure_data_dir_creates_directory_structure` and `global_config_path_within_data_dir` failed in CI under parallel test runs
- All 27+ registry tests pass reliably after serialization
- Cross-referenced in `general-audit-for-enhancements` session (2026-07-20)
