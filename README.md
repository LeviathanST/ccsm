# cc-tui

A TUI dashboard wrapper for Claude Code. Embeds the real Claude Code CLI inside a persistent terminal UI with sidebar panels for session management, task tracking, token costs, and git status.

## Why

Claude Code is powerful but its conversation UI is ephemeral — scroll past it, it's gone. cc-tui adds persistent panels that stay visible while you work:

- **Session manager** — browse, switch, and create sessions without restarting
- **Task dashboard** — live view of all tasks (auto-updates when Claude creates/completes them)
- **Token tracker** — see usage at a glance
- **Git status** — files changed, branch info
- **Subagent monitor** — track spawned agents

## Architecture

```
┌─ cc-tui ──────────────────────────────────────────────────────────┐
│ ┌─ Sidebar ────────┐ ┌─ Claude Code (real, via PTY) ─────────────┐ │
│ │ Sessions          │ │  Full Claude Code harness — tools, hooks, │ │
│ │ Tasks             │ │  permissions, skills, compaction.         │ │
│ │ Stats             │ │  100% ANSI passthrough.                   │ │
│ └──────────────────┘ └───────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────┘
```

**Augment, don't rebuild.** Claude Code runs untouched. The TUI only adds panels.

## Tech

- Rust + ratatui + crossterm + portable-pty
- Zero PTY parsing — all sidebar data from filesystem + hook bridges

## Status

Pre-alpha. Phase 1 (PTY spawning) in progress.
