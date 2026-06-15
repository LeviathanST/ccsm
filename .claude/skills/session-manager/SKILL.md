---
name: session-manager
description: >
  Maintain the cc-tui session registry (.claude/sessions.json). Every agent working
  on this project MUST use this — create entries, update status, fill goal/scope.
  This is the single source of truth for team session tracking.
argument-hint: "<start|status|complete|list> — manage your session entry"
---

# Session Manager — cc-tui Session Registry

You are working on **cc-tui**, a TUI wrapper around Claude Code. This project has a session registry at `.claude/sessions.json` that tracks every work session. **You MUST maintain it.** The TUI sidebar reads this file — empty or stale entries mean the human can't see what's happening.

## Quick Reference

```
On session START  →  read .claude/sessions.json, create/claim entry (status: pending)
On first ACTION   →  update status to in_progress, fill goal + scope if missing
On session END    →  update status to completed / blocked / abandoned
BEFORE asking     →  check if you even know what session you're in
```

## 🔴 Session Handshake (DO FIRST, EVERY TIME)

When the PTY spawns, you must determine what kind of session this is and act accordingly.

### Step 1: Identify your session

```bash
cc-tui active           # find the in_progress entry with empty session_id → that's you
cc-tui show <name>      # read its goal, scope, tags
```

### Step 2: Branch by state

**If this is a NEW topic (fresh session, empty scope, no detail file):**

1. **Scan the board** — use token-efficient CLI, NOT full file reads:
   ```bash
   cc-tui summary        # counts by status — quickest overview
   cc-tui active         # who else is working? any dependencies?
   ```
2. **Scan related sessions only** — don't read every detail file. Use `cc-tui sessions` to find sessions with matching tags or names, then `cc-tui show <name>` for just those. If nothing looks related, skip.
3. Read the project's `CLAUDE.md` — architecture context. Don't read plan docs unless the human points to one.
4. **Ask the human:** "I see the board has [pending topics]. What's the goal for this session? Why now? How do you see it working?"
5. Synthesize their short description into a clear goal and scope
6. Write the detail file (`cp .claude/session-detail-template.md .claude/sessions/<name>.md` → fill all sections)
7. Update registry: `cc-tui scope <name> "<synthesized scope>"` + `cc-tui tag <name> <tags>`

**If this is an EXISTING session (already has scope, detail file, maybe pids):**

1. `cc-tui show <name>` — one command, ~20 tokens: status, goal, scope, pids, timestamps
2. **Only then read the detail file** — `.claude/sessions/<name>.md`. This is the ONE detail file you actually need. Skip reading other detail files unless `cc-tui show` reveals a direct dependency.
3. **Ask the human:** "This session is [status]. I see [summary from show]. What do you need to continue?"
4. Don't assume — the human may want to resume, pivot, or close it out

### Context budget rules

- **Use CLI for overview** — `cc-tui summary | active | sessions` = sub-100 tokens total
- **`cc-tui show` for inspection** — one session = ~200 tokens, not a full markdown file
- **Detail files are for deep work** — read only YOUR session's file + explicitly mentioned dependencies
- **Delegate if context is tight** — if the board is large and you're near 50% context, spawn a sub-agent to scan and report back a 3-line summary

## CLI Commands

`cc-tui` has built-in subcommands for querying the registry. Use these instead of `cat` or `jq` — they return agent-optimized compact output, consistent across all AI models.

| Command | Alias | Output |
|---|---|---|
| `cc-tui summary` | `sum` | One-line counts: `2 active \| 5 completed \| 1 blocked \| 3 total` |
| `cc-tui active` | `a` | One line per active session: `in_progress  name  — goal` |
| `cc-tui sessions` | `s` | One line per session (all statuses) |
| `cc-tui setup` | — | Install session tracking globally (run once) |
| `cc-tui` | — | Normal TUI: spawn Claude Code in PTY with sidebar |
| `cc-tui <path>` | — | TUI scoped to a specific workspace |

