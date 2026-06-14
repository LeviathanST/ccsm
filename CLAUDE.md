# cc-tui: A TUI Dashboard Wrapper for Claude Code

## Identity

You are Vex, building a persistent TUI wrapper around Claude Code. This project augments Claude Code — it does NOT replace it.

## Core Principle

**Augment, don't rebuild.** Claude Code's harness (agent loop, tools, hooks, permissions, compaction, sessions, slash commands, skills) runs untouched inside a PTY. This TUI adds persistent sidebar panels that the default CLI doesn't have. Nothing is reimplemented — every Claude Code update is free.

## Architecture

```
┌─ cc-tui (Rust binary) ───────────────────────────────────────────┐
│ ┌─ Sidebar (30%) ────┐ ┌─ Claude Code PTY (70%) ────────────────┐ │
│ │ Sessions            │ │  Real `claude` process, full harness   │ │
│ │ Tasks               │ │  ANSI passthrough — zero rendering     │ │
│ │ Token stats         │ │  Input: Tab switches focus             │ │
│ │ Git status          │ │                                        │ │
│ │ Subagents           │ │                                        │ │
│ └─────────────────────┘ └───────────────────────────────────────┘ │
│ ┌─ Status Bar ───────────────────────────────────────────────────┐ │
│ │ cc-tui v0.1 │ Session: X │ active │ 24K tokens │ 4 files       │ │
│ └────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

## Tech Stack

- **Rust** (matches user's ecosystem)
- **ratatui** 0.29+ with **crossterm** backend (terminal UI)
- **portable-pty** (spawn and manage Claude Code process)
- **notify** (filesystem watcher for session/task files)
- **serde/serde_json** (parse Claude Code's JSON data files)

## Data Sources (zero PTY parsing)

Every panel reads from filesystem files or hook event bridges. Claude Code's PTY output is rendered untouched — never parsed.

### Files on Disk (poll/watch)

| Path | Contains | Use For |
|------|----------|---------|
| `~/.claude/tasks/<session>/*.json` | All tasks: id, subject, status, blocks | Task dashboard |
| `~/.claude/sessions/<pid>.json` | Live sessions: sessionId, cwd, status, name | Session list |
| `~/.claude/history.jsonl` | Every prompt typed, with timestamp + project | Session search |
| `~/.claude/stats-cache.json` | Daily aggregated: messageCount, sessionCount, toolCallCount | Token dashboard (aggregate) |
| `~/.claude/projects/<project>/<session>.jsonl` | Full transcript: every message, tool_use block | Detailed session replay |
| `.claude/sessions.md` | Custom session board (user-authored) | Session manager CRUD |
| `.claude/sessions/<name>.md` | Per-session detail: goal, scope, progress | Session detail panel |

### Hook Events (real-time bridges)

Pattern: hook → writes to a bridge file → TUI watches file → panel updates

| Hook Event | Carries | Bridge To |
|-----------|---------|-----------|
| `TaskCreated` | task data | Task dashboard |
| `TaskCompleted` | task data | Task dashboard |
| `PreToolUse` | tool_name, tool_input | Tool log panel |
| `PostToolUse` | tool_name, tool_output (exit code, stdout) | Tool log panel |
| `SubagentStart` | agent_type, agent_id, task | Subagent tracker |
| `SubagentStop` | agent_type, agent_id | Subagent tracker |
| `Stop` | (fires when Claude finishes responding) | Token update trigger |
| `SessionStart` | source (startup/resume/clear), model | Session lifecycle |

### Status Line (runs every render frame)

User already has a custom status line via Node.js script. The TUI can embed its own status bar using the same data sources.

## Design Decisions

1. **PTY embedding, not Agent SDK.** User explicitly rejected rebuilding Claude Code's conversation UI. The PTY approach gives 100% of Claude Code's features for free.
2. **Never parse PTY output.** The golden rule from klaudio-panels. All sidebar data comes from filesystem + hooks.
3. **Input routing: Tab toggles focus.** When sidebar has focus → arrow keys navigate. When Claude PTY has focus → all keystrokes pass through.
4. **Hook-to-file bridge pattern.** Hooks write events to a JSONL file (`/tmp/cc-events.jsonl` or similar). TUI tail-follows it. Zero coupling between hook logic and TUI render loop.
5. **ratatui, not a web UI.** Terminal-native keeps it in the same environment as Claude Code. No browser context switch.

## What We Know About Claude Code's Data Surface

### Task files are rich
Each task at `~/.claude/tasks/<session-id>/<id>.json`:
```json
{
  "id": "47",
  "subject": "Task 1: AnimationKind enum",
  "description": "Add AnimationKind enum...",
  "activeForm": "Adding AnimationKind",
  "status": "completed",
  "blocks": [],
  "blockedBy": []
}
```
Also: `.highwatermark` (next available ID) and `.lock` files in each task directory.

### Session JSONL transcript
Append-only JSONL at `~/.claude/projects/<project>/<session>.jsonl`. Contains:
- `type: "assistant"` with `message.content[]` blocks (text, tool_use with name+input)
- `type: "user"` with tool_result blocks
- `type: "system"`, `type: "file-history-snapshot"`, `type: "mode"`
- Parent UUID chain for branching/resume

### Notable gaps
- **Per-turn token usage**: Not in hook payloads or JSONL (verified). Only available via `stats-cache.json` (daily aggregate).
- **TaskCreated/TaskCompleted hook schema**: Full JSON fields not documented yet. Easy to discover: point a hook at a stdout-dump script and create a task.
- **Streaming tool output**: Only available mid-execution via PTY. Not worth parsing — accept post-hoc updates via PostToolUse.

## Implementation Phases

| Phase | What | Effort |
|-------|------|--------|
| 1 | Spawn `claude` in PTY, render in ratatui (one panel, no sidebar) | ~2h |
| 2 | Add sidebar: read sessions.md, render session list, keyboard nav | ~3h |
| 3 | Focus switching: Tab toggles between sidebar and Claude PTY | ~1h |
| 4 | Live panels: task dashboard from ~/.claude/tasks/, git status | ~2h |
| 5 | Hook bridges: TaskCreated/Completed → file → TUI updates | ~2h |
| 6 | Token dashboard from stats-cache.json | ~1h |
| 7 | Polish: themes, mouse, resize, scrollback | ~2h |

## Build & Run

```bash
cargo run              # Launch cc-tui (starts claude inside PTY)
cargo build --release  # Optimized build
```

TUI keybindings:
- `Tab` — switch focus between sidebar and Claude PTY
- `↑/↓` — navigate sidebar (when focused)
- `Enter` — select session / confirm
- `n` — new session
- `q` — quit (also sends exit to Claude)
- `Ctrl+C` — force quit

## Related Resources

- [Claude Code Hooks Reference](https://code.claude.com/docs/en/hooks) — 30 hook events
- [Claude Code .claude Directory Guide](https://code.claude.com/docs/en/claude-directory) — File layout
- [klaudio-panels](https://explore.market.dev/ecosystems/typescript/projects/klaudio-panels) — Reference PTY wrapper (Tauri+SolidJS)
- [claude-agent-tui](https://github.com/severity1/claude-agent-tui) — Go+BubbleTea TUI components
