# CLI Commands

## Workspace Resolution

ccsm finds the workspace root in this order:

1. `--workspace` / `-w` flag
2. `CCSM_WORKSPACE` env var (absolute path to existing dir — set at session start for safety)
3. Walk-up from PWD looking for `.ccsm/sessions.json` (innermost match wins)
4. PWD as-is (fallback)

`CCSM_WORKSPACE` is the escape hatch for wrong-PWD bugs. Agents that chdir between commands set this env var at spawn time.

## Query

| Command | Output |
|---|---|
| `ccsm list` | All sessions, one line each (goal truncated at 80 chars) |
| `ccsm list --active` | in_progress + blocked only |
| `ccsm list --active --verbose` | Teammate scan: full goal + tags per session (~80 tokens) |
| `ccsm list --summary` | Counts per status |
| `ccsm list --status <s>` | Filter by status. Pass "help" to see what each status means |
| `ccsm scan` | Compact grouped output (agents: ~50 tokens/entry). Grep-friendly field markers |
| `ccsm scan --group <g>` | Filter by group |
| `ccsm scan --status <s>` | Filter by status |
| `ccsm scan --search <q>` | Full-text across name, goal, tags (case-insensitive) — no grep needed |
| `ccsm scan --json` | Structured JSON array for programmatic consumers |
| `ccsm show <name>` | Registry fields + detail file section headlines (with line counts) |
| `ccsm show <name> --section <s>` | Extract one section from the detail file |
| `ccsm --help` | Full command list |

## Mutate

| Command | Transition |
|---|---|
| `ccsm new <name> -g <goal> [-b <branch>] [-c [<type>]]` | → pending. `-b <branch>` sets target git branch for branch-session tracking; `-c` adds ## Checklist section (empty or pre-populated from template type: feat/fix/research/chore). Config in `.ccsm/config.toml` enforces branch requirement and WIP limit |
| `ccsm start <name>` | pending → in_progress |
| `ccsm complete <name> [--force]` | in_progress → completed, sets timestamp. Runs gate checks first (use --force to bypass) |
| `ccsm block <name>` | in_progress → blocked (waiting on dependency) |
| `ccsm abandon <name>` | in_progress → abandoned (no longer relevant) |
| `ccsm pending <name>` | → pending, clears session_id + pids + timestamps |
| `ccsm scope <name> <text>` | Set scope field |
| `ccsm tag <name> <tags...>` | Replace tags |
| `ccsm attach <name>` | Link a session UUID to a ccsm entry. Three modes: **(1)** `ccsm attach <name>` — auto-discover (Claude: reads live `~/.claude/sessions/<pid>.json`; Pi: picks most recently modified `.jsonl` in `~/.pi/agent/sessions/<slug>/`). **(2)** `ccsm attach <name> <uuid>` — explicit UUID. **(3)** `ccsm attach <name> --pid <pid>` — harvest from PID (Claude only) |
| `ccsm resume <name>` | Spawn agent (claude or pi). `--resume <uuid>` for Claude, `--session <uuid>` for Pi. Harvests session_id on exit |
| `ccsm refresh <name> [-r]` | Retire current session to retired_session_ids, spawn fresh (no --resume). Spawns `pi --continue` when consumer is Pi |
| `ccsm rename <old> <new> [-g] [-s]` | Rename session across registry, detail file, live sessions, transcript |
| `ccsm close <name>` | Pre-completion gate: hard checks + self-review checklist. Exit non-zero if hollow. Blocks if pending/blocked checklist items exist |
| `ccsm checklist <name>` | List checklist items from detail file. `--init` adds ## Checklist section to existing session |
| `ccsm check <name> <item> -s <status>` | Set checklist item status, or add new item if no match. Auto-creates ## Checklist section. `<item>` can be 1-based index or text substring |
| `ccsm note <name> <text>` | Append timestamped entry to detail file Progress Log |
| `ccsm note <name> --cross <src> <text>` | Cross-session note: prepends `CROSS-SESSION [src]:` |
| `ccsm sequence -q <cmd> <args...> ...` | Batch mutations under a single lock/save. Faster than `&&` chaining |
| `ccsm completions <shell>` | Generate shell completion script to stdout. bash, fish, or zsh |
| `ccsm setup` | Install session tracking into global CLAUDE.md + skills (run once) |

## Lifecycle (trash/clean)

