# Session: group-detail

> **in_progress** | started 2026-06-20 15:54Z | completed — | 1 pid

## Goal

Central group markdown files at .claude/session-group/<name>.md

## Scope / Plan

Added `.claude/session-group/` directory with per-group markdown files (## Goal / ## Scope / ## Members / ## Notes). Added `--goal` flag to `ccsm group`. Group files auto-create on first `--group` join with member list, auto-delete on last `--clear`. Overview (`ccsm group <name>`) reads and displays the group Goal section. Helpers: `ensure_group_file`, `update_group_members`, `set_group_goal`, `group_file_path`. No registry format changes. Updated docs: `--help` text and CLAUDE.md.

## Tags

grouping, detail-files, markdown, session-lifecycle

## Live Session Data

| Field | Value |
|---|---|
| session_id | `c8438d48-3b46-473d-af4c-a1df686bd6c1` |
| cwd | `/home/leviathanst/workspaces/cc-tui` |
| pids | 66057 |
| kind | `claude` |
| version | `0.7.1` |
| waitingFor | `(none)` |

## Progress Log

<!--
  Append dated entries as work happens. Keep newest at top.
  Format: [YYYY-MM-DD HH:MM] <note>
-->

- [2026-06-21 02:57Z] END-GATE: Built depends_on field + ccsm depend (add/clear/list with same-group validation) + dep-aware ccsm next (skips blocked) + ccsm group-deps (tree render). All 65 tests pass. Group detail files + --list + --goal from prior scope also done. Nothing left.

- [2026-06-21 02:56Z] Built depends_on: Vec<String> field, ccsm depend subcommand (add/clear/list with same-group validation), dep-aware ccsm next (skips blocked), ccsm group-deps (dependency tree). All 65 tests pass.

- [2026-06-20 16:13Z] END-GATE: Built group detail markdown files — auto-create on join, auto-clean on last leave, --goal flag, overview display. All 65 tests pass. Docs updated (--help + CLAUDE.md). Nothing left undone.

- [2026-06-20 16:08Z] Built group detail markdown files — auto-create on first join, auto-clean on last leave, --goal flag, overview display integration. All 65 tests pass.

- [day20624T15:52:55Z] Session created

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none)

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->
