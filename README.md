# ccsm — Session Registry & Lifecycle Manager

ccsm is a CLI session registry and lifecycle manager for AI coding agents. It tracks sessions in `.ccsm/sessions.json`, links transcripts, and spawns agents (`claude` or `pi`) with resume support.

---

## For Agents: Start Here

### Decision Tree

```
I need to register my session              → ccsm new <name> -g "<goal>"
I need to start working                     → ccsm start <name>
I need to log progress                      → ccsm note <name> "<what I did>"
I need to see who's working on what         → ccsm list --active
I need a compact overview                   → ccsm scan
I need the full picture of a session        → ccsm show <name>
I need only one section (cheap)             → ccsm show <name> --section <name>
I need text-search across sessions          → ccsm scan --search <query>
I need to check session health              → ccsm doctor
I need to know what to do next              → ccsm next <group>
I need to batch multiple mutations          → ccsm sequence -q <cmd> -q <cmd> ...
I finished — gate check                     → ccsm close <name>
I finished — mark done                      → ccsm complete <name>
I need to archive completed work            → ccsm archive <name>
```

### Token Budget

Every command costs context window tokens. Prefer the cheapest variant that answers your question.

| Command | ✅ Best For | ~Output Tokens | 💡 Tip |
|---|---|---|---|
| `ccsm list --summary` | Quick count | ~30 | Start here — 0 context risk |
| `ccsm list --active` | Who's working | ~40 + ~15/session | Fast team scan |
| `ccsm scan` | Full overview | ~50 + ~25/session | Grouped, grep-friendly |
| `ccsm scan --search <q>` | Find specific session | ~60 + results | No grep needed |
| `ccsm scan --json` | Programmatic | ~100 + ~30/session | Script piping |
| `ccsm show <name> --section <s>` | One detail field | ~50 + section | Cheapest way to get a field |
| `ccsm show <name>` | Full detail | ~150 + detail file | Creates detail file? No, just reads |
| `ccsm doctor` | Health scan | ~200 | Run before cleanup sessions |
| `ccsm next <group>` | Next work item | ~60 | Group workflow |
| `ccsm group <name> --roadmap` | Group roadmap | ~200 | Mermaid dep graph |
| `ccsm new <name> -g "<goal>"` | Register | ~120 | Creates entry + detail file |
| `ccsm note <name> <text>` | Log progress | ~80 | Append-only |
| `ccsm sequence -q cmd ...` | Batch mutations | ~120 + per op | Single lock/save |
| `ccsm close <name>` | Gate check | ~100 | Exit non-zero on failures |
| `ccsm check <name> <item> -s <s>` | Checklist | ~60 | Auto-creates section |
| `ccsm resume <name>` | Spawn agent | ~50 (spawn overhead) | Harvests session_id on exit |
| `ccsm archive <name>` | Archive | ~120 | Delete transcripts, keep entry |

**Rule of thumb:** Use `--section` to read one field instead of the whole detail file. Use `scan` instead of `list` for compact output. Use `--summary` for just counts.

---

## Commands

### Query

| Command | Output |
|---|---|
| `ccsm list` | All sessions, one line each |
| `ccsm list --active` | in_progress + blocked only |
| `ccsm list --summary` | Counts per status |
| `ccsm list --status <s>` | Filter by status. Pass `help` for legend |
| `ccsm scan` | Compact grouped output, grep-friendly |
| `ccsm scan --search <q>` | Full-text across name+goal+tags |
| `ccsm scan --group <g>` | Filter by group |
| `ccsm scan --json` | Structured JSON for programmatic use |
| `ccsm show <name>` | Registry fields + detail file section headlines |
| `ccsm show <name> --section <s>` | Extract one section from the detail file |

### Mutate

| Command | Transition |
|---|---|
| `ccsm new <name> -g <goal>` | → pending |
| `ccsm start <name>` | pending → in_progress |
| `ccsm complete <name> [--force]` | → completed + timestamp |
| `ccsm block <name>` | → blocked |
| `ccsm abandon <name>` | → abandoned |
| `ccsm pending <name>` | → pending, clears session_id + pids |
| `ccsm scope <name> <text>` | Set scope field |
| `ccsm tag <name> <tags...>` | Replace tags |
| `ccsm rename <old> <new>` | Rename across registry + detail file + transcript |
| `ccsm attach <name>` | Link live session UUID to entry. `--pid <pid>` or `<uuid>` |
| `ccsm note <name> <text>` | Append to progress log |
| `ccsm note <name> --cross <src> <text>` | Cross-session note |
| `ccsm check <name> <item> -s <status>` | Set checklist item |
| `ccsm close <name>` | Pre-completion gate (hard checks + self-review) |
| `ccsm sequence -q <cmd> ...` | Batch mutations in single lock/save |
| `ccsm completions <shell>` | Generate shell completions (bash/fish/zsh) |

### Lifecycle

| Command | Effect |
|---|---|
| `ccsm resume <name>` | Spawn agent with --resume/--session |
| `ccsm refresh <name> [-r why]` | Retire session, spawn fresh |
| `ccsm trash <name>` | Soft-delete (recoverable) |
| `ccsm recover <name>` | trashed → in_progress |
| `ccsm clean <name>` | Permanent delete — irreversible |
| `ccsm clean-all` | Delete ALL trashed — irreversible |

### Groups & Dependencies

