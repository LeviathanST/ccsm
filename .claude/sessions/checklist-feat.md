# Session: checklist-feat

> **in_progress** | started 2026-06-20T15:15:00Z | completed (none) | 1 pid

## Goal

Each session can have a checklist — all items must be done before the close gate allows completion

## Scope / Plan

Add checklist section to session detail files. CLI: ccsm checklist <name> (list items), ccsm check <name> <item> --status <pending|done|skipped|blocked> (toggle). ccsm close gate blocks completion if any item is pending/blocked. Detail file gets ## Checklist section with markdown checkboxes. In: CLI, detail file section, close-gate integration. Out: TUI rendering, templates, cross-session aggregation.

## Tags

checklist, close-gate, session-lifecycle, detail-file

## Live Session Data

| Field | Value |
|---|---|
| session_id | `3b65d05b-1e59-48fa-bc6d-c60d6c79ab49` |
| cwd | `/home/leviathanst/workspaces/cc-tui` |
| pids | 27712 |
| kind | `claude` |
| version | `0.7.1` |
| waitingFor | `(none)` |

## Progress Log

<!--
  Append dated entries as work happens. Keep newest at top.
  Format: [YYYY-MM-DD HH:MM] <note>
-->

- [2026-06-20 15:26Z] Checklist section is now opt-in: ccsm new -c adds it, ccsm checklist --init adds it later. Template stays clean.

- [2026-06-20 15:23Z] Implemented checklist CLI subcommands — checklist, check with --status, gate integration in run_gate_checks, template section

- [2026-06-20 15:17Z] Resume — filled template fields, added checklist to detail file, starting implementation

- [2026-06-20 15:15] Resume — filled template fields from registry, starting implementation
- [day20624T15:13:10Z] Session created

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none)

## Checklist

<!--
  All items must be resolved before close gate allows completion.
  Status: pending | done | skipped | blocked
-->

- [x] Parse checklist from detail file ## Checklist section
- [x] ccsm checklist <name> — list items with status
- [x] ccsm check <name> <item> --status <pending|done|skipped|blocked> — toggle/write item
- [x] ccsm close gate — blocks completion if pending/blocked items exist
- [ ] Integration test: close gate blocks on pending item
- [ ] Integration test: close gate passes when all items done/skipped

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->
