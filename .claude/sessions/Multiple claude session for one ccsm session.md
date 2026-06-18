# Session: Multiple claude session for one ccsm session

> **in_progress** | started 2026-06-18 10:58Z | completed — | 1 pid

## Goal

Enable multiple claude session for a ccsm session to avoid chaos, large context window or produce some approaches

## Scope / Plan

Implement ccsm refresh: retire current Claude session to retired_session_ids, spawn fresh (no --resume), inject CCSM_SESSION. Added session close gate, hardened complete, note-check for Stop hook. Global hook wiring via setup.sh.

## Tags

session-lifecycle, refresh, close-gate, hooks, setup

## Live Session Data

| Field | Value |
|---|---|
| session_id | `37bf5f37-0a88-43b4-9fca-c265219df6f4` |
| cwd | `/home/leviathanst/workspaces/cc-tui` |
| pids | 419095 |
| kind | `claude` |
| version | `0.7.0` |
| waitingFor | `(none)` |

## Progress Log

<!--
  Append dated entries as work happens. Keep newest at top.
  Format: [YYYY-MM-DD HH:MM] <note>
-->

- [2026-06-18 13:45Z] END-GATE: built — ccsm refresh (retire+spawn), ccsm close (pre-completion gate with hard checks), hardened ccsm complete --force, ccsm note-check (time-based Stop hook), RetiredSession schema, seed-session skill, hook wiring in setup.sh. deferred — unified data directory (pending session created), settings.local.json hook cleanup. left — commit and close this session.

- [2026-06-18 12:32Z] Created seed-session skill — slash command for quick pending session setup. User gives name + rough description, agent synthesizes scope + tags, creates via ccsm sequence (pending only, no start).

- [2026-06-18 12:02Z] Built session close gate (ccsm close + hardened complete + note-check Stop hook). 3 new subcommands: close (detail completeness checks, exits non-zero), complete --force (bypass gate), note-check (hook: dirty tree→nudge). Wired Stop hook in settings.local.json. Updated CLAUDE.md + session-manager SKILL.md. 0.7.0, 65 tests pass.

- [2026-06-18 11:22Z] Removed Agent Workflow (MANDATORY) section from CLAUDE.md — CCSM_SESSION env var + inject-scope hook on SessionStart/UserPromptSubmit mechanically handles session discovery. Cut ~50 lines of dead agent instructions. Added refresh + rename to CLI reference.

- [2026-06-18 11:14Z] Implemented ccsm refresh: new subcommand retires current session_id to retired_session_ids with timestamp+reason, spawns fresh claude (no --resume), injects CCSM_SESSION env var. Added RetiredSession struct, show displays retired history, doctor flags >=3 refreshes. Builds clean, 65 tests pass.

- [day20622T10:56:46Z] Session created

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none)


## Notes

- Renamed scope mid-session: evolved from multi-session to refresh (retire+spawn) mechanism.
- `ccsm close` auto-detects template residue — mechanical, not heuristic.
- Stop hook note-check: time-based (2 min), not git-diff — avoids false positives.
- Setup.sh now installs: CLAUDE.md section, session-manager, seed-session, 3 hooks (SessionStart, UserPromptSubmit, Stop).


