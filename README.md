# ccsm — Session Registry & Lifecycle Manager

**ccsm is an [OpenCode](https://opencode.ai) companion.** It tracks sessions in `.ccsm/sessions.json`, links transcripts, and spawns OpenCode with resume support. ccsm is the backbone of a structured multi-agent workflow: plan → session → work → review → merge → archive. Every session is tracked, every transcript is linked, and nothing falls through the cracks.

> **ccsm-swarm was removed in v0.21.0.** See [AGENTS.md](AGENTS.md) for the current orchestration approach. If you need swarm functionality, use ccsm v0.20.0 or earlier.

---

## Quick Start

```bash
cargo build --release
cp target/release/ccsm ~/.local/bin/ccsm
ccsm setup
```

Prerequisites: Rust toolchain. Build times ~30s.

---

## Architecture

```
ccsm CLI              — Session registry (new, start, resume, complete, archive)
    │
    ├── <workspace>/.ccsm                  # Identity file (TOML: version + id)
    ├── ~/.ccsm/<id>/sessions.json         # Canonical registry (single source of truth)
    ├── ~/.ccsm/<id>/sessions/<name>.md    # Per-session detail files (progress, checklists)
    ├── ~/.ccsm/<id>/session-group/        # Group detail files
    ├── ~/.ccsm/<id>/worktrees/            # Git worktrees for branch isolation
    ├── ~/.ccsm/<id>/config.toml           # Project policy config
    └── OpenCode SQLite DB (~/.local/share/opencode/opencode.db)
```

### Design Principles

1. **CLI-first.** Purpose-built subcommands for every operation. Same output format across agents and shell scripts.
2. **`.ccsm/sessions.json` is canonical.** Structured JSON, diffable, parseable by any tool.
3. **OpenCode-first.** ccsm is designed for and tested against [OpenCode](https://opencode.ai). Other consumers are legacy (see below).
4. **Batch with `sequence`.** Multiple mutations under one lock and one save — faster than `&&` chaining.
5. **Advisory file locking.** Exclusive `flock` on `.ccsm/sessions.json.lock` before every mutation.

---

## Reference: Skills & Documentation

Most workflow documentation lives in Claude skills (`.claude/skills/`). These are the canonical reference — the tables below tell you where to look.

### Core Workflow (Humans & Agents)

| Skill / Doc | What it covers | For |
|-------------|----------------|-----|
| [.claude/skills/session-manager/SKILL.md](.claude/skills/session-manager/SKILL.md) | Full session lifecycle — start, work, note, complete. Status rules, attach modes, team awareness | Every session |
| [AGENTS.md](AGENTS.md) | Orchestration workflow for multi-agent sessions | Orchestrator setup |
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

_(ccsm-swarm was removed in v0.21.0. Use v0.20.0 if you need swarm functionality.)_

---

## Consumer Model

ccsm is **OpenCode-first** — this is the only actively maintained consumer. Other consumers are legacy and receive no active development.

| Consumer | Status | Binary | Sessions |
|----------|--------|--------|----------|
| **OpenCode** | **Active (default)** | `opencode` | SQLite at `~/.local/share/opencode/opencode.db` |
| Claude Code | Legacy | `claude` | `~/.claude/sessions/<pid>.json` |
| Pi | Legacy | `pi` | `~/.pi/agent/sessions/<slug>/` |
| CodeWhale | Legacy | — | — |

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
| `<workspace>/.ccsm` | Identity file (TOML with workspace ID) |
| `~/.ccsm/<id>/sessions.json` | Registry entries (canonical) |
| `~/.ccsm/<id>/sessions/<name>.md` | Session detail files (progress, checklists) |
| `<workspace>/.claude/skills/` | Workflow skills (session-manager, seed-session, wrap-up) |
| `~/.local/share/opencode/opencode.db` | OpenCode session transcripts |

---

## Related

- [.claude/skills/session-manager/](.claude/skills/session-manager/) — Full session management protocol
- [AGENTS.md](AGENTS.md) — Multi-agent orchestration workflow
- [OpenCode](https://opencode.ai) — The AI coding agent ccsm is built for
- [Learn Session Manager Skill](https://opencode.ai/docs/skills) — How to write OpenCode skills
