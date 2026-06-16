---
name: session-manager
description: >
  Maintain the cc-tui session registry (.claude/sessions.json). Every agent working
  on this project MUST use this — create entries, update status, fill goal/scope.
  This is the single source of truth for team session tracking.
argument-hint: "<start|status|complete|list> — manage your session entry"
---

# Session Manager — cc-tui Session Registry

You are working on **cc-tui**, a CLI session registry for Claude Code. This project has a session registry at `.claude/sessions.json` that tracks every work session. **You MUST maintain it.**

## Quick Reference

```
On session START  →  read .claude/sessions.json, create/claim entry (status: pending)
On first ACTION   →  update status to in_progress, fill goal + scope if missing
On session END    →  update status to completed / blocked / abandoned
BEFORE asking     →  check if you even know what session you're in
```

## 🔴 Session Handshake (DO FIRST, EVERY TIME)

### Step 1: Identify your session

```bash
cc-tui list --active       # find the in_progress entry with empty session_id → that's you
cc-tui show <name>         # registry fields + detail file section headlines
```

### Step 2: Branch by state

**If this is a NEW topic (fresh session, empty scope, no detail file):**

1. **Scan the board** — token-efficient CLI:
   ```bash
   cc-tui list --summary    # counts by status — quickest overview
   cc-tui list --active     # who else is working? any dependencies?
   ```
2. Read the project's `CLAUDE.md` for architecture context.
3. **Ask the human:** "What's the goal? Why now? How do you see it working?"
4. Synthesize into goal + scope, create entry:
   ```bash
   # Use sequence to create and configure in one command:
   cc-tui sequence -q new <name> -q start <name> -q scope <name> "<approach>" -q tag <name> <tag1> <tag2>
   # `new` auto-creates .claude/sessions/<name>.md from template.
   # Edit the detail file NOW — fill remaining sections before starting work.
   ```

**If this is an EXISTING session (has scope, detail file, maybe pids):**

1. `cc-tui show <name>` — registry fields + section headlines with line counts
2. `cc-tui show <name> --section progress-log` — pull just what you need
3. **Ask the human:** "This session is [status]. What do you need to continue?"

### Context budget rules

- **`cc-tui list --summary`** — sub-50 tokens, quickest overview
- **`cc-tui show <name>`** — ~200 tokens, shows registry + section headlines
- **`cc-tui show <name> --section <s>`** — pull one section, save tokens
- **Detail files are for deep work** — read only YOUR session's file + explicit dependencies

## CLI Commands

### Query

| Command | Output |
|---|---|
| `cc-tui list` | All sessions, one line each |
| `cc-tui list --active` | in_progress + blocked only |
| `cc-tui list --summary` | Counts per status |
| `cc-tui list --status <s>` | Filter by status. Pass "help" to see what each status means |
| `cc-tui show <name>` | Registry fields + detail file section headlines (with line counts) |
| `cc-tui show <name> --section <s>` | Extract one section from the detail file |
| `cc-tui --help` | Full command list |

### Mutate

| Command | Transition |
|---|---|
| `cc-tui new <name> -g <goal>` | → pending |
| `cc-tui start <name>` | pending → in_progress (max 1 per workspace) |
| `cc-tui complete <name>` | in_progress → completed, sets timestamp |
| `cc-tui block <name>` | in_progress → blocked (waiting on dependency) |
| `cc-tui abandon <name>` | in_progress → abandoned (no longer relevant) |
| `cc-tui pending <name>` | → pending, clears session_id + pids + timestamps |
| `cc-tui scope <name> <text>` | Set scope field |
| `cc-tui tag <name> <tags...>` | Replace tags |
| `cc-tui attach <name> <sid>` | Manually link a Claude session_id |
| `cc-tui resume <name>` | Spawn claude. --resume if session_id set, -n <name>, harvests session_id on exit |
| `cc-tui sequence -q <cmd> <args...> ...` | Batch mutations under a single lock/save. Faster than `&&` chaining |

### Lifecycle (trash/clean)

| Command | Transition |
|---|---|
| `cc-tui trash <name>` | → trashed (soft-delete, recoverable) |
| `cc-tui recover <name>` | trashed → in_progress |
| `cc-tui clean <name>` | Permanent delete: transcript + session files + entry. Irreversible |
| `cc-tui clean-all` | Permanent delete ALL trashed entries. Irreversible |

### Statuses