| Command | Transition |
|---|---|
| `ccsm trash <name>` | → trashed (soft-delete, recoverable) |
| `ccsm recover <name>` | trashed → in_progress |
| `ccsm clean <name>` | Permanent delete: transcript + session files + entry. Irreversible |
| `ccsm clean-all` | Permanent delete ALL trashed entries. Irreversible |

## Statuses

```
pending      — planned, not started yet
in_progress  — actively working on
completed    — finished successfully
blocked      — can't proceed, waiting on a dependency
abandoned    — gave up, no longer relevant
trashed      — soft-deleted, recoverable with `ccsm recover <name>`
```

## Grouping & Dependencies

| Command | Effect |
|---|---|
| `ccsm group <session> -g <group> [-r free\|<n>]` | Assign session to group (auto-creates `.ccsm/session-group/<group>.md`) |
| `ccsm group <session> --clear` | Remove from group (auto-deletes group file when last leaves) |
| `ccsm group <name>` | Overview — list members sorted by rank, show group goal |
| `ccsm group <name> --goal <text>` | Set group goal in `.ccsm/session-group/<name>.md` |
| `ccsm group <name> --roadmap` | Live markdown roadmap → stdout: table (rank/status/goal/scope) + Mermaid dep graph |
| `ccsm group --list` | List all groups in workspace with member counts + status breakdown |
| `ccsm next <group>` | Next unblocked session to work on (respects depends_on) |
| `ccsm group-deps <group>` | ASCII dependency tree with status markers (✓→○!) |
| `ccsm depend <name> --on <dep>` | Add dependency (both sessions must be in same group) |
| `ccsm depend <name> --clear` | Clear all dependencies |
| `ccsm depend <name>` | List dependencies with status |

## Consumer (Target Agent)

ccsm supports multiple AI coding agents via the `--consumer` flag:

| Consumer | Flag | Binary | Sessions Dir |
|----------|------|--------|-------------|
| Claude Code (default) | `--consumer claude` | `claude` | `~/.claude/sessions/<pid>.json` |
| Pi | `--consumer pi` | `pi` | `~/.pi/agent/sessions/<slug>/<ts>_<uuid>.jsonl` |

Detection order: `--consumer` flag → `CCSM_CONSUMER` env var → auto-detect.

### What changes per consumer

| Feature | Claude | Pi |
|---------|--------|----|
| `resume` | `claude --resume <uuid> -n <name>` | `pi --session <uuid> -n <name>` |
| `refresh` | `claude -n <name>` (fresh) | `pi --continue -n <name>` |
| `attach` (auto) | Reads live `~/.claude/sessions/<pid>.json` (PID-based live session files) | Scans `~/.pi/agent/sessions/<slug>/` for most recently modified `.jsonl` (no live PID files in Pi) |
| Session harvesting | PID-based JSON polling | UUID known from `--session` flag |

## Miscellaneous

| Command | Effect |
|---|---|
| `ccsm doctor` | Scan for health issues (orphaned IDs, dead PIDs, template residue, stale locks, archive candidates) |
| `ccsm note-check` | Stop-hook: check if working tree is dirty and detail file is stale. Reminds to note progress. Silent when clean |
| `ccsm archive <name>` | Delete transcript + session files, keep registry entry as permanent work log |
| `ccsm archive-all` | Archive all completed sessions with transcripts |
| `ccsm inject-scope` | Output `<system-reminder>` block with goal + scope + checklist summary + branch check for system prompt injection. If session has a target branch (`-b`), compares it to current git branch and warns on mismatch |

## Project Configuration (.ccsm/config.toml)

ccsm reads `.ccsm/config.toml` for project-level policy enforcement. All fields are optional.

```toml
# Branch tracking: "required" | "optional" | "disabled"
branch_tracking = "optional"

# WIP guard — warn when creating with >N in_progress sessions (0 = off)
wip_limit = 0

# Default checklist type when -c is used without a value
# default_checklist_type = "feat"

# Custom checklist templates override built-in defaults
# [checklist_templates.feat]
# items = ["Plan drafted", "Tests written", "Edge cases handled"]
```

| Field | Effect |
|-------|--------|
| `branch_tracking = "required"` | `ccsm new` errors without `-b <branch>` |
| `branch_tracking = "optional"` | `-b` is accepted but not required (default) |
| `branch_tracking = "disabled"` | `-b` is accepted but never enforced |
| `wip_limit = 3` | Warning when `>N` sessions are already in_progress |
| `default_checklist_type = "feat"` | `-c` alone uses the feat template |
| `[checklist_templates.<type>]` | Override built-in checklist items for a type |

Built-in checklist types: `feat`, `fix`, `research`, `chore`.
