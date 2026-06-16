# Session: ccsm-audit-and-vision

> **in_progress** | started {{started}} | completed {{completed}} | {{pid_count}} pids

## Goal

Audit ccsm stability, identify gaps, and decide next-phase vision

## Scope / Plan

Stability audit of ccsm codebase + vision planning for next phase.

**Audit areas:**
- Test coverage (unit only, no integration tests)
- Error handling and panic surface
- main.rs monolith risk (1,343 lines, growing)
- CLI surface consistency and naming (cc-tui vs ccsm)
- Registry edge cases and lifecycle bugs
- Session state inconsistencies (blocked+completed bug, template residue)

**Vision areas:**
- What is ccsm as a product? CLI? TUI? Both?
- Binary naming: cc-tui vs ccsm reconciliation
- Dependency automation (ccsm depend <a> <b>)
- Cross-workspace awareness
- Integration tests and CI
- Release process (versioning, changelog)

**In scope:** codebase audit, structured assessment, prioritized session backlog
**Out of scope:** implementing fixes (separate sessions), cross-project features beyond cc-tui workspace

## Tags

audit, vision, planning, roadmap, stability

## Live Session Data

| Field | Value |
|---|---|
| session_id | `(auto — ccsm manages)` |
| cwd | `/home/leviathanst/workspaces/cc-tui` |
| pids | (auto — ccsm manages) |
| kind | `(auto)` |
| version | `(auto)` |
| waitingFor | `(none)` |

## Progress Log

<!--
  Append dated entries as work happens. Keep newest at top.
  Format: [YYYY-MM-DD HH:MM] <note>
-->

- [2026-06-16 14:58Z] Added 'On Session RESUME' awareness protocol to setup.sh — agents now instructed to run ccsm list --active before any other output

- [2026-06-16 14:49Z] Completed audit: test coverage, monolith risk, CLI surface, registry edges, vision/backlog. Wrote findings to detail file.

- [2026-06-16 13:34Z] Session created. scope and tags filled. Preceded by completing stale session-registry (foundational mega-session from June 14).

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none — this is the strategic planning session)

## Audit Findings

### 1. Test Coverage & Error Handling

**Tests:** 38 unit tests pass (0 integration). Distribution:
- `sequence.rs`: 22 tests (parse + apply_op + pipeline) — best-covered module
- `registry.rs`: 16 tests (LockFile, load_locked, serialization, clean/archive)
- `main.rs`: 0 tests — all 1364 lines are untested
- `session.rs`: 0 tests — minimal file, low risk

**Error handling assessment:**
- Production path uses anyhow::Result throughout — good
- ONE production `unwrap()`: `main.rs:279` in `mutate_session` — the re-borrow for status line display. "Safe" because the closure above already found the entry, but fragile under refactor.
- Several silent error swallows with `let _ =` in `clean()`, `archive()`, seed template writes — fs operations fail silently
- `run_resume` Phase 5: `eprintln!` warnings if session file parse/read fails — doesn't abort, which is correct but means tracking can silently degrade

**Top gaps:**
1. **Zero integration tests** — no end-to-end CLI test (`ccsm new → start → complete`)
2. **Zero main.rs unit tests** — all subcommand handlers are untested
3. `run_resume` (187 lines, 7-phase spawn/harvest lifecycle) has no test coverage at all

### 2. Main.rs Monolith Risk

- **1364 lines** (was 1343 at session start — grew 21 lines for `completions`)
- **50% of codebase** (2743 lines across 4 files)
- Growing at ~20 lines per feature — will hit 2000 lines by v0.8

**Extractable modules:**
| Candidate | Lines | Notes |
|---|---|---|
| `run_resume` | 187 | 7-phase spawn/harvest/wait lifecycle — deserves own module |
| `run_doctor` | 180 | Health scanning — self-contained, no shared state |
| `run_list` + `run_show` | ~160 | Query operations, read-only |
| `datetime helpers` | ~50 | `now_iso_ts`, `days_to_date`, `is_leap`, `note_timestamp` — DUPLICATED with `registry.rs::now_iso` |
| `edit_distance` | 20 | Pure function, belongs in a utils module |
| `insert_note` + `parse_sections` | ~80 | Markdown manipulation — could be a `detail.rs` module |

**Duplication:** `now_iso_ts()` in main.rs and `now_iso()` in registry.rs are identical logic — one should import the other.

**Template residue in main.rs:** `#[allow(dead_code)]` on all three modules (line 1-6) — leftover from early development.

### 3. CLI Surface Consistency & Naming

**The name problem:**
| Thing | Name |
|---|---|
| Repo directory | `cc-tui` |
| Cargo package | `ccsm` |
| Binary | `ccsm` |
| Git remote | `ccsm.git` |

The `cc-tui` repo name implies a TUI app that doesn't exist yet. The binary has *always* been `ccsm`. Resolution: either rename the repo to `ccsm` (pure CLI tool) or commit to building the TUI (cc-tui uses ccsm as its CLI entry point).

**CLI surface issues:**
- `--verbose` on `list` means "teammate scan mode" — misleading name, should be `--detailed` or `--long`
- `--cross` on `note` is cryptic — should be `--from` or `--source`
- `completions` subcommand added in latest commit, uses hardcoded `"ccsm"` binary name (line 1320)
- No `--json` flag on `list` or `show` — makes scripting harder for non-Rust consumers
- Session names have no validation — spaces, slashes, and special chars are accepted, creating unopenable detail files

