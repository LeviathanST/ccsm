# Session: groups-list

> **in_progress** | started 2026-06-21 02:21Z | completed — | 1 pid

## Goal

Add --list flag to ccsm group to list all groups in workspace

## Scope / Plan

Added `--list` flag + `-l` short flag to `ccsm group`. Scans registry for all unique group names, prints each with member count, in_progress/pending status breakdown, and goal snippet from group detail file. `run_groups_list()` uses BTreeMap for determinstic ordering. Made `name` optional (Option<String>) so `--list` works without a required positional arg.

## Tags

grouping, cli, discovery

## Live Session Data

| Field | Value |
|---|---|
| session_id | `(auto)` |
| cwd | `/home/leviathanst/workspaces/cc-tui` |
| pids | (auto — ccsm manages) |
| kind | `claude` |
| version | `0.7.1` |
| waitingFor | `(none)` |

## Progress Log

<!--
  Append dated entries as work happens. Keep newest at top.
  Format: [YYYY-MM-DD HH:MM] <note>
-->

- [2026-06-21 02:23Z] END-GATE: Added --list flag to ccsm group. Scans registry for unique groups, shows member counts + status breakdown + goal snippet. 65 tests pass.

- [2026-06-21 02:23Z] Built --list flag: ccsm group --list shows all groups with member counts, status breakdown, goal snippet. 65 tests pass.

- [day20625T02:21:20Z] Session created

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none)

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->
