# Session: session-group

> **in_progress** | started 2026-06-20T15:32:40Z | completed — | 1 pid

## Goal

Group related sessions together with ordering — free (any order) or numeric rank

## Scope / Plan

Add group field to session entries: group.name (kebab slug) + group.rank (free | number). CLI: ccsm group <name> --group <g> [--rank free|<n>]. Query: ccsm list --group <g> [--by-rank]. Registry schema gets optional group object. free = order doesn't matter, numeric = ordinal priority within group. In: CLI, registry schema, list filtering, detail file section. Out: dependency graph, cross-group constraints, TUI rendering.

## Tags

grouping, ordering, session-lifecycle, registry

## Live Session Data

| Field | Value |
|---|---|
| session_id | `6c355ed8-7734-4480-8a54-a9cb112f4873` |
| cwd | `/home/leviathanst/workspaces/cc-tui` |
| pids | 45694 |
| kind | `claude-code` |
| version | `0.7.1` |
| waitingFor | — |

## Progress Log

<!--
  Append dated entries as work happens. Keep newest at top.
  Format: [YYYY-MM-DD HH:MM] <note>
-->

- [2026-06-20 15:54Z] END-GATE: Built group feature — GroupRank enum + Group struct, CLI (group assign/clear/overview, next, list --group/--by-rank), detail file ## Group section, sequence support. 65 tests pass, docs updated. NOT done: central group detail files (.claude/session-group/) — seeded as group-detail session.

- [2026-06-20 15:33Z] Resumed session. Detail file populated from template — all mechanical fields filled. Initializing checklist for implementation.

- [2026-06-20 15:32] Session resumed. Detail file initialized from template.

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

—

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->

## Checklist

<!--
  All items must be resolved before close gate allows completion.
  Status: pending | done | skipped | blocked
  Checkbox chars: - [ ] pending, - [x] done, - [~] skipped, - [!] blocked
-->

- [x] Add GroupRank enum + Group struct to registry data model (registry.rs)
- [x] Add Group + Next variants to Commands enum (main.rs)
- [x] Implement run_group() — set (--group + --rank), clear (--clear), overview (<name>)
- [x] Implement run_next() — next session in group by rank/alpha
- [x] Add --group <g> and --by-rank filters to list command
- [x] Update list output to show group column when filtering
- [x] Wire group data into detail file (populate ## Group section when set)
- [x] Rank collisions: accept (tie-break alphabetical)
- [x] Build + smoke test all paths

## Group

- **Group:** features
- **Rank:** 1
