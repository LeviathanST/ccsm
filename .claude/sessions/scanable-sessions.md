# Session: scanable-sessions

> **pending** | started {{started}} | completed {{completed}} | {{pid_count}} pids

## Goal

Make sessions can be scanable with less effort, token optimize, time optimize which are perfect for agents to scan and know sessions do what by a quick check

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

- [2026-06-21 13:10Z] Pushed 888d57f — feat: ccsm scan, doctor vague-goal checks, doc updates

- [2026-06-21 13:07Z] Implemented ccsm scan subcommand: compact grouped output with --search/--group/--status/--json flags. Format uses grep-friendly field markers. Added vague-goal detection to ccsm doctor (short goals, name-as-goal, CLI artifacts). Updated seed-session SKILL.md with keyword-rich goal guidelines. Updated CLAUDE.md design decision #8.

- [day20625T12:44:03Z] Session created

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none)

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->