```
pending      — planned, not started yet
in_progress  — actively working on (max 1 per workspace)
completed    — finished successfully
blocked      — can't proceed, waiting on a dependency
abandoned    — gave up, no longer relevant
trashed      — soft-deleted, recoverable with `cc-tui recover <name>`
```

## Registry Schema

`.claude/sessions.json` at the workspace root:

```json
{
  "updated": "day20618T08:25Z",
  "sessions": [
    {
      "session_id": "",        // AUTO — cc-tui manages this
      "name": "my-feature",    // MANUAL — kebab-case label
      "goal": "Add X to Y",    // MANUAL — one sentence
      "scope": "Details...",   // MANUAL — 2-4 sentences: approach, constraints, in/out
      "status": "in_progress", // MANUAL — pending|in_progress|completed|blocked|abandoned|trashed
      "pids": [],              // AUTO — cc-tui manages this
      "tags": ["ui", "pty"],   // MANUAL — lowercase tags
      "started": "",           // AUTO — cc-tui manages this
      "completed": ""          // MANUAL — set when status → completed
    }
  ]
}
```

### Field Rules

| Field | Who | When |
|-------|-----|------|
| `session_id` | **cc-tui** — NEVER touch | Harvested from `~/.claude/sessions/<pid>.json` on exit. Use `cc-tui attach` to set manually |
| `pids` | **cc-tui** — NEVER touch | Set at spawn, cleared on exit |
| `started` | **cc-tui** — NEVER touch | Set on first spawn |
| `name`, `goal`, `scope`, `tags` | **You** | On session create, refine as needed |
| `status` | **You** | Update as work progresses |
| `completed` | **You** | When status → completed |

## Session Detail Files

Detail files live at `.claude/sessions/<name>.md`. `cc-tui new` auto-creates them from the template with placeholders — your job is to **fill them in**, not create them.

```bash
cc-tui show <name>          # check what's already filled
# Then Edit .claude/sessions/<name>.md to replace remaining {{placeholders}}
```

**Token-efficient reading:**
```bash
cc-tui show <name>                    # headlines + line counts
cc-tui show <name> --section progress-log   # just one section
cc-tui show <name> --section dependencies   # just one section
```

Sections: `goal`, `scope-plan` (or `scope / plan`), `tags`, `live-session-data`, `progress-log`, `dependencies`, `notes`.

### When to update

| Trigger | Action |
|---|---|
| Session created | Copy template, fill ALL sections |
| Status changes | Update status badge line |
| Work done | Append to Progress Log |
| New dependency | Add to Dependencies |
| Discovery | Add to Notes |
| Session completed | Final update: status, completed date, summary |

## How Resume Works

`cc-tui resume <name>`:

1. **Spawn**: captures child PID, writes to registry, polls `~/.claude/sessions/<pid>.json` (up to 5s), harvests `sessionId` BEFORE Claude exits
2. **Wait**: blocks on `child.wait()` — Claude runs interactively
3. **Cleanup**: clears stale pids, saves registry
4. **Next resume**: finds session_id → `claude --resume <id> -n <name>`

Session_id is persisted before Claude exits — Claude v2.1+ deletes the session file on graceful exit, so harvesting must happen while the process is alive.

## 🔴 Team Awareness (MANDATORY)

### Before Starting ANY Work

1. `cc-tui list --summary` — quick counts
2. `cc-tui list --active` — who's working?
3. Ask:

| Question | What to do |
|---|---|
| **Duplicate?** | STOP. Report: "Session X is already doing this." |
| **Dependency?** | Note in scope: "Depends on: <name> (status: ...)" |
| **Subtask?** | Join existing session, don't create new one |
| **No overlap?** | Create entry. Proceed. |

### Decision Flow

```
cc-tui list --active
    │
    ├─→ Duplicate?     → STOP. Help that session or narrow scope.
    ├─→ Depends on?    → NOTE in scope. Check status before claiming done.
    └─→ No overlap?    → Create entry. Proceed.
```

## Anti-Patterns

- ❌ **Touch `session_id` or `pids`** — cc-tui manages these
- ❌ **Leave name/goal/scope empty** — blank labels help no one
- ❌ **Read the full detail file blindly** — use `--section` to pull what you need
- ❌ **Parse JSONL transcripts** — cc-tui uses `claude --resume`, not transcript parsing
- ❌ **Use `jq`/`cat` for reading** — CLI commands are token-optimized and consistent across agents
