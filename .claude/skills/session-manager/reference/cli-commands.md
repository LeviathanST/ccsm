# CLI Commands

## Query

| Command | Output |
|---|---|
| `ccsm list` | All sessions, one line each (goal truncated at 80 chars) |
| `ccsm list --active` | in_progress + blocked only |
| `ccsm list --active --verbose` | Teammate scan: full goal + tags per session (~80 tokens) |
| `ccsm list --summary` | Counts per status |
| `ccsm list --status <s>` | Filter by status. Pass "help" to see what each status means |
| `ccsm show <name>` | Registry fields + detail file section headlines (with line counts) |
| `ccsm show <name> --section <s>` | Extract one section from the detail file |
| `ccsm --help` | Full command list |

## Mutate

| Command | Transition |
|---|---|
| `ccsm new <name> -g <goal>` | → pending |
| `ccsm start <name>` | pending → in_progress |
| `ccsm complete <name> [--force]` | in_progress → completed, sets timestamp. Runs gate checks first (use --force to bypass) |
| `ccsm block <name>` | in_progress → blocked (waiting on dependency) |
| `ccsm abandon <name>` | in_progress → abandoned (no longer relevant) |
| `ccsm pending <name>` | → pending, clears session_id + pids + timestamps |
| `ccsm scope <name> <text>` | Set scope field |
| `ccsm tag <name> <tags...>` | Replace tags |
| `ccsm attach <name>` | Auto-discover & link live session. `--pid <pid>` or `<uuid>` for explicit |
| `ccsm resume <name>` | Spawn claude. --resume if session_id set, -n <name>, harvests session_id on exit |
| `ccsm refresh <name> [-r]` | Retire current Claude session to retired_session_ids, spawn fresh (no --resume) |
| `ccsm rename <old> <new> [-g] [-s]` | Rename session across registry, detail file, live sessions, transcript |
| `ccsm close <name>` | Pre-completion gate: hard checks + self-review checklist. Exit non-zero if hollow |
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
| `ccsm group <session> -g <group> [-r free\|<n>]` | Assign session to group (auto-creates `.claude/session-group/<group>.md`) |
| `ccsm group <session> --clear` | Remove from group (auto-deletes group file when last leaves) |
| `ccsm group <name>` | Overview — list members sorted by rank, show group goal |
| `ccsm group <name> --goal <text>` | Set group goal in `.claude/session-group/<name>.md` |
| `ccsm group <name> --roadmap` | Live markdown roadmap → stdout: table (rank/status/goal/scope) + Mermaid dep graph |
| `ccsm group --list` | List all groups in workspace with member counts + status breakdown |
| `ccsm next <group>` | Next unblocked session to work on (respects depends_on) |
| `ccsm group-deps <group>` | ASCII dependency tree with status markers (✓→○!) |
| `ccsm depend <name> --on <dep>` | Add dependency (both sessions must be in same group) |
| `ccsm depend <name> --clear` | Clear all dependencies |
| `ccsm depend <name>` | List dependencies with status |
