---
name: session-manager
description: >
  Maintain the ccsm session registry (.claude/sessions.json). Every agent working
  on this project MUST use this — create entries, update status, fill goal/scope.
  This is the single source of truth for team session tracking.
argument-hint: "<start|status|complete|list> — manage your session entry"
---

# Session Manager — ccsm Session Registry

You are working on **ccsm**, a CLI session registry and workflow harness for Claude Code. This project has a session registry at `.claude/sessions.json` that tracks every work session. **You MUST maintain it.**

## Quick Reference

```
On session START  →  read .claude/sessions.json, create/claim entry (status: pending)
On first ACTION   →  update status to in_progress, fill goal + scope if missing
After EVERY change →  ccsm note <name> "<what you did and why>"
On session END    →  END-GATE → ccsm complete
BEFORE asking     →  check if you even know what session you're in
```

## 🔴 Session Handshake (DO FIRST, EVERY TIME)

### Step 1: Identify your session

```bash
ccsm list --active       # find the in_progress entry with empty session_id → that's you
ccsm show <name>         # registry fields + detail file section headlines
```

### Step 2: Branch by state

**If this is a NEW topic (fresh session, empty scope, no detail file):**

1. **Scan the board** — token-efficient CLI:
   ```bash
   ccsm list --summary    # counts by status — quickest overview
   ccsm list --active     # who else is working? any dependencies?
   ```
2. Read the project's `CLAUDE.md` for architecture context.
3. **Ask the human:** "What's the goal? Why now? How do you see it working?"
4. Synthesize into goal + scope, create entry:
   ```bash
   ccsm sequence -q new <name> -q start <name> -q scope <name> "<approach>" -q tag <name> <tag1> <tag2>
   # `new` auto-creates .claude/sessions/<name>.md from template.
   # Edit the detail file NOW — fill remaining sections before starting work.
   ```

**If this is an EXISTING session (has scope, detail file, maybe pids):**

1. `ccsm show <name>` — registry fields + section headlines with line counts
2. `ccsm show <name> --section progress-log` — pull just the progress log
3. **Ask the human:** "This session is [status]. What do you need to continue?"

### Context budget rules

- **`ccsm list --summary`** — sub-50 tokens, quickest overview
- **`ccsm show <name>`** — ~200 tokens, shows registry + section headlines
- **`ccsm show <name> --section <s>`** — pull one section, save tokens
- **Detail files are for deep work** — read only YOUR session's file + explicit dependencies

## 🔴 The 5 Laws of Session Mutation

Every field change has rules. Follow them.

### 1. Goal

| CAN change | CANNOT change |
|------------|---------------|
| Human redirects you | "I found a more interesting problem" |
| Initial goal was too vague | You're bored and want to pivot |
| Scope shift forces goal change | Without human approval |

**Must document:** `ccsm note <name> "GOAL: <old> → <new>. Reason: <why>"`

### 2. Scope

| CAN change | CANNOT change |
|------------|---------------|
| New constraint discovered | "While I'm here, I'll also refactor X" |
| Something proved infeasible | Scope creep without human approval |
| Human adds/removes items | Gold-plating |

**Must document:** `ccsm note <name> "SCOPE: changed <what>. Deferred: <what's pushed out>"`

### 3. Status

| CAN change | CANNOT change |
|------------|---------------|
| Clear boundary crossed (started/done/blocked/abandoned) | Status ping-pong (complete→in_progress→complete) |
| Blocked by specific, named dependency | "Blocked because it's hard" |
| Abandoned with clear rationale | Abandoned because you lost interest |

**Must document:**
- Blocked: `ccsm note <name> "BLOCKED: <specific blocker>. Resolution: <what needs to happen>"`
- Abandoned: `ccsm note <name> "ABANDONED: <why>. Alternative: <what should happen instead>"`

### 4. Tags

| CAN change | CANNOT change |
|------------|---------------|
| Classification changes | Tag spam (5+ is a smell) |
| Priority shifts | Tags that duplicate the goal text |

