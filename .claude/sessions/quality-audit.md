# Session: quality-audit

> **pending** | started — | completed —

## Goal

Full codebase quality audit before Phase 4-7 features.

## Scope / Plan

Systematic quality audit of all cc-tui modules before proceeding to Phase 4-7 features. Covers: compilation (zero warnings), module-by-module review (sidebar, registry, pty, ansi, session, main), edge case testing (empty state, single entry, overflow, rapid resize), CLI command verification (all subcommands produce correct output), and integration sanity (spawn, resume, sidebar refresh, trash/recover/clean lifecycle). Produces findings report with severity ratings.

## Tags

`audit` `testing` `quality` `stability`

## Audit Checklist

### 0. Compilation Gate
- [ ] `cargo build` — zero errors, zero warnings
- [ ] `cargo build --release` — zero errors, zero warnings
- [ ] `cargo clippy` — no new warnings
- [ ] `cargo test` — if tests exist, all pass (note: no test suite yet)

### 1. Sidebar (`src/sidebar.rs`)
- [ ] Empty state — 0 entries renders cleanly
- [ ] Single entry — one live, one registry, one trashed
- [ ] Overflow — 50+ entries, scroll works, selection stays visible
- [ ] Navigation — j/k/arrows work from all start positions
- [ ] Separator — not selectable, not counted in title
- [ ] Trash section — shows only when trashed entries exist
- [ ] Mouse clicks — select, double-click resume, scroll work
- [ ] Mouse drag resize — divider responds, clamped 5-80%
- [ ] Identity tracking — selection persists across refresh
- [ ] Deduplication — same-name entries don't shadow each other
- [ ] Refresh doesn't flicker — 2s poll cycle stable

### 2. Registry (`src/registry.rs`)
- [ ] Load empty file → returns empty WorkspaceRegistry
- [ ] Load valid file → all fields deserialize correctly
- [ ] Save → roundtrip preserves all data
- [ ] link_spawn — sets pid, session_id (only if empty), started
- [ ] refresh_from_live — fills empty session_ids, cleans stale pids
- [ ] trash/recover/clean — status transitions correct
- [ ] clean_all_trashed — batch delete works
- [ ] seed — only fills when empty
- [ ] project_slug — all non-alphanumeric → hyphen

### 3. CLI Commands
- [ ] `cc-tui --version` — shows correct version
- [ ] `cc-tui sessions` — lists all entries
- [ ] `cc-tui active` — only in_progress/blocked
- [ ] `cc-tui summary` — counts by status
- [ ] `cc-tui show <name>` — full detail output
- [ ] `cc-tui new <name> [goal]` — creates pending entry
- [ ] `cc-tui start <name>` — promotes to in_progress
- [ ] `cc-tui complete <name>` — completed + timestamp
- [ ] `cc-tui block <name>` — sets blocked
- [ ] `cc-tui abandon <name>` — sets abandoned
- [ ] `cc-tui pending <name>` — pending + clears identity
- [ ] `cc-tui scope <name> <text>` — updates scope
- [ ] `cc-tui tag <name> <tags...>` — replaces tags
- [ ] Error handling — missing name, nonexistent session

### 4. PTY (`src/pty.rs`)
- [ ] Fresh spawn — cc-tui launches cds correctly
- [ ] Resume — `--resume <session_id>` works when transcript exists
- [ ] Input passthrough — typing reaches cds
- [ ] Exit cleanup — child terminated gracefully
- [ ] Resize — PTY resizes when terminal resizes

### 5. ANSI (`src/ansi.rs`)
- [ ] Text styling — bold, italic, underline, colors render
- [ ] Grid rendering — all cells filled, no gaps
- [ ] Resize — screen resize followed by process works

### 6. Session (`src/session.rs`)
- [ ] load_all — filters by workspace correctly
- [ ] Skipped incomplete — entries without updated_at filtered out
- [ ] display_name — empty name → "unnamed"
- [ ] cwd_short — returns basename correctly

### 7. Main Event Loop (`src/main.rs`)
- [ ] Tab focus switching — sidebar ↔ PTY
- [ ] Ctrl+N wizard — topic picker shows pending + upcoming phases
- [ ] Wizard "Other..." → name input → spawn
- [ ] Ctrl+Q quit — graceful exit
- [ ] Session refresh — every 2s, sidebar updates
- [ ] Status bar — correct context in each mode

### 8. Integration
- [ ] Fresh start → Ctrl+N → pick topic → cds spawns
- [ ] Resume — select session → cds resumes with transcript
- [ ] Trash flow — d to trash, Enter to recover, D to clean
- [ ] Clean all — C removes all trashed
- [ ] Multiple registry states — completed + in_progress + blocked all display correctly

## Dependencies

Depends on: `session-registry` (status: in_progress) — CLI commands and registry API

## Notes

- Run with a real terminal, not in CI — PTY requires a TTY
- Check each edge case by manually manipulating `.claude/sessions.json`
- For overflow testing, seed 50+ fake entries
- Use `script` or `asciinema` to record test sessions for reference