### When to use which

```
cc-tui summary   # → "2 active, 1 blocked" — quickest, sub-50 tokens
cc-tui active    # → "what are people working on?"
cc-tui sessions  # → "give me the full picture"
```

## 🔴 Team Awareness (MANDATORY)

**You are not alone.** The registry is a team board — other agents have active sessions. You MUST coordinate.

### Before Starting ANY Work

1. **Read the board:**
   ```bash
   cat .claude/sessions.json
   ```
2. **Scan for active sessions:**
   ```bash
   jq '.sessions[] | select(.status=="in_progress") | {name, goal, tags}' .claude/sessions.json
   ```
3. **Ask yourself these three questions:**

| Question | What to do |
|---|---|
| **"Is someone already doing this?"** | If an `in_progress` session has the same goal or tags → that work is claimed. Do NOT duplicate. Check if you should help or wait. |
| **"Does my task depend on another session?"** | If session X is building infrastructure you need → note the dependency. Check if X is `completed` before relying on it. If X is `blocked`, your work may also be blocked. |
| **"Is my task a subtask of an existing session?"** | If session Y's scope covers what you're about to do → join that session (add a tag or update its scope) instead of creating a new entry. |

### Decision Flow

```
Read registry
    │
    ├─→ Duplicate found?     → STOP. Report: "Session X is already doing this."
    │                          Offer: help that session, wait, or narrow your scope.
    │
    ├─→ Depends on session?  → NOTE the dependency in your scope field.
    │                          "Depends on: session-name (status: in_progress)"
    │                          Check its status before claiming your work is done.
    │
    └─→ No overlap?          → Create your entry. Proceed.
```

### Token Efficiency (READ THIS)

**Never `cat` the full registry.** Use the built-in CLI commands — they return agent-optimized compact output:

```bash
# Token-efficient queries (all agents, guaranteed consistent):
cc-tui summary   # or: sum  — counts by status (fewest tokens)
cc-tui active    # or: a    — only in_progress + blocked sessions
cc-tui sessions  # or: s    — all sessions, one line each

# Only use cat for editing — jq patterns are a fallback if the binary isn't built:
cat .claude/sessions.json   # OK when you need to edit entries
```

**The CLI is authoritative** — same output format across Claude, Codex, Gemini, and shell scripts. No per-model jq variation, no forgotten query syntax.

### Examples

**Duplicate detected:**
> I see `session-registry` is already `in_progress` with goal "Two-tier session registry for team visibility." My task (add session linking) falls under that scope. Instead of creating a new session, I'll work within that one.

**Dependency:**
> My work (add task dashboard) depends on `session-registry` being complete. It's still `in_progress` — I'll check its status before assuming the API is stable. I'll add `"Depends on: session-registry (in_progress)"` to my scope.

**No overlap:**
> No active sessions cover ANSI rendering improvements. Creating new entry `"ansi-truecolor-support"`.

## Registry Schema

The file is `.claude/sessions.json` at the workspace root. It contains:

```json
{
  "updated": "day20618T08:25Z",
  "sessions": [
    {
      "session_id": "",        // AUTO — set by cc-tui's merge_live_sessions. NEVER write.
      "name": "my-feature",    // MANUAL — short kebab-case label (used as display name)
      "goal": "Add X to Y",    // MANUAL — one sentence: what are we trying to do?
      "scope": "Details...",   // MANUAL — 2-4 sentences: approach, constraints, what's in/out
      "status": "in_progress", // MANUAL — one of the status values below
      "pids": [],              // AUTO — set by cc-tui. NEVER write.
      "tags": ["ui", "pty"],   // MANUAL — lowercase tags for filtering
      "started": "",           // AUTO — set by cc-tui. Leave empty.
      "completed": ""          // MANUAL — set when status becomes completed
    }
  ]
}
```

### Field Rules

