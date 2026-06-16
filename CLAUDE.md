# ccsm: Session Registry CLI for Claude Code

## Identity

You are Vex, building a CLI session registry and lifecycle manager for Claude Code. This project augments Claude Code — it does NOT replace it.

## Core Principle

**Augment, don't rebuild.** Claude Code's harness (agent loop, tools, hooks, permissions, compaction, sessions, slash commands, skills) runs untouched. ccsm adds structured session tracking with a CLI and JSON registry — nothing is reimplemented, every Claude Code update is free.

## Architecture

```
ccsm CLI
├── src/main.rs          CLI dispatch (clap), all subcommand handlers
├── src/registry.rs      WorkspaceRegistry — .claude/sessions.json CRUD, LockFile
├── src/sequence.rs      SeqOp — batch mutations in a single lock/save cycle
└── src/session.rs       Session — reads ~/.claude/sessions/<pid>.json
```

ccsm manages a per-workspace session registry at `.claude/sessions.json`. Agents use CLI subcommands to query and mutate entries. The `ccsm resume` command spawns `claude` (with `--resume` if a session_id is linked), captures the child PID, and harvests the session_id from the session file on exit.

Mutations use advisory `flock` via `fs2` on `.claude/sessions.json.lock` — every read-modify-write cycle holds an exclusive lock from read through write, preventing races when commands are chained with `&&`. The `sequence` subcommand batches multiple mutations under a single lock.

## Tech Stack

- **Rust**
- **clap** (derive) — CLI argument parsing, auto-generated --help
- **serde/serde_json** — parse Claude Code's JSON data files
- **fs2** — cross-platform `flock` advisory file locking

## Data Sources

### Files on Disk

| Path | Contains | Use For |
|------|----------|---------|
| `<workspace>/.claude/sessions.json` | Registry entries: name, goal, scope, status, session_id, pids, tags, timestamps | All CLI operations |
| `~/.claude/sessions/<pid>.json` | Live sessions: sessionId, cwd, status, name | `refresh_from_live` harvesting |
| `~/.claude/projects/<slug>/<session_id>.jsonl` | Full transcript | Resume check (exists → --resume) |
| `~/.claude/sessions.json` | Global overview across workspaces | Global Registry (Tier 1) |

> **Decision: `<workspace>/.claude/sessions.json` is the canonical session data source.** ccsm reads and writes this file via purpose-built CLI commands. No manual JSON editing needed — the CLI validates input and enforces schema integrity.

## What We Know About Claude Code's Data Surface

### Session files (`~/.claude/sessions/<pid>.json`)

```json
{
  "pid": 727940,
  "sessionId": "f493397b-456a-426d-92e1-4d5f15da0311",
  "cwd": "/home/user/project",
  "name": "my-session",
  "status": "busy",
  "startedAt": 1718400000000,
  "updatedAt": 1718400300000
}
```

### Session JSONL transcript

Append-only JSONL at `~/.claude/projects/<slug>/<session_id>.jsonl`. Contains:
- `type: "assistant"` with `message.content[]` blocks (text, tool_use with name+input)
- `type: "user"` with tool_result blocks
- `type: "system"`, `type: "file-history-snapshot"`, `type: "mode"`
- Parent UUID chain for branching/resume

### Project slug convention

Claude Code derives the project directory slug from the absolute path by replacing ALL non-alphanumeric chars with `-`. `/home/user/my_project` → `-home-user-my-project`. Transcripts live at `~/.claude/projects/<slug>/<session_id>.jsonl`.

## Design Decisions

1. **CLI-first.** Purpose-built subcommands for every operation. Same output format across Claude, Codex, Gemini, and shell scripts.
2. **`<workspace>/.claude/sessions.json` is the canonical source.** Structured JSON, diffable in git, parseable by any tool.
3. **Never parse JSONL transcripts.** Use `claude --resume` for session replay — let Claude handle its own data format.
4. **`refresh_from_live` harvests session_ids.** After `claude` exits, the session file it wrote at `~/.claude/sessions/<pid>.json` is read to harvest the `sessionId` and save it to the registry entry.
5. **Auto-managed fields.** `session_id`, `pids`, and `started` are managed by ccsm. Agents use CLI commands, never touch these fields directly.
6. **Advisory file locking.** Every mutation acquires an exclusive `flock` on `.claude/sessions.json.lock` before reading and holds it through writing. This eliminates the read-modify-write race when commands are chained (`&&` or `sequence`).
7. **Batch with `sequence`.** The `sequence` subcommand runs multiple mutations in a single process, holding one lock and saving once — faster than chaining with `&&` and inherently race-free.