Not mandatory to note, but do it if the tag change represents a significant reclassification.

### 5. Progress Log — MANDATORY

**You MUST `ccsm note` after ANY non-trivial work:**
- Code written or changed
- Decision made (architecture, tool choice, approach)
- Roadblock hit
- Milestone reached
- Dependency added/removed

```bash
ccsm note <name> "<what you did and why>"
```

The progress log IS the audit trail. If you did something, log it. **Never skip this.**

## 🔴 End-Gate Protocol (BEFORE `ccsm complete`)

Before marking a session complete, you MUST answer these three questions via `ccsm note`:

```
1. WHAT was built — vs. what the scope promised?
2. What was explicitly NOT done — what's deferred, cut, or out of scope?
3. What's LEFT — technical debt, follow-up sessions, open questions?
```

Example:
```bash
ccsm note my-feature "END-GATE: built — PTY embedding with fixed-grid ANSI rendering (matches scope). deferred — F-key passthrough (needs separate session). left — vt100 parser has edge cases with OSC sequences."
```

**You cannot `ccsm complete` without an END-GATE note.**

## Session Lifecycle

```
NEW → start → (work → note → work → note → ...) → END-GATE → complete
                                                       ↓
                                                   blocked/abandoned
                                                   (must note why)
```

## CLI Commands

### Query

| Command | Output |
|---|---|
| `ccsm list` | All sessions, one line each |
| `ccsm list --active` | in_progress + blocked only |
| `ccsm list --summary` | Counts per status |
| `ccsm list --status <s>` | Filter by status. Pass "help" to see what each status means |
| `ccsm show <name>` | Registry fields + detail file section headlines (with line counts) |
| `ccsm show <name> --section <s>` | Extract one section from the detail file |
| `ccsm --help` | Full command list |

### Mutate

| Command | Transition |
|---|---|
| `ccsm new <name> -g <goal>` | → pending |
| `ccsm start <name>` | pending → in_progress (max 1 per workspace) |
| `ccsm complete <name>` | in_progress → completed, sets timestamp |
| `ccsm block <name>` | in_progress → blocked (waiting on dependency) |
| `ccsm abandon <name>` | in_progress → abandoned (no longer relevant) |
| `ccsm pending <name>` | → pending, clears session_id + pids + timestamps |
| `ccsm scope <name> <text>` | Set scope field |
| `ccsm tag <name> <tags...>` | Replace tags |
| `ccsm attach <name> <sid>` | Manually link a Claude session_id |
| `ccsm resume <name>` | Spawn claude. --resume if session_id set, -n <name>, harvests session_id on exit |
| `ccsm note <name> <text>` | Append timestamped entry to detail file Progress Log |
| `ccsm sequence -q <cmd> <args...> ...` | Batch mutations under a single lock/save. Faster than `&&` chaining |

### Lifecycle (trash/clean)

| Command | Transition |
|---|---|
| `ccsm trash <name>` | → trashed (soft-delete, recoverable) |
| `ccsm recover <name>` | trashed → in_progress |
| `ccsm clean <name>` | Permanent delete: transcript + session files + entry. Irreversible |
| `ccsm clean-all` | Permanent delete ALL trashed entries. Irreversible |

### Statuses

```
pending      — planned, not started yet
in_progress  — actively working on (max 1 per workspace)
completed    — finished successfully
blocked      — can't proceed, waiting on a dependency
abandoned    — gave up, no longer relevant
trashed      — soft-deleted, recoverable with `ccsm recover <name>`
```

## Registry Schema

`.claude/sessions.json` at the workspace root:

```json
{
  "updated": "day20618T08:25Z",
  "sessions": [
    {
      "session_id": "",        // AUTO — ccsm manages this
      "name": "my-feature",    // MANUAL — kebab-case label
      "goal": "Add X to Y",    // MANUAL — one sentence
      "scope": "Details...",   // MANUAL — 2-4 sentences: approach, constraints, in/out
      "status": "in_progress", // MANUAL — pending|in_progress|completed|blocked|abandoned|trashed
      "pids": [],              // AUTO — ccsm manages this
      "tags": ["ui", "pty"],   // MANUAL — lowercase tags
      "started": "",           // AUTO — ccsm manages this
      "completed": ""          // MANUAL — set when status → completed
    }
  ]
}
```