| Command | Effect |
|---|---|
| `ccsm group <session> -g <g> [-r free\|<n>]` | Assign to group |
| `ccsm group <session> --clear` | Remove from group |
| `ccsm group <name>` | Overview of group members |
| `ccsm group <name> --roadmap` | Render roadmap markdown (table + Mermaid) |
| `ccsm group --list` | List all groups in workspace |
| `ccsm next <group>` | Next unblocked session to work on |
| `ccsm group-deps <group>` | ASCII dependency tree |
| `ccsm depend <name> --on <dep>` | Add dependency |
| `ccsm depend <name> --clear` | Clear dependencies |

### Maintenance

| Command | Effect |
|---|---|
| `ccsm doctor` | Health scan (orphaned IDs, dead PIDs, template residue, stale locks) |
| `ccsm doctor --fix` | Auto-resolve common issues |
| `ccsm archive <name>` | Delete transcript + session files, keep registry entry |
| `ccsm archive-all` | Archive all completed sessions with transcripts |
| `ccsm note-check` | Stop-hook: warn if dirty tree + stale detail file |
| `ccsm inject-scope` | Output `<system-reminder>` block for system prompt injection |
| `ccsm setup` | Install session tracking into global CLAUDE.md + skills |

### Statuses

```
pending      — planned, not started yet
in_progress  — actively working on (max 1 per workspace)
completed    — finished successfully
blocked      — waiting on a dependency
abandoned    — no longer relevant
trashed      — soft-deleted, recoverable
```

---

## Consumer Model

ccsm supports multiple AI coding agents via the `--consumer` flag:

| Consumer | Flag | Binary | Sessions Dir |
|---|---|---|---|
| Claude Code (default) | `--consumer claude` | `claude` | `~/.claude/sessions/<pid>.json` |
| Pi | `--consumer pi` | `pi` | `~/.pi/agent/sessions/<slug>/<ts>_<uuid>.jsonl` |

Detection order: `--consumer` flag → `CCSM_CONSUMER` env var → auto-detect (most recently active config dir wins).

| Feature | Claude | Pi |
|---|---|---|
| `resume` spawns | `claude --resume <uuid> -n <name>` | `pi --session <uuid> -n <name>` |
| `refresh` spawns | `claude -n <name>` (fresh) | `pi --continue -n <name>` |
| `attach` auto-discovers | Reads live `~/.claude/sessions/<pid>.json` | Scans `~/.pi/agent/sessions/<slug>/` by mtime |
| Session harvesting | PID-based JSON polling | UUID known from `--session` flag |

---

## Install (for Humans)

```bash
cargo build --release
cp target/release/ccsm ~/.local/bin/ccsm
ccsm setup
```

Prerequisites: Rust toolchain. Build times are ~30s on a modern machine.

---

## Workspace Resolution

| Priority | Source | Example |
|----------|--------|---------|
| 1 | `--workspace` flag | `ccsm -w /path/to/project list` |
| 2 | `CCSM_WORKSPACE` env var | `CCSM_WORKSPACE=/path/to/project ccsm list` |
| 3 | Walk-up from PWD | Finds innermost `.ccsm/sessions.json` in parent chain |
| 4 | PWD as-is | Current directory (fallback) |

---

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

- **Rust + clap** (derive) — CLI argument parsing
- **serde/serde_json** — JSON data handling
- **fs2** — cross-platform `flock` advisory file locking

### Design Decisions

1. **CLI-first.** Purpose-built subcommands for every operation. Same output format across agents and shell scripts.
2. **`<workspace>/.ccsm/sessions.json` is canonical.** Structured JSON, diffable, parseable by any tool.
3. **Never parse JSONL transcripts.** Let the agent handle its own data format.
4. **Consumer abstraction.** `src/consumer.rs` encapsulates all agent-specific paths. New consumer = one enum variant.
5. **Pi extension.** `.pi/extensions/ccsm/index.ts` registers all ccsm operations as native Pi tools (20+).
6. **Auto-managed fields.** `session_id`, `pids`, `started` — set by ccsm, never touched by agents.
7. **Advisory file locking.** Every mutation acquires exclusive `flock` on `.ccsm/sessions.json.lock` before read, holds through write.
8. **Batch with `sequence`.** Multiple mutations under one lock and one save — faster than `&&` chaining.
9. **Keyword-rich goals.** Self-contained and searchable. Bad: `"Fix bugs"`. Good: `"Fix PTY spawn race in ccsm resume"`. `ccsm doctor` flags vague goals.

---

## Data Sources

| Path | Contains | Use |
|------|----------|-----|
| `<workspace>/.ccsm/sessions.json` | Registry entries | All CLI operations |
| `<workspace>/.ccsm/sessions/<name>.md` | Session detail files | Notes, checklists, dependencies |
| `~/.claude/sessions/<pid>.json` | Live Claude session ID | `resume` harvesting |
| `~/.claude/projects/<slug>/<uuid>.jsonl` | Claude transcript | Resume check (exists → --resume) |
| `~/.pi/agent/sessions/<slug>/<ts>_<uuid>.jsonl` | Pi session files | `resume`, `attach` |

---

## Related

- [Claude Code Hooks Reference](https://code.claude.com/docs/en/hooks)
- [Claude Code .claude Directory Guide](https://code.claude.com/docs/en/claude-directory)
- [Pi Extension Docs](https://pi.dev/docs/extensions)