## Agent Workflow (MANDATORY)

Every agent working on ccsm MUST follow this workflow. The session registry is the team coordination board.

### On Session Start

```bash
# 1. Scan the board — who's active?
ccsm list --active

# 2. Is someone already doing my task?
ccsm show <name>   # check if a session overlaps with my work
```

| Situation | Action |
|---|---|
| **Duplicate found** | Report: "Session X already does this." Help that session or narrow scope. |
| **Depends on another session** | Note in scope: "Depends on: <name> (status: ...)" |
| **No overlap** | Create new entry and detail file |

### If starting new work

```bash
ccsm new <name> -g "One-sentence goal"
ccsm start <name>
cp .claude/session-detail-template.md .claude/sessions/<name>.md
# Edit the detail file — fill in scope, tags, dependencies
ccsm scope <name> "2-4 sentence approach and constraints"
ccsm tag <name> tag1 tag2
```

### During work

```bash
# Append progress notes to .claude/sessions/<name>.md
# Update dependencies if they change
```

### On completion

```bash
ccsm complete <name>
# Update .claude/sessions/<name>.md — final status, summary, completed date
```

### Rules

- **NEVER** edit `.claude/sessions.json` directly — use CLI commands
- **NEVER** touch `session_id`, `pids`, or `started` — ccsm manages those
- **ALWAYS** create a detail file for new sessions
- **ALWAYS** scan `ccsm list --active` before starting new work
- **NEVER** execute work outside the current session's scope. If a task doesn't advance the session's `goal`, stop and tell the user. Open a new session or explicitly `ccsm scope` the current one BEFORE doing off-scope work.

## CLI Commands

### Query (token-efficient, agent-optimized)

```
ccsm list              (ls, sessions, s)  # all sessions, one line each
ccsm list --active     (-a)               # in_progress + blocked only
ccsm list --summary    (-s)               # counts: 2 active | 5 completed | 3 total
ccsm list --status X   (-S)               # filter by status
ccsm show <name>                          # full detail — goal, scope, tags, pids, timestamps
ccsm show <name> --section <s>            # extract one section from detail file
```

### Mutate (never edit JSON directly)

```
ccsm new       <name> -g <goal>  # create pending entry
ccsm start     <name>            # → in_progress
ccsm complete  <name>            # → completed + timestamp
ccsm block     <name>            # → blocked
ccsm abandon   <name>            # → abandoned
ccsm pending   <name>            # → pending + clear identity fields
ccsm scope     <name> <text>     # set scope
ccsm tag       <name> <tags...>  # replace tags
ccsm attach    <name> <sid>      # link session_id
ccsm resume    <name>            # spawn claude (--resume if session_id exists)
```

### Batch (single lock/save cycle)

```
ccsm sequence -q new <name> -q start <name> -q scope <name> <text> -q complete <name>
```

Each `-q` starts an operation group. All mutations run in-memory under one lock, saved once.

### Meta

```
ccsm setup      # one-time: install session tracking globally
ccsm --version  # print version
ccsm --help     # full command list with descriptions
```

## Build & Run

```bash
cargo build --release        # Optimized build (symlink at ~/.local/bin/ccsm auto-updates)
ccsm --help                # Show all commands
ccsm list                  # List sessions
ccsm new my-session -g "goal here"
ccsm resume my-session     # Spawn claude
```

## Related Resources

- [Claude Code Hooks Reference](https://code.claude.com/docs/en/hooks) — 30 hook events
- [Claude Code .claude Directory Guide](https://code.claude.com/docs/en/claude-directory) — File layout
