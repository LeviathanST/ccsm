# Session: session-scope-awareness

> **pending** | started {{started}} | completed {{completed}} | {{pid_count}} pids

## Goal

Agents scan existing sessions before creating new ones, detect overlaps, update existing session scope when extending

## Scope / Plan

Add Cross-Session Teammate Awareness to the session-manager SKILL.md — a mandatory protocol that makes agents aware of other active sessions as *teammates*, not just log entries.

**Approach:**
1. Rust: `ccsm list --verbose` flag for token-efficient teammate scanning (full goal + tags)
2. Rust: `ccsm note --cross <source>` flag for auto-formatted cross-session annotations
3. SKILL.md: new "Cross-Session Teammate Awareness" section with 3 coordination patterns (Dependency, Redundancy, Related Work), re-scan triggers, decision flow, and cross-session note conventions

**In scope:** CLI flags, SKILL.md protocol, session detail file update
**Out of scope:** Sequence support for `--cross` (notes are filesystem ops, not registry mutations), automated overlap detection (NLP/embeddings), cross-workspace awareness

## Tags

agent-workflow, cross-session, teammate-awareness, coordination, protocol

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

- [2026-06-16 11:39Z] END-GATE: built — (1) Rust: --verbose flag on ccsm list (token-efficient teammate scan, ~80 tokens), --cross flag on ccsm note (auto-formatted cross-session annotations). (2) SKILL.md: Cross-Session Teammate Awareness protocol with 3 coordination patterns (Dependency/Redundancy/Related Work) + re-scan triggers + decision flow. (3) SKILL.md refactored from ~600-line monolith into 106-line router + 4 protocol files + 5 reference files (82% token reduction on default load). deferred — automated NLP overlap detection (out of scope), cross-workspace awareness (out of scope), sequence support for --cross (notes are FS ops, not registry mutations). left — older cruft sessions from doctor (separate cleanup), scope-gate-protocol session detail says done but may need status sync.

- [2026-06-16 11:24Z] SKILL.md refactored: split from ~600 lines monolith into central router (106 lines) + 4 protocol files + 5 reference files. Agents now read 106 lines on default load (82% reduction), pull protocol/reference files on demand via index tables.

- [2026-06-16 11:15Z] Rust: added `--verbose` flag to `ccsm list` for teammate scanning (full goal + tags, ~80 tokens for all actives). Added `--cross <source>` flag to `ccsm note` for auto-formatted cross-session annotations (`CROSS-SESSION [source]:` prefix). Fixed pre-existing duplicate alias panics on `clean-all` and `archive-all`.

- [2026-06-16 11:15Z] SKILL.md: added full "Cross-Session Teammate Awareness" section — ongoing re-scan triggers, 3 coordination patterns (Dependency/Redundancy/Related Work), cross-session note conventions with `--cross`, decision flow, relationship to other protocols. Updated CLI Commands table and Anti-Patterns.

- [2026-06-16 11:15Z] Filled scope/plan and tags in this detail file.

- [day20620T08:57:46Z] Session created

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

(none)

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->
