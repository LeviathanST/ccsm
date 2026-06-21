# Session: group-roadmap

> **pending** | started {{started}} | completed {{completed}} | {{pid_count}} pids

## Goal

Auto-render group roadmap markdown from session data

## Scope / Plan

(fill in — approach, constraints, what's in/out)

## Tags

(fill in)

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

- [2026-06-21 03:21Z] Updated skill docs: added group/depends_on to registry-schema.md, added Documentation Discipline rule to CLAUDE.md — every new feature must be documented in skill references.

- [2026-06-21 03:12Z] feat: add --roadmap flag to Group command — renders markdown table (rank/status/goal/scope) + Mermaid graph TD for depends_on. Output to stdout, pipeable. Pure query, no new stores. Options: --roadmap flag on Group struct, run_group_roadmap() fn with helpers (status_icon, read_session_section, truncate_md, md_escape_pipe). 65 tests pass, e2e verified with 5-session demo group.

- [day20625T03:00:54Z] Session created

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none)

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->
