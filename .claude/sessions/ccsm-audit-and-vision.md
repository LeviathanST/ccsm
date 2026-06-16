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

- [2026-06-16 13:34Z] Session created. scope and tags filled. Preceded by completing stale session-registry (foundational mega-session from June 14).

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none — this is the strategic planning session)

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->

### Prelim scan (done before session creation)

- 38 unit tests pass, 0 integration tests
- main.rs: 1,343 lines — half the codebase, growing fast
- 5 cruft sessions with empty goals/no detail files
- 2 completed sessions with template residue (scope-gate-protocol, proactive-ideation-dashboard)
- All `unwrap()` calls in test blocks — main path uses anyhow::Result
- Binary naming drift: repo is cc-tui, binary is ccsm, remote says ccsm.git
- session-registry had blocked+completed inconsistency (now fixed)
