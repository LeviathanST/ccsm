# Session: Integrate recorded-lesson-issues into ccsm

> **in_progress** | started 2026-06-25T05:57Z | completed 2026-06-25T06:35Z | 1 pid

## Goal

Decide if we should integrate the skill as our core concept

## Scope / Plan

Evaluate whether learned-lesson-issue skill should integrate as a ccsm core concept.
Conversational design session — discuss architecture, refine, implement agreed approach.
Outcome: keep as separate pull/push skills sharing .claude/lessons/ data store.
Deliverables: reshaped wrap-up SKILL.md, updated learned-lesson-issue SKILL.md,
.claude/lessons/INDEX.md + migrated lessons, updated setup.sh.
Out of scope: code changes to ccsm, new CLI surface, hook implementation.

## Tags

architecture, skills, lessons, wrap-up, learned-lesson-issue, session-lifecycle

## Live Session Data

| Field | Value |
|---|---|
| session_id | `dd706ac6-682f-40e8-bbce-2020774f4093` |
| cwd | `/home/leviathanst/workspaces/cc-tui` |
| pids | 253522 |
| kind | `claude` |
| version | `2.x` |
| waitingFor | `(none)` |

## Progress Log

<!--
  Append dated entries as work happens. Keep newest at top.
  Format: [YYYY-MM-DD HH:MM] <note>
-->

- [2026-06-25 06:31Z] END-GATE: Built wrap-up skill (2-phase Ship+Learn), reshaped learned-lesson-issue (pull at debug time), created .claude/lessons/ with INDEX.md table-pointer + migrated 5 session-lifecycle lessons, updated setup.sh to install all 4 skills. NOT done: hook implementation for mid-session lesson nudges, cleanup of old skill references/ dir. Left: lesson corpus needs to grow through actual usage before any mechanical tooling is justified.

- [2026-06-25 06:26Z] Architected lesson capture system: (1) Created wrap-up skill in ccsm bundle — 2-phase Ship+Learn replacing Reddit original, (2) Updated learned-lesson-issue SKILL.md to point at .claude/lessons/ with INDEX.md table-pointer, (3) Created .claude/lessons/INDEX.md + migrated session-lifecycle.md, (4) Updated setup.sh to install all 4 skills (session-manager, seed-session, wrap-up, learned-lesson-issue). Two separate skills with shared data store: learned-lesson-issue = pull (debug-time check), wrap-up Phase 2 = push (closeout record). Old references/ dir in skill left as-is during migration.

- [day20629T05:57:11Z] Session created

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none)

## Notes

- Decided against ccsm integration — lessons are a skill-level concern, not a CLI/data-model concern
- learned-lesson-issue = pull (debug-time), wrap-up Phase 2 = push (closeout). Separate skills, shared .claude/lessons/ data store
- wrap-up reshaped from 4-phase Reddit version to 2-phase Ship+Learn (killed Publish It, merged Review & Apply into Learn)
- .claude/lessons/INDEX.md as table-pointer for token-efficient agent access (~20 tokens to scan)
- Moved wrap-up into ccsm bundle, removed stale global copy
- setup.sh installs all 4 skills: session-manager, seed-session, wrap-up, learned-lesson-issue
- Old learned-lesson-issue/references/ dir still exists — cleanup deferred
