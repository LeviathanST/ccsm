# cc-tui

CLI session registry and lifecycle manager for Claude Code. Tracks sessions in `.claude/sessions.json`, links transcripts, spawns `claude` with `--resume` and `--name`.

## Install

```bash
cargo build --release
ln -s $(pwd)/target/release/cc-tui ~/.local/bin/cc-tui
cc-tui setup
```

## Commands

### Query

| Command | |
|---|---|
| `cc-tui list` | All sessions, one line each |
| `cc-tui list --active` | in_progress + blocked |
| `cc-tui list --summary` | Counts per status |
| `cc-tui list --status <s>` | Filter by status. Pass `help` for legend |
| `cc-tui show <name>` | Registry fields + detail file section headlines |
| `cc-tui show <name> --section <s>` | Extract one section from the detail file |

### Mutate

| Command | |
|---|---|
| `cc-tui new <name> -g <goal>` | → pending |
| `cc-tui start <name>` | pending → in_progress |
| `cc-tui complete <name>` | → completed |
| `cc-tui block <name>` | → blocked |
| `cc-tui abandon <name>` | → abandoned |
| `cc-tui pending <name>` | Reset to pending |
| `cc-tui scope <name> <text>` | Set scope |
| `cc-tui tag <name> <tags...>` | Replace tags |
| `cc-tui attach <name> <sid>` | Link a session_id |
| `cc-tui sequence -q <cmd> <args...> -q <cmd> ...` | Batch mutations in a single lock/save |

### Lifecycle

| Command | |
|---|---|
| `cc-tui resume <name>` | Spawn claude. --resume if session_id set, -n <name> |
| `cc-tui trash <name>` | Soft-delete (recoverable) |
| `cc-tui recover <name>` | trashed → in_progress |
| `cc-tui clean <name>` | Permanent delete. Irreversible |
| `cc-tui clean-all` | Delete ALL trashed. Irreversible |

### Statuses

```
pending      — planned, not started yet
in_progress  — actively working on (max 1 per workspace)
completed    — finished successfully
blocked      — waiting on a dependency
abandoned    — no longer relevant
trashed      — soft-deleted, recoverable
```

## Session Detail Files

Sessions have markdown detail files at `.claude/sessions/<name>.md`. Copy the template:

```bash
cp .claude/session-detail-template.md .claude/sessions/<name>.md
```

Token-efficient reading: `cc-tui show <name>` lists section headlines with line counts. Use `--section <name>` to pull just one section.

## How Resume Works

`cc-tui resume <name>` spawns `claude -n <name>` (with `--resume <id>` if session_id is set). It polls `~/.claude/sessions/<pid>.json` at startup and harvests the session_id before Claude exits — Claude v2.1+ deletes the session file on graceful exit, so harvesting happens while the process is alive.

## Data Sources

| Path | Use |
|------|-----|
| `<workspace>/.claude/sessions.json` | Canonical session registry |
| `~/.claude/sessions/<pid>.json` | Live session status (harvested on spawn) |
| `~/.claude/projects/<slug>/<id>.jsonl` | Session transcripts |
| `<workspace>/.claude/sessions/<name>.md` | Session detail files |

## Agent Integration

Agents use the `/session-manager` skill (installed by `cc-tui setup`). It enforces session tracking protocol: create entries, update status, maintain detail files.

## Sequence (Batch Mutations)

Run multiple mutations in a single process, single lock, single save:

```bash
cc-tui sequence -q new foo -q start foo -q scope foo "multi word" -q tag foo a b -q complete foo
```

Each `-q` starts an operation group. Faster than `&&` chaining — one JSON parse, one file write, no race window. Supports: `start`, `complete`, `block`, `abandon`, `pending`, `scope`, `tag`, `new`, `trash`, `recover`, `attach`.

## File Locking

Mutations use advisory `flock` on `.claude/sessions.json.lock` — every read-modify-write cycle is atomic across processes. Safe to chain commands with `&&` or run `sequence` alongside standalone mutations.

## Tech

Rust + clap + serde_json + fs2. Reads Claude Code's native session files — no PTY parsing, no transcript parsing.