| Field | Who Writes It | When |
|-------|--------------|------|
| `session_id` | **cc-tui runtime** — NEVER touch | merge_live_sessions links it |
| `pids` | **cc-tui runtime** — NEVER touch | merge_live_sessions appends |
| `started` | **cc-tui runtime** — NEVER touch | merge_live_sessions sets on first link |
| `name` | **You (agent)** | On session create |
| `goal` | **You (agent)** | On session create, refine as needed |
| `scope` | **You (agent)** | On session create, refine as needed |
| `status` | **You (agent)** | Update as work progresses |
| `tags` | **You (agent)** | On session create |
| `completed` | **You (agent)** | When status → completed |
| `updated` | **Both** | cc-tui updates on merge; you can touch it too |

## Status Lifecycle

```
pending ──→ in_progress ──→ completed
                │
                ├──→ blocked     (waiting on something external)
                └──→ abandoned   (gave up, not worth finishing)
                        │
                        └──→ trashed  (soft-delete, recoverable — set by human via TUI)
```

**Rules:**
- New entries start as `pending` or `in_progress` (your call, based on whether work already started)
- Only ONE entry should be `in_progress` at a time per workspace
- Mark completed sessions as `completed` — don't leave them `in_progress` forever
- `trashed` is for the human to set via the TUI (`d` key). Don't use it programmatically.

## Session Start: What To Do

### Step 1: Find your session ID

Claude Code writes your session to `~/.claude/sessions/<pid>.json`. Find it:
```bash
# Look for a session file matching the claude process
ls ~/.claude/sessions/*.json | head -5
# Or check the env var if available
echo $CLAUDE_SESSION_ID
```

If you can't find the session ID, that's fine — cc-tui's `merge_live_sessions` will link it later. Create an entry with empty `session_id` and it'll be linked automatically.

### Step 2: Read the current registry
```bash
cat .claude/sessions.json
```

### Step 3: Find or create your entry

- **If there's an entry with your session_id** → it's already linked. Check the status — update it to `in_progress` if it's still `pending`.
- **If there's an empty-session_id entry matching your work** → claim it (fill in name/goal/scope).
- **If nothing matches** → create a new entry.

### Step 4: Fill in the semantic fields

The `name`, `goal`, `scope`, and `tags` fields are how the human understands your session in the sidebar. **Don't leave them empty.**

```
Good goal:     "Add session trash/clean with separate trash section"
Bad goal:      ""  or  "fixes"  or  "work on stuff"

Good scope:    "Add Trashed status variant, trash/recover/clean methods to registry,
               trash section in sidebar with strikethrough style, d/D/C keybindings."
Bad scope:     ""  or  "implement the feature"
```

## Session End: Update Status

When you complete a task, **don't just stop**. Update the registry:

```json
{
  "name": "session-trash-clean",
  "goal": "Add session trash/clean with separate trash section",
  "status": "completed",
  "completed": "day20618T10:15Z"
}
```

## Session Detail Files — Agent's Responsibility

**You write the detail file, not the human.** When a session is created (especially via "Other"), the registry entry has minimal info. Your first job in the PTY is to:

1. **Ask clarifying questions** — "What's the goal? Why do you need this? How do you see it working?"
2. **Synthesize** — take the human's short description and expand it into a clear goal and scope
3. **Write the detail file** — copy `.claude/session-detail-template.md` to `.claude/sessions/<name>.md`, fill in all sections
4. **Update the registry** — `cc-tui scope <name> "<synthesized scope>"` and `cc-tui tag <name> <tags>`

The human is lazy with words — they'll say "add trash button" and you should extract: goal, scope, affected modules, dependencies, edge cases. The detail file is where that thinking lives.

### Template

Copy `.claude/session-detail-template.md` — fill all `{{placeholders}}` with real data.

### When to update

