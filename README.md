# ccsm — Session Registry & Lifecycle Manager

ccsm is a CLI session registry and lifecycle manager for AI coding agents. It tracks sessions in `.ccsm/sessions.json`, links transcripts, and spawns agents (`opencode`, `claude`, `pi`) with resume support. It also ships **ccsm-swarm**, an MCP server for multi-agent orchestration via tmux.

ccsm is the backbone of a structured multi-agent workflow: plan → session → work → review → merge → archive. Every session is tracked, every transcript is linked, and nothing falls through the cracks.

---

## Quick Start

```bash
cargo build --release
cp target/release/ccsm ~/.local/bin/ccsm
cp target/release/ccsm-swarm ~/.local/bin/ccsm-swarm
ccsm setup
```

Prerequisites: Rust toolchain, tmux (for swarm). Build times ~30s.

---

## Architecture

```
ccsm CLI              — Session registry (new, start, resume, complete, archive)
ccsm-swarm (MCP)      — Multi-agent orchestration via tmux
    │
    ├── ccsm commands               # Session lifecycle, groups, dependencies
    ├── .ccsm/sessions.json         # Canonical registry (single source of truth)
    ├── .ccsm/sessions/<name>.md    # Per-session detail files (progress, checklists)
    └── rmcp (stdio MCP)            # Rust MCP SDK for ccsm-swarm tooling
```

### Design Principles

1. **CLI-first.** Purpose-built subcommands for every operation. Same output format across agents and shell scripts.
2. **`.ccsm/sessions.json` is canonical.** Structured JSON, diffable, parseable by any tool.
3. **Consumer abstraction.** `src/consumer.rs` encapsulates all agent-specific paths. New consumer = one enum variant.
4. **Batch with `sequence`.** Multiple mutations under one lock and one save — faster than `&&` chaining.
5. **Advisory file locking.** Exclusive `flock` on `.ccsm/sessions.json.lock` before every mutation.
6. **ccsm-swarm is an MCP server over stdio.** Single 3.8MB Rust binary, no runtime deps beyond tmux.

---

## Reference: Skills & Documentation

Most workflow documentation lives in Claude skills (`.claude/skills/`). These are the canonical reference — the tables below tell you where to look.

### Core Workflow (Humans & Agents)

| Skill / Doc | What it covers | For |
|-------------|----------------|-----|
| [AGENTS.md](AGENTS.md) | ccsm-swarm MCP tools — create swarm, label panes, inject, wait, capture | Orchestrator setup |
| [.claude/skills/session-manager/SKILL.md](.claude/skills/session-manager/SKILL.md) | Full session lifecycle — start, work, note, complete. Status rules, attach modes, team awareness | Every session |
| [docs/ccsm-swarm.md](docs/ccsm-swarm.md) | Detailed MCP tool reference — args, returns, architecture, delta tracking | Tool implementors |
| [docs/adding-a-consumer.md](docs/adding-a-consumer.md) | Adding a new agent backend (Consumer enum) | Developers |

### Agent Workflow Skills

| Skill | What it covers |
|-------|----------------|
| `seed-session` (built-in) | Create a pending session stub from a quick description |
| `wrap-up` (built-in) | End-of-session Ship + Learn checklist |
| `git-discipline` (global) | Commit discipline, branch hygiene, PR writing rules |
| `learned-lesson-issue` (global) | Debugging protocol — check prior lessons before investigating |

### CLI Command Families

| Area | Key Commands |
|------|--------------|
| **Query** | `list`, `list --active`, `list --summary`, `scan`, `show`, `show --section` |
| **Mutate** | `new`, `start`, `complete`, `block`, `abandon`, `scope`, `tag`, `note`, `check` |
| **Lifecycle** | `resume`, `refresh`, `trash`, `recover`, `clean`, `archive` |
| **Groups** | `group`, `group --roadmap`, `group-deps`, `next`, `depend` |
| **Maintenance** | `doctor`, `doctor --fix`, `archive-all`, `clean-all`, `note-check`, `setup` |

See the full command list in [.claude/skills/session-manager/reference/cli-commands.md](.claude/skills/session-manager/reference/cli-commands.md).

### ccsm-swarm MCP Tools

| Tool | Description |
|------|-------------|
| `swarm-list-panes` | List all tmux panes with session, window, process |
| `swarm-capture` | Read pane output (delta-aware — only new content) |
| `swarm-inject` | Type text into a pane |
| `swarm-wait` | Block until a sentinel string appears |
| `swarm-status` | Consolidated status of all panes |
| `swarm-broadcast` | Same text to every pane |
| `swarm-label` | Name a pane for role-based targeting |

See [AGENTS.md](AGENTS.md) for the orchestration workflow.

---

## Consumer Model

ccsm supports multiple AI coding agents via the `--consumer` flag:

| Consumer | Flag | Binary | Sessions |
|----------|------|--------|----------|
| OpenCode | `--consumer opencode` | `opencode` | SQLite at `~/.local/share/opencode/opencode.db` |
| Claude Code (default) | `--consumer claude` | `claude` | `~/.claude/sessions/<pid>.json` |
| Pi | `--consumer pi` | `pi` | `~/.pi/agent/sessions/<slug>/` |

Detection order: `--consumer` flag → `CCSM_CONSUMER` env var → auto-detect (most recently active config dir wins).

---

## Status Lifecycle

```
pending → start → in_progress → (work → note → note → ...) → close → complete
                                     ↓
                                 blocked / abandoned
```

See [.claude/skills/session-manager/SKILL.md](.claude/skills/session-manager/SKILL.md) for full lifecycle rules and the END-GATE protocol.

---

## Data Sources

| Path | Contains |
|------|----------|
| `<workspace>/.ccsm/sessions.json` | Registry entries (canonical) |
| `<workspace>/.ccsm/sessions/<name>.md` | Session detail files (progress, checklists) |
| `<workspace>/.claude/skills/` | Workflow skills (session-manager, seed-session, wrap-up) |
| `~/.claude/sessions/<pid>.json` | Live Claude session ID |
| `~/.claude/projects/<slug>/<uuid>.jsonl` | Claude transcript |

---

## Related

- [.claude/skills/session-manager/](.claude/skills/session-manager/) — Full session management protocol
- [AGENTS.md](AGENTS.md) — ccsm-swarm orchestration workflow
- [docs/ccsm-swarm.md](docs/ccsm-swarm.md) — MCP tool reference
- [Claude Code Hooks Reference](https://code.claude.com/docs/en/hooks)
- [Claude Code .claude Directory Guide](https://code.claude.com/docs/en/claude-directory)
