# ccsm: Session Registry CLI

## Identity

You are Vex, building a CLI session registry and lifecycle manager for AI coding agents. ccsm supports **Claude Code** and **Pi** — abstracted behind the `Consumer` enum.

## Core Principle

**Augment, don't rebuild.** The agent's harness (agent loop, tools, hooks, permissions, compaction, sessions, slash commands, skills) runs untouched. ccsm adds structured session tracking with a CLI and JSON registry — nothing is reimplemented, every agent update is free.

## Architecture

```
ccsm CLI
├── src/main.rs          CLI dispatch (clap), all subcommand handlers
├── src/consumer.rs      Consumer enum — Claude, Pi; path/binary abstraction
├── src/registry.rs      WorkspaceRegistry — .ccsm/sessions.json CRUD, LockFile
├── src/sequence.rs      SeqOp — batch mutations in a single lock/save cycle
├── src/session.rs       Session — reads agent session files (Claude PID format)
└── src/commands/
    ├── resume.rs        Spawn agent (claude or pi) with resume/fresh
    └── doctor.rs        Health scan
```

ccsm manages a per-workspace session registry at `.ccsm/sessions.json`. Agents use CLI subcommands to query and mutate entries. The `ccsm resume` command spawns the agent (`claude` or `pi`) and harvests the session_id on exit.

The `Consumer` enum (`src/consumer.rs`) abstracts agent-specific paths, binary names, and session file formats. Auto-detects from `--consumer` flag, `CCSM_CONSUMER` env var, or most recently active config directory.

Mutations use advisory `flock` via `fs2` on `.ccsm/sessions.json.lock` — every read-modify-write cycle holds an exclusive lock from read through write, preventing races when commands are chained with `&&`. The `sequence` subcommand batches multiple mutations under a single lock.

## Tech Stack

- **Rust**
- **clap** (derive) — CLI argument parsing, auto-generated --help
- **serde/serde_json** — parse agent JSON data files
- **fs2** — cross-platform `flock` advisory file locking

## Data Sources

### Files on Disk

| Path | Contains | Use For |
|------|----------|---------|
| `<workspace>/.ccsm/sessions.json` | Registry entries: name, goal, scope, status, session_id, pids, tags, timestamps | All CLI operations |
| `~/.claude/sessions/<pid>.json` | Live Claude session: sessionId, cwd, status, name | `resume` harvesting |
| `~/.claude/projects/<slug>/<session_id>.jsonl` | Claude transcript | Resume check (exists → --resume) |
| `~/.pi/agent/sessions/<slug>/<ts>_<uuid>.jsonl` | Pi session files | `resume` (--session), `attach` auto-discover |
| `~/.codewhale/sessions/<uuid>.json` | CodeWhale saved session | `resume` (--resume), `attach` (recent session scan) |
| `~/.codewhale/state.db` | CodeWhale SQLite state DB (threads table) | `thread list` for durable sessions |

> **Decision: `<workspace>/.ccsm/sessions.json` is the canonical session data source.** ccsm reads and writes this file via purpose-built CLI commands. No manual JSON editing needed — the CLI validates input and enforces schema integrity.

## Consumer Detection

ccsm supports multiple agents (consumers), auto-detected or explicitly set:

| Method | Example |
|--------|---------|
| **Flag** | `ccsm --consumer codewhale resume <name>` |
| **Env var** | `CCSM_CONSUMER=codewhale ccsm resume <name>` |
| **Auto-detect** | `ccsm <command>` — picks the most recently active config dir (`~/.pi/agent/`, `~/.claude/`, or `~/.codewhale/`) |

| Consumer | Binary | Config Dir | Session Files |
|----------|--------|------------|---------------|
| `claude` | `claude` | `~/.claude/` | `~/.claude/sessions/<pid>.json` + `~/.claude/projects/<slug>/<uuid>.jsonl` |
| `pi` | `pi` | `~/.pi/agent/` | `~/.pi/agent/sessions/<slug>/<ts>_<uuid>.jsonl` |
| `codewhale` | `codewhale` | `~/.codewhale/` | `~/.codewhale/sessions/<uuid>.json` + `~/.codewhale/state.db` (SQLite)