### Field Rules

| Field | Who | When |
|-------|-----|------|
| `session_id` | **ccsm** — NEVER touch | Harvested from `~/.claude/sessions/<pid>.json` on exit. Use `ccsm attach` to set manually |
| `pids` | **ccsm** — NEVER touch | Set at spawn, cleared on exit |
| `started` | **ccsm** — NEVER touch | Set on first spawn |
| `name`, `goal`, `scope`, `tags` | **You** | On session create, refine as needed |
| `status` | **You** | Update as work progresses |
| `completed` | **You** | When status → completed |

## Session Detail Files

Detail files live at `.claude/sessions/<name>.md`. `ccsm new` auto-creates them from the template — your job is to **fill them in**, not create them.

```bash
ccsm show <name>          # check what's already filled
# Then Edit .claude/sessions/<name>.md to replace remaining {{placeholders}}
```

**Token-efficient reading:**
```bash
ccsm show <name>                    # headlines + line counts
ccsm show <name> --section progress-log   # just one section
ccsm show <name> --section dependencies   # just one section
```

Sections: `goal`, `scope-plan` (or `scope / plan`), `tags`, `live-session-data`, `progress-log`, `dependencies`, `notes`.

### When to update

| Trigger | Action |
|---|---|
| Session created | Copy template, fill ALL sections |
| Status changes | Update status badge line |
| ANY work done | `ccsm note <name> "<what + why>"` |
| New dependency | Add to Dependencies |
| Discovery | Add to Notes |
| Session completed | END-GATE note first, then `ccsm complete` |

## How Resume Works

`ccsm resume <name>`:

1. **Spawn**: captures child PID, writes to registry, polls `~/.claude/sessions/<pid>.json` (up to 5s), harvests `sessionId` BEFORE Claude exits
2. **Wait**: blocks on `child.wait()` — Claude runs interactively
3. **Cleanup**: clears stale pids, saves registry
4. **Next resume**: finds session_id → `claude --resume <id> -n <name>`

Session_id is persisted before Claude exits — Claude v2.1+ deletes the session file on graceful exit, so harvesting must happen while the process is alive.

## 🔴 Team Awareness (MANDATORY)

### Before Starting ANY Work

1. `ccsm list --summary` — quick counts
2. `ccsm list --active` — who's working?
3. Ask:

| Question | What to do |
|---|---|
| **Duplicate?** | STOP. Report: "Session X is already doing this." |
| **Dependency?** | Note in scope: "Depends on: <name> (status: ...)" |
| **Subtask?** | Join existing session, don't create new one |
| **No overlap?** | Create entry. Proceed. |

### Decision Flow

```
ccsm list --active
    │
    ├─→ Duplicate?     → STOP. Help that session or narrow scope.
    ├─→ Depends on?    → NOTE in scope. Check status before claiming done.
    └─→ No overlap?    → Create entry. Proceed.
```

## Anti-Patterns

- ❌ **Touch `session_id` or `pids`** — ccsm manages these
- ❌ **Leave name/goal/scope empty** — blank labels help no one
- ❌ **Skip the progress log** — `ccsm note` after every change. Never miss it.
- ❌ **Complete without END-GATE** — the three questions are mandatory.
- ❌ **Change goal/scope without documenting why** — the 5 Laws require rationale.
- ❌ **Status ping-pong** — complete↔in_progress without a real reason.
- ❌ **Read the full detail file blindly** — use `--section` to pull what you need
- ❌ **Parse JSONL transcripts** — ccsm uses `claude --resume`, not transcript parsing
- ❌ **Use `jq`/`cat` for reading** — CLI commands are token-optimized and consistent across agents
