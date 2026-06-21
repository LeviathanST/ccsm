# Session: group-roadmap

> **in_progress** | started day20625T03:02Z | completed (not yet) | 1 pid

## Goal

Auto-render group roadmap markdown from session data

## Scope / Plan

ccsm group roadmap <name> reads group detail file + all member sessions, generates a markdown table with rank, status, goal, scope per session. Renders dependency graph if depends_on exists. Output goes to stdout (pipeable to file). No new data stores — pure render from existing registry + detail files + depends_on. ALSO: ccsm doctor detects missing essential files (session-detail-template.md, .claude/session-group/ dir) and auto-creates them.

## Tags

grouping, rendering, markdown, ergonomics

## Live Session Data

| Field | Value |
|---|---|
| session_id | `9862a1e8-60d3-42ab-9e5c-b0a7e3791531` |
| cwd | `/home/leviathanst/workspaces/cc-tui` |
| pids | 134148 |
| kind | `claude` |
| version | `0.7.1` |
| waitingFor | `(none)` |

## Progress Log

<!--
  Append dated entries as work happens. Keep newest at top.
  Format: [YYYY-MM-DD HH:MM] <note>
-->

- [2026-06-21 03:28Z] END-GATE: Built — (1) ccsm group <name> --roadmap renders markdown table + mermaid dep graph from live registry (2) ccsm doctor auto-creates missing template + .claude/sessions/ dir. Not done — (a) nested subcommand syntax (ccsm group roadmap <name> vs --roadmap flag) was deferred for clap refactor later (b) no test group with real data in this workspace. All 65 tests pass. 4 commits pushed.

- [2026-06-21 03:26Z] feat: doctor auto-creates missing session-detail-template.md + .claude/sessions/ dir. Template embedded as TEMPLATE_CONTENT const. Two paths: (1) ccsm doctor detects → auto-creates → reports in 🔧 section, (2) ccsm new auto-creates silently → detail file creation proceeds normally. Root cause fix: previously template absence caused silent detail-file skip.

- [2026-06-21 03:21Z] Updated skill docs: added group/depends_on to registry-schema.md, added Documentation Discipline rule to CLAUDE.md — every new feature must be documented in skill references.

- [2026-06-21 03:12Z] feat: add --roadmap flag to Group command — renders markdown table (rank/status/goal/scope) + Mermaid graph TD for depends_on. Output to stdout, pipeable. Pure query, no new stores. Options: --roadmap flag on Group struct, run_group_roadmap() fn with helpers (status_icon, read_session_section, truncate_md, md_escape_pipe). 65 tests pass, e2e verified with 5-session demo group.

- [day20625T03:00:54Z] Session created

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none)

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->