### What changes per consumer

| Feature | Claude | Pi | CodeWhale |
|---------|--------|----|-----------|
| `resume` spawns | `claude --resume <uuid> -n <name>` | `pi --session <uuid> -n <name>` | `codewhale --resume <uuid>` |
| `refresh` spawns | `claude -n <name>` (fresh) | `pi --continue -n <name>` | `codewhale --skip-onboarding` |
| `attach` auto-discovers | **Claude:** reads live PID-based session file `~/.claude/sessions/<pid>.json` for exact UUID | **Pi:** scans `~/.pi/agent/sessions/<slug>/` for most recently modified `.jsonl` (Pi has no live PID files, so mtime is used as best approximation) | **CodeWhale:** scans `~/.codewhale/sessions/*.json` for most recent session matching the workspace git root |
| `inject-scope` format | `<system-reminder>...</system-reminder>` | Same (both agents accept it) | `--append-system-prompt "<system-reminder>...</system-reminder>"` (or write `.codewhale/constitution.json` for standing scope) |
| Session harvesting | PID-based JSON polling | UUID already known from `--session` flag | No PID files; harvest UUID from session filename stem or `thread list` output |

## Design Decisions

1. **CLI-first.** Purpose-built subcommands for every operation. Same output format across Claude, Pi, and shell scripts.
2. **`<workspace>/.ccsm/sessions.json` is the canonical source.** Structured JSON, diffable in git, parseable by any tool.
3. **Never parse JSONL transcripts.** Use `claude --resume` or `pi --session` for session replay — let the agent handle its own data format.
4. **Consumer abstraction.** `src/consumer.rs` encapsulates all agent-specific paths and binary names. Adding a new consumer means adding one enum variant.
5. **Pi extension.** `.pi/extensions/ccsm/index.ts` registers all ccsm operations as native Pi tools (20+ tools). The extension always passes `--consumer pi`.
6. **Auto-managed fields.** `session_id`, `pids`, and `started` are managed by ccsm. Agents use CLI commands, never touch these fields directly.
7. **Advisory file locking.** Every mutation acquires an exclusive `flock` on `.ccsm/sessions.json.lock` before reading and holds it through writing. This eliminates the read-modify-write race when commands are chained (`&&` or `sequence`).
8. **Batch with `sequence`.** The `sequence` subcommand runs multiple mutations in a single process, holding one lock and saving once — faster than chaining with `&&` and inherently race-free.
9. **Keyword-rich goals.** Session goals must be self-contained and searchable. Bad: `"Fix bugs"`. Good: `"Fix PTY spawn race condition in ccsm resume command"`. Never use the session name as the goal. `ccsm doctor` flags vague goals (< 20 chars), name-as-goal, and CLI-artifact goals (`-g ` prefix).

## Workspace Resolution

ccsm resolves the workspace root in this priority order:

| Priority | Source | Example |
|----------|--------|---------|
| 1 | `--workspace` flag | `ccsm -w /path/to/project list` |
| 2 | `CCSM_WORKSPACE` env var | `CCSM_WORKSPACE=/path/to/project ccsm list` |
| 3 | Walk-up from PWD | Finds innermost `.ccsm/sessions.json` in parent chain |
| 4 | PWD as-is | Current directory (fallback) |

**`CCSM_WORKSPACE`** must be an absolute path to an existing directory. It's the escape hatch when an agent chdir'd into a subdirectory and PWD-based detection finds the wrong marker. Set it once at session start — all subsequent `ccsm` commands inherit it.

**Walk-up** looks for `.ccsm/sessions.json` starting at PWD and going up. Innermost match wins (closest to PWD). This handles the common case of an agent being in `src/deep/path/` when the workspace is the project root — no configuration needed.

> **Anti-pattern:** Agents running `ccsm` commands from wrong PWD create dangling `.ccsm/` directories. If you see duplicate registries, set `CCSM_WORKSPACE` at the point of failure and confirm the path.

**Loud failure on bad env var:** If `CCSM_WORKSPACE` points to a non-existent or non-absolute path, ccsm errors immediately (no silent fallback).

## CLI Commands

