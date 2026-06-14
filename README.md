# cc-tui

A TUI dashboard wrapper for Claude Code. Embeds the real Claude Code CLI inside a persistent terminal UI with sidebar panels for session management.

## Why

Claude Code is powerful but its conversation UI is ephemeral — scroll past it, it's gone. cc-tui adds persistent panels:

- **Session manager** — browse, switch, create, trash, and resume sessions
- **Task dashboard** — live view of all tasks (auto-updates)
- **Token tracker** — see usage at a glance
- **Git status** — files changed, branch info
- **Subagent monitor** — track spawned agents

## Architecture

```
┌─ cc-tui ──────────────────────────────────────────────────────────┐
│ ┌─ Sidebar (30%) ───┐ ┌─ Claude Code (real, via PTY, 70%) ───────┐ │
│ │ Sessions           │ │  Full Claude Code harness — tools, hooks, │ │
│ │ Tasks              │ │  permissions, skills, compaction.         │ │
│ │ Stats              │ │  100% ANSI passthrough, zero parsing.     │ │
│ └───────────────────┘ └──────────────────────────────────────────┘ │
│ ┌─ Status Bar ────────────────────────────────────────────────────┐ │
│ │ cc-tui │ session │ shortcuts                                    │ │
│ └─────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────┘
```

**Augment, don't rebuild.** Claude Code runs untouched. The TUI only adds panels.

## Tech

- Rust + ratatui + crossterm + portable-pty + vt100
- Zero PTY parsing — all sidebar data from `~/.claude/sessions/*.json` + registry files

## Setup

cc-tui spawns `claude` directly (not through a shell wrapper). Required environment variables must be exported in your shell config so cc-tui inherits them:

```fish
# ~/.config/fish/config.fish
set -gx DISABLE_AUTOUPDATER 1
set -gx ANTHROPIC_BASE_URL https://api.deepseek.com/anthropic
set -gx ANTHROPIC_AUTH_TOKEN <your-token>
set -gx ANTHROPIC_MODEL deepseek-v4-pro[1m]
set -gx ANTHROPIC_DEFAULT_OPUS_MODEL deepseek-v4-pro[1m]
set -gx ANTHROPIC_DEFAULT_SONNET_MODEL deepseek-v4-pro[1m]
set -gx ANTHROPIC_DEFAULT_HAIKU_MODEL deepseek-v4-flash
set -gx CLAUDE_CODE_SUBAGENT_MODEL deepseek-v4-flash
set -gx CLAUDE_CODE_EFFORT_LEVEL max
```

```bash
cargo run              # Launch cc-tui (starts claude inside PTY)
cargo build --release  # Optimized build
cargo run -- /path/to/project  # Launch for a specific workspace
```

## Keybindings

### Global

| Key | Action |
|-----|--------|
| `Ctrl+Q` | Quit cc-tui |
| `Tab` | Switch focus between sidebar and Claude PTY |
| `Ctrl+N` | Create new session (opens name prompt) |

### Sidebar (when focused)

| Key | Action |
|-----|--------|
| `↑` / `↓` / `j` / `k` | Navigate session list |
| `Enter` | Resume selected session in PTY |
| `Enter` on 🗑 | Recover trashed session |
| `d` | Trash selected session (soft-delete, recoverable) |
| `D` (Shift+D) | Permanently clean selected session |
| `C` (Shift+C) | Clean all trashed sessions permanently |
| `Space` | Switch focus to PTY |

### PTY (when focused)

All keystrokes pass through to Claude Code. Standard terminal input.

### Input mode (new session prompt)

| Key | Action |
|-----|--------|
| `Enter` | Confirm session name |
| `Esc` | Cancel |

## Session Lifecycle

```
  Ctrl+N ──→ Pending ──→ InProgress (linked to live cds session)
                  │
                  ├── Enter ──→ resume via `claude --resume <id>`
                  │
                  ├── d ──→ Trashed (hidden, recoverable)
                  │         ├── Enter ──→ InProgress (recover)
                  │         └── D ──→ Clean (permanent delete)
                  │
                  └── D ──→ Clean (permanent delete, skips trash)
```

### What "Clean" deletes

Permanently removes all traces of a session:
- Transcript file: `~/.claude/projects/<slug>/<session-id>.jsonl`
- Session directory: `~/.claude/projects/<slug>/<session-id>/`
- Session files: `~/.claude/sessions/<pid>.json` containing that session ID
- Registry entry: removed from `.claude/sessions.json`

### What "Trash" keeps

Only marks the registry entry as `Trashed`. All files are preserved. Recover at any time.

## Data Sources

| Path | Use |
|------|-----|
| `~/.claude/sessions/<pid>.json` | Live session status |
| `~/.claude/projects/<slug>/<id>.jsonl` | Session transcripts |
| `<workspace>/.claude/sessions.json` | Workspace session registry |
| `~/.claude/sessions.json` | Global session overview |
| `~/.claude/tasks/<id>/*.json` | Task dashboard |

## Status

Phase 4 (sidebar + session management + trash/clean). PTY embedding stable, session resume working, trash/clean implemented.