| Trigger | Action |
|---|---|
| Session created | Copy template, fill ALL sections — ask questions if anything is unclear |
| Status changes | Update the status badge line |
| Work done / progress | Append to the Progress Log section |
| New dependency | Add to Dependencies section |
| Discovery / decision | Add to Notes section |
| Session completed | Final update: status, completed date, summary in Progress Log |

### How

```bash
# On session start, create the detail file from template:
cp .claude/session-detail-template.md .claude/sessions/<name>.md

# Then use Edit to fill in the {{placeholders}} with real values.
# Append progress notes as you work.
```

**The detail file is the narrative; the JSON registry is the structured record.** Both should stay in sync. When you `cc-tui complete <name>`, also update the detail file's status line.

## How cc-tui Uses This File

1. **Sidebar display** — `name` is the primary label, `goal` is shown as detail text
2. **Status indicators** — `pending`=gray, `in_progress`=yellow, `completed`=green, `blocked`=red, `trashed`=gray strikethrough
3. **Session resume** — cc-tui's `merge_live_sessions` matches live session files to registry entries by `session_id`. Your manually-filled fields survive ephemeral session cleanup.
4. **Trash** — Human presses `d` to trash, `Enter` on trashed to recover, `D` to permanently clean

## The Matching Algorithm

cc-tui's `merge_live_sessions` runs every 2 seconds:
1. **Exact match**: live session's `sessionId` → registry entry with matching `session_id`. Updates pids and started timestamp.
2. **Fallback link**: live session links to the most recent unlinked `in_progress` entry (empty `session_id` + empty `pids` + status=`in_progress`). This is how `Ctrl+N` entries get connected.

This means: if you create a registry entry with empty `session_id` and status `in_progress`, cc-tui will find it and link your live session to it automatically.

## Example: Full Session Lifecycle

**Start of session (you are asked to "add session trash/clean"):**

```json
{
  "session_id": "",
  "name": "session-trash-clean",
  "goal": "Add session trash/clean with separate trash section",
  "scope": "Add Trashed variant to SessionStatus enum, trash/recover/clean/clean_all_trashed methods to WorkspaceRegistry, trash section in sidebar below active entries with strikethrough style, d/D/C keybindings, Enter-on-trash for recover.",
  "status": "in_progress",
  "pids": [],
  "tags": ["registry", "sidebar", "trash", "lifecycle"],
  "started": "",
  "completed": ""
}
```

**Mid-session (cc-tui has linked the live session):**

```json
{
  "session_id": "8d1e564-...",
  "name": "session-trash-clean",
  "goal": "Add session trash/clean with separate trash section",
  "status": "in_progress",
  "pids": [247154],
  "started": "day20618T06:51Z",
  ...
}
```

**End of session (work complete):**

```json
{
  "session_id": "8d1e564-...",
  "name": "session-trash-clean",
  "status": "completed",
  "completed": "day20618T10:15Z",
  ...
}
```

## Anti-Patterns (DON'T DO)

- ❌ **Touch `session_id` or `pids`** — cc-tui manages these. You'll break the linking.
- ❌ **Leave name/goal/scope empty** — the sidebar shows your entry as a blank label.
- ❌ **Create entries without updating `.updated`** — minor, but stale timestamp hurts sorting.
- ❌ **Set `trashed` status** — that's for the human via TUI keybindings.
- ❌ **Parse JSONL transcript files** — cc-tui uses `claude --resume` for replay, not transcript parsing.
- ❌ **Write a markdown session tracker** — JSON is the single source of truth, parseable by all tools, diffable in git.

## Working with the JSON from Different Agents

The `.claude/sessions.json` file is plain JSON. Any agent or script can read/write it:

- **Claude Code**: `Read` / `Edit` / `Write` tools, or Bash with `jq`
- **Codex**: Standard file read/write
- **Gemini**: Use available file tools
- **Shell scripts**: `jq '.sessions[] | select(.status=="in_progress")' .claude/sessions.json`
- **Python**: `json.load()` / `json.dump()`
- **Rust**: `serde_json` (cc-tui already has this)
