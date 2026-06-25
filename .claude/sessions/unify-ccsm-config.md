# Session: unify-ccsm-config

> **pending** | started {{started}} | completed {{completed}} | {{pid_count}} pids

## Goal

Unify ccsm config/data to .ccsm/ in project root, add consumer tracking to session entries, cross-agent resume warnings

## Scope / Plan

(fill in — approach, constraints, what's in/out)

## Tags

(fill in)

## Live Session Data

| Field | Value |
|---|---|
| session_id | `(auto — ccsm manages)` |
| cwd | `/home/leviathanst/workspaces/tools/ccsm` |
| pids | (auto — ccsm manages) |
| kind | `(auto)` |
| version | `(auto)` |
| waitingFor | `(none)` |

## Progress Log

<!--
  Append dated entries as work happens. Keep newest at top.
  Format: [YYYY-MM-DD HH:MM] <note>
-->

- [2026-06-25 13:54Z] Implemented full migration from .claude/ → .ccsm/ for workspace data.

Changes made:
- registry.rs: Added `consumer: String` field to WorkspaceSession, changed all paths from .claude/ to .ccsm/, added backward-compat .claude/ fallback in load()
- consumer.rs: Added ccsm_dir(), ccsm_sessions_dir(), ccsm_group_dir(), ccsm_registry_path(), ccsm_lock_path(), ccsm_template_path(), ccsm_detail_path() methods
- main.rs: Changed all 15 workspace `.join(".claude")` to `.join(".ccsm")`, added `MigrateCcsm` CLI command with cross-consumer warning logic
- resume.rs: Added cross-consumer detection (warns when session's consumer != current consumer), updated detail path to .ccsm/
- doctor.rs: Updated all paths from .claude/ to .ccsm/
- sequence.rs: Added consumer field to WorkspaceSession constructor

New command: `ccsm migrate-ccsm` — copies registry+sessions+groups+templates from .claude/ to .ccsm/, stamps consumer="claude" on legacy entries. Safe to re-run.

Key design decisions:
1. Consumer field is stamped on first resume, not on creation — so you can create a session without deciding which agent will own it
2. Transcripts stay with their native agents (~/.claude/projects/, ~/.pi/agent/sessions/) — ccsm only manages the registry/metadata layer
3. .claude/ path in home dir (~/.claude/) is unchanged — only workspace-relative .claude/ is migrated

- [day20629T13:36:19Z] Session created

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none)

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->

## Checklist

<!--
  All items must be resolved before close gate allows completion.
  Status: pending | done | skipped | blocked
  Checkbox chars: - [ ] pending, - [x] done, - [~] skipped, - [!] blocked
-->

(no items yet — `ccsm check <name> "<text>" -s pending` adds one)
- [x] Move workspace data from .claude/ to .ccsm/
- [x] Add consumer field to registry schema
- [~] Store transcripts in .ccsm/transcripts/<agent>/
- [x] Cross-agent resume warning + fresh session fallback
- [x] Update Consumer paths to use .ccsm/ as canonical
- [x] Update Pi extension for new paths
- [x] Migration path: import existing .claude/ data into .ccsm/