### Query (token-efficient, agent-optimized)

```
ccsm list              (ls, sessions, s)  # all sessions, one line each
ccsm list --active     (-a)               # in_progress + blocked only
ccsm list --summary    (-s)               # counts
ccsm list --status X   (-S)               # filter by status
ccsm scan              (sc)               # compact grouped output, grep-friendly
ccsm scan --search <q>                    # full-text across name+goal+tags
ccsm scan --json                          # structured JSON for programmatic use
ccsm show <name>                          # full detail
ccsm show <name> --section <s>            # extract one section from detail file
```

### Mutate (never edit JSON directly)

```
ccsm new       <name> -g <goal>            # create pending entry
ccsm start     <name>                      # → in_progress
ccsm complete  <name> [--force]            # → completed + timestamp
ccsm block     <name>                      # → blocked
ccsm abandon   <name>                      # → abandoned
ccsm pending   <name>                      # → pending + clear identity fields
ccsm scope     <name> <text>               # set scope
ccsm tag       <name> <tags...>            # replace tags
ccsm rename    <old> <new>                 # rename session
ccsm attach    <name>                      # auto-discover & link live session
ccsm resume    <name>                      # spawn agent (--resume/--session)
ccsm refresh   <name> [-r why]             # retire session, spawn fresh
ccsm close     <name>                      # pre-completion gate
ccsm note      <name> <text>               # append to progress log
ccsm check     <name> <item> -s <status>   # checklist item
ccsm group     <session> -g <g> [-r <r>]   # assign to group
ccsm group     <name> --roadmap             # render roadmap markdown
ccsm next      <group>                     # next session to work on
ccsm depend    <name> --on <dep>           # add dependency
ccsm doctor                                # scan for health issues
ccsm archive   <name>                      # delete transcript, keep entry
```

### Pi Extension (auto-discovered)

When Pi runs in this workspace (`.pi/extensions/ccsm/`), it automatically gets 20+ native tools:

| Pi Tool | Maps To |
|---------|---------|
| `ccsm_list` | `ccsm --consumer pi list` |
| `ccsm_scan` | `ccsm --consumer pi scan` |
| `ccsm_new` | `ccsm --consumer pi new` |
| `ccsm_start` | `ccsm --consumer pi start` |
| `ccsm_complete` | `ccsm --consumer pi complete` |
| `ccsm_note` | `ccsm --consumer pi note` |
| `ccsm_scope` | `ccsm --consumer pi scope` |
| `ccsm_inject_scope` | `ccsm --consumer pi inject-scope` |
| `ccsm_resume` | `ccsm --consumer pi resume` |
| ... and more | all with `--consumer pi` |

The extension also hooks `before_agent_start` to auto-inject the active session's goal and scope into Pi's system prompt.

## Documentation Discipline

Every new CLI feature (command, flag, field, workflow) **must** be documented in the skill reference files before the session is closed. Agents discover capabilities through these docs — stale docs mean agents don't know features exist.

- `.claude/skills/session-manager/reference/cli-commands.md` — new commands/flags
- `.claude/skills/session-manager/reference/registry-schema.md` — new fields, schema changes
- `.claude/skills/session-manager/SKILL.md` — new workflows or protocols
- `CLAUDE.md` — project-level architecture changes only (not agent instructions)
- `docs/adding-a-consumer.md` — checklist for adding a new AI coding agent consumer

Run `ls .claude/skills/session-manager/reference/` to discover all skill reference docs.
Verify with `grep` that every new term appears. Commit docs with code in the same push.

## Build & Run

```bash
cargo build --release              # Optimized build
cp target/release/ccsm ~/.local/bin/ccsm  # Install
ccsm list                          # List sessions
ccsm --consumer pi list --summary  # List with Pi consumer
CCSM_CONSUMER=pi ccsm resume <name>  # Resume with Pi
```

## Related Resources

- [Claude Code Hooks Reference](https://code.claude.com/docs/en/hooks) — 30 hook events
- [Claude Code .claude Directory Guide](https://code.claude.com/docs/en/claude-directory) — File layout
- [Pi Extension Docs](https://pi.dev/docs/extensions) — Custom tools, events, UI