**Session naming conventions:**
- 5 legacy sessions have spaces in names ("Session picking proble from Ascendra project", "Claude Code panel scrollable") — these predate ccsm's kebab-case convention
- `doctor` correctly flags them but doesn't offer a rename path

### 4. Registry Edge Cases & Lifecycle Bugs

**Seed data staleness:** `default_seed()` lists `session-registry` as `InProgress` but it's `completed` in the live registry — seed only matters for fresh repos but it's misleading.

**Locking gaps:**
- `resume` Phase 1→3 gap: two processes could race between lock/release cycles — Phase 1 locks, promotes, saves, unlocks; then Phase 3 locks again. Between these, another process could `start` the same session.
- `sequence` solves this for mutations but `resume` can't be batched.

**Status transitions:**
- No transition validation — you can go `completed → in_progress` without `pending` intermediate
- `recover` always sets `InProgress` even if the session was `Completed` before trashing
- `Pending` clears identity fields but doesn't force goal/scope reset

**Fragile matching:**
- `clean()` finds session files via `contents.contains(session_id)` — substring match, could hit unrelated files
- `archive()` has the same pattern
- `trash()`/`recover()`/`clean()` fall back to name matching for empty session_id — could match wrong entry if duplicate names existed

**Detail file <-> registry integrity:**
- No FK enforcement — detail files can exist without registry entries and vice versa
- `doctor` detects this but can't auto-fix
- Template `{{placeholder}}` detection in doctor is a good start

**Lifecycle completeness:**
- `blocked` status exists but no `ccsm unblock` command (must use `start`)
- No `ccsm rename` command
- No way to merge duplicate sessions

### 5. Agent Self-Awareness Gap (🔴 found during audit)

**Problem:** Agents spawned via `ccsm resume` don't know which ccsm session they belong to. `ccsm resume` passes `-n <session-name>` to `claude`, but the agent has no mechanism to discover this. The agent opens with "What are we working on today?" — wasting turns on context recovery.

**Root cause:** The ccsm session identity is stored in the registry and the `-n` flag, but it's not injected into the agent's startup context. The agent can't:
- Know its own PID (to read `~/.claude/sessions/<pid>.json`)
- Read the `-n` flag that was passed to the claude process
- Distinguish which `in_progress` session is its own (if there are multiple workspaces)

**Proposed fix (two-tier):**

*Short-term — CLAUDE.md instruction (today):*
```markdown
## Session Awareness (MANDATORY ON STARTUP)

1. Run `ccsm list --active`
2. If exactly one `in_progress` session: `ccsm show <name>` to load goal, scope, and progress
3. If multiple active sessions: ask which one you're continuing
4. Never open with "What are we working on?" — the session registry already knows
```

*Medium-term — env var injection (code change):*
In `run_resume` Phase 2, set `CCSM_SESSION=<name>` on the child process environment:
```rust
cmd.env("CCSM_SESSION", name);
```
Then agents just read `$CCSM_SESSION` and run `ccsm show $CCSM_SESSION`.

**Priority:** This should be done *before* the next session starts. Every agent session wastes turns on context discovery without it.

### 6. Vision & Prioritized Backlog

**Product identity decision (unresolved):** Is ccsm a standalone CLI tool, or is cc-tui the product that ccsm is merely the CLI entry point for? The repo name implies the latter but all code is the former. This decision gates everything below.

**Recommended v0.7 sessions (priority order):**

| # | Session | Why |
|---|---|---|
| 1 | `integration-tests` | Unblocks all refactoring. Test `new→start→note→complete` end-to-end. |
| 2 | `main-rs-modularization` | Extract `resume`, `doctor`, `detail` modules before 2000-line threshold |
| 3 | `session-name-validation` | Reject spaces/slashes/special chars. Validate on `new`. Prevent unopenable detail files. |
| 4 | `deduplicate-datetime` | Unify `now_iso_ts` / `now_iso` — single source of truth in registry |
| 5 | `binary-naming-resolution` | Either rename repo to `ccsm` OR commit to cc-tui TUI roadmap |

**v0.8 candidates:**
| # | Session | Why |
|---|---|---|
| 6 | `cross-workspace-awareness` | `ccsm list --all`, global overview, multi-repo team coordination |
| 7 | `dependency-automation` | `ccsm depend <a> <b>`, auto-block when dep completes |
| 8 | `json-output` | `--json` on list/show for scripting |
| 9 | `lifecycle-validation` | Enforce status transitions, prevent invalid state changes |
| 10 | `ci-and-release` | GitHub Actions: build + test + clippy. Changelog, version tags. |

**v1.0 gate:** All v0.7 + v0.8 items complete. Binary naming resolved. Integration tests passing in CI. No production `unwrap()` calls remaining.

### Prelim scan (done before session creation)

- 38 unit tests pass, 0 integration tests
- main.rs: 1,343 lines — half the codebase, growing fast
- 5 cruft sessions with empty goals/no detail files
- 2 completed sessions with template residue (scope-gate-protocol, proactive-ideation-dashboard)
- All `unwrap()` calls in test blocks — main path uses anyhow::Result
- Binary naming drift: repo is cc-tui, binary is ccsm, remote says ccsm.git
- session-registry had blocked+completed inconsistency (now fixed)
