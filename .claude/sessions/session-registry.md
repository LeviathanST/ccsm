# Session: session-registry

> **in_progress** | started day20618T06:51Z | completed — | ~20 pids

## Goal

Two-tier session registry for team visibility + agent skills + CLI tooling

## Scope / Plan

Tier 1: global overview at `~/.claude/sessions.json` scanning all workspaces.
Tier 2: per-repo `.claude/sessions.json` with goal/scope/status/tags.
Auto-merges live session data, survives ephemeral session cleanup.
Built session-manager skill with `always:true`, team awareness protocol,
token-efficient CLI commands, topic picker wizard, and cc-tui setup subcommand.

## Tags

`registry` `sessions` `team` `skills` `cli` `wizard` `sidebar`

## Progress Log

- [2026-06-16 13:34Z] RETROACTIVE-COMPLETE: foundational mega-session (June 14, 10 hours) that built the entire ccsm skeleton — two-tier registry, CLI, sidebar, session-manager skill, Ctrl+N picker, merge strategies. Was left blocked without END-GATE. Binary later renamed cc-tui → ccsm.

- [2026-06-14 16:30] Created `quality-audit` pending session with 55-point checklist covering
  8 audit areas: compilation, sidebar, registry, CLI, PTY, ANSI, session, integration.
  Ready for Ctrl+N picker.
- [2026-06-14 16:00] Added `cc-tui pending <name>` CLI command — the last lifecycle state
  without a dedicated command. Resets status to pending, clears session_id/pids/timestamps.
  Documented in skill + CLAUDE.md. v0.3.2 built and pushed.
- [2026-06-14 15:30] Sub-agent review (rune/haiku) of sidebar.rs found 4 more issues:
  unnamed live session suppression, scroll_into_view timing, separator is_registry flag,
  selection fallback when entry has no session_id. All fixed.
- [2026-06-14 14:30] Sidebar polish: BTreeMap deterministic ordering, select_next infinite
  loop guard, scroll_into_view offset management via ratatui's offset_mut().
- [2026-06-14 14:00] Sidebar thorough audit: navigation, mouse click/double-click/scroll,
  drag-to-resize sidebar (5-80%), layout fixed (Constraint::Length instead of Percentage),
  version bump 0.3.0 → 0.3.1, release build + push to github.com/LeviathanST/cc-tui.
- [2026-06-14 13:30] Full sidebar code review — 7 bugs found: select_next skips first entry,
  select_prev doesn't go to last, separator selectable, session count includes separator,
  trashed shadows completed, label-based identity fragile, live matching misses Blocked.
  All fixed in one pass.
- [2026-06-14 12:30] Fixed registry re-corruption: promote-in-place when picking topics,
  demote old InProgress→Completed for handoff, transcript file check before --resume.
  cc-tui v0.3.0 built.
- [2026-06-14 12:00] Simplified sidebar label matching to per-session (session_id/pid),
  removed global InProgress name override that caused all sessions to share one label.
- [2026-06-14 11:30] Added Focus::Wizard to isolate picker input from sidebar/PTY.
  Fixed "no conversation found" by spawning fresh for registry entries without live cds.
- [2026-06-14 11:00] Topic picker: dynamic height, styled overlay, Ctrl+N wizard.
  Session handshake protocol in skill (new topic vs resume).
- [2026-06-14 10:30] CLI mutation commands: new, start, complete, block, abandon, scope, tag.
  Session detail template + per-session .md files.
  cc-tui sessions/active/summary/show subcommands.
- [2026-06-14 10:00] Migrated Ascendra's .claude/sessions.md → .claude/sessions.json.
  5 completed + 10 pending entries.
- [2026-06-14 09:30] merge_live_sessions Strategy 3: pid-based matching.
  Sidebar hides pending entries. Session detail files.
- [2026-06-14 09:00] cc-tui setup subcommand: appends session mandate to ~/.claude/CLAUDE.md,
  installs session-manager skill globally, creates empty registry.
- [2026-06-14 08:00] Session-manager skill with always:true, team awareness protocol,
  token efficiency (CLI > jq), session handshake (new vs existing).
- [2026-06-14 07:00] Two-tier session registry: GlobalRegistry + WorkspaceRegistry.
  merge_live_sessions Strategies 1 & 2. Seed entries for cc-tui phases.
- [2026-06-14 06:51] Session started. Goal: build session registry infrastructure
  that works across Claude, Codex, Gemini, and other AI tools.

## Dependencies

None — this is the foundation session. `phase-4-task-dashboard` through `phase-7-polish`
depend on this session's registry API being stable.

## Notes

### Key decisions
- Registry names are canonical; live session names (from cds session files) are ephemeral.
- At most one InProgress per workspace — handoff via demote + promote.
- Pending entries are hidden from sidebar, shown only in Ctrl+N picker.
- `--resume` only when transcript file exists on disk (checked via filesystem).
- Symlink to release binary: `~/.local/bin/cc-tui → target/release/cc-tui`.

### Known bugs remaining (for next session)
- (none — navigation fixed, persistence likely fixed, needs more usage to confirm)

### Architecture
```
~/.claude/sessions.json          ← Global overview (auto-built)
<project>/.claude/sessions.json  ← Workspace registry (canonical)
<project>/.claude/sessions/      ← Per-session detail .md files
~/.claude/skills/session-manager/ ← Global skill (always:true)
```

## Group

- **Group:** demo-group
- **Rank:** 3
