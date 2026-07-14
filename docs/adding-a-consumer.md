# Adding a New Consumer

ccsm abstracts AI coding agents behind the `Consumer` enum in `src/consumer.rs`.
Each consumer has different binary names, config paths, session file formats, and
resume flags. This checklist ensures nothing is missed.

## Checklist

### 1. Enum variant (`src/consumer.rs`)

Add the variant to `Consumer`:

```rust
pub enum Consumer {
    Claude,
    Pi,
    YourAgent,
}
```

### 2. Required methods

Every variant must be handled in these match blocks:

| Method | Returns | Example (YourAgent) |
|--------|---------|---------------------|
| `binary()` | `&str` — CLI binary name | `"youragent"` |
| `home_config_dir()` | `&str` — subdir under `$HOME` | `".config/youragent"` |
| `project_slug()` | `String` — deterministic path-to-slug | Claude-style raw replace or Pi-style collapsed |
| `sessions_dir()` | `PathBuf` — live session data directory | `home.join(".local/share/youragent")` |
| `projects_dir()` | `PathBuf` — per-project session data | `home.join(".local/share/youragent")` |
| `live_session_file()` | `Option<PathBuf>` — PID-based file or None | `None` (if no PID files) |
| `find_session_file()` | `Option<PathBuf>` — check if session exists | Query DB or check file |
| `list_session_files()` | `Vec<PathBuf>` — all session files | Query DB or list dir |
| `system_prompt_tags()` | `(&str, &str)` — open/close tags | `("<system-reminder>", "</system-reminder>")` |
| `constraint_line()` | `&str` — constraint message | See existing implementations |
| `is_youragent()` | `bool` — helper | `matches!(self, Self::YourAgent)` |
| `parse()` | `Result<Self>` — string parsing | `"youragent"` `"ya"` → `YourAgent` |
| `Display` | Formatted name | `write!(f, "youragent")` |

### 3. Auto-detect (`Consumer::auto_detect()`)

Add your agent's config dir/DB path to the candidates list. Priority is
most-recently-modified wins. Default fallback should be the preferred consumer
on new systems.

### 4. DB/data helpers (if applicable)

If your agent uses a database (e.g. SQLite), add free functions near the
Consumer impl:

```rust
pub fn youragent_db_path(home: &Path) -> PathBuf { ... }
pub fn youragent_session_exists(db_path: &Path, session_id: &str) -> bool { ... }
pub fn youragent_harvest_session(db_path: &Path, ...) -> Option<String> { ... }
```

Add dependencies to `Cargo.toml` as needed (e.g. `rusqlite`).

### 5. Resume logic (`src/commands/resume.rs`)

Three match blocks need your variant:

1. **Worktree spawn** — `sh -c "cd <wt> && exec agent <flags>"`
2. **Direct spawn** — `Command::new(binary()).args([...])`
3. **Harvest (Phase 5)** — Poll PID file or DB for new session ID

### 6. Fresh spawn / Refresh (`src/main.rs`)

Update the `run_refresh()` match with your agent's fresh-start flags.

### 7. Setup (`src/main.rs`)

Add a `Consumer::YourAgent` arm to `run_setup()`. This should install any
config files, plugins, or skill files needed by your agent.

### 8. Attach auto-discover (`src/main.rs`)

Update the auto-discover match in `run_attach()` to find live sessions for
your agent (query DB, scan dir, etc.).

### 9. Registry guards (`src/registry.rs`)

If your agent doesn't use transcript files on disk:

- `clean()` — guard file deletion with `!consumer.is_youragent()`
- `archive()` — guard file deletion with `!consumer.is_youragent()`

### 10. Rename (`src/main.rs`)

If your agent stores session names in a DB, update `run_rename()` to sync
the title when `consumer.is_youragent()`.

### 11. Documentation

- `docs/adding-a-consumer.md` — this file (update with your agent's specifics)
- `.claude/skills/session-manager/reference/cli-commands.md` — consumer table
- `CLAUDE.md` — if default consumer changes
- `README.md` — if new consumer is notable

### 12. Tests

Add test cases to the `#[cfg(test)] mod tests` block in `consumer.rs`:

```rust
assert_eq!(Consumer::parse("youragent").unwrap(), Consumer::YourAgent);
assert_eq!(Consumer::YourAgent.binary(), "youragent");
assert!(Consumer::YourAgent.is_youragent());
```

Run `cargo test` to verify all tests pass.

## Architecture Notes

- The `Consumer` enum is **stateless** — all methods take `self` and parameters.
- Session data paths should follow XDG conventions when possible.
- If your agent uses a SQLite DB, prefer `rusqlite` with `bundled` feature for
  zero system-dependency installation.
- The plugin system (for OpenCode's JS/TS plugins) lives in `plugins/<consumer>/`.
