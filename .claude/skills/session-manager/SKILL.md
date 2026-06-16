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
   ```

5. **IMMEDIATELY fill the detail file.** `ccsm new` creates a template — Scope/Plan and Tags will say `(fill in)`. You MUST edit these NOW:
   ```bash
   # Fill Scope/Plan with the concrete approach
   ccsm scope <name> "approach, constraints, what's in/out — be specific"
   # Fill Tags
   ccsm tag <name> <tag1> <tag2> ...
   ```
   Do NOT skip this. An empty detail file means the next agent to resume this session has no plan to follow.

**If this is an EXISTING session (has scope, detail file, maybe pids):**

1. `ccsm show <name>` — registry fields + section headlines with line counts
2. `ccsm show <name> --section progress-log` — pull just the progress log

3. **Check the detail file is actually filled.** Pull Scope/Plan and Tags:
   ```bash
   ccsm show <name> --section scope-plan
   ccsm show <name> --section tags
   ```
   If either says `(fill in` or is empty, the detail file is still a template. **STOP and say:**
   > "This session's detail file is an empty template — the registry has a goal but no plan. Want me to flesh out the plan before we start?"
   - If yes: design the plan, fill Scope/Plan + Tags, log a progress note. Then continue.
   - If no: proceed with just the registry goal as context.

4. **Ask the human:** "This session is [status]. What do you need to continue?"

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

## 🔴 Scope-Gate Protocol (MANDATORY)

The CLAUDE.md already says: *"NEVER execute work outside the current session's scope."* This section turns that rule into an **interactive gate** — when the human asks for off-scope work, you MUST pause and offer a structured fork. Never silently drift.

### What's "in scope"?

Work is **in scope** if it directly advances the session's stated `goal` and fits within the `scope` field. The goal is your compass. When uncertain, ask: "Does this move us toward `<goal>`?" If no, the gate triggers.

### What definitely triggers the gate

| Situation | Example |
|-----------|---------|
| New feature unrelated to goal | Human: "Add session export to CSV" while session goal is "fix PTY resize bug" |
| Infrastructure change not in scope | Human: "Rewrite the lockfile module" while session goal is "add archive command" |
| Cross-cutting concern that needs its own session | Human: "Add structured logging everywhere" during a focused bugfix session |
| Work on a different project subsystem | Human: "Refactor the sidebar renderer" while session is about the `sequence` subcommand |

### What does NOT trigger the gate

- Clarifying questions about the current work
- Minor approach adjustments that stay within scope
- Things the human explicitly says are part of the current work
- Trivial one-liners that take under 2 minutes (`ccsm note` is enough)

### The Gate Pattern

When you detect an out-of-scope request, follow this exact pattern:

**Step 1 — Flag** (name the mismatch)

> "This isn't in the current session plan. `scope-gate-protocol`'s goal is `<goal>`, and this request is about `<different thing>`."

**Step 2 — Offer** (exactly 3 options, let the human choose)

> "Options:
> **(1)** Create a pending session for it — we'll track it and pick it up later.
> **(2)** Update the current scope to include it — expand what this session covers.
> **(3)** Handle it as a quick aside — if it's trivial enough to do in under 2 minutes."

**Step 3 — Execute** based on human choice:

*Option 1 (new pending session):*
```bash
ccsm new <name> -g "<goal>"
ccsm scope <name> "<approach>"
ccsm tag <name> scope-gate <relevant-tags>
# Then return to current session work.
```

*Option 2 (expand current scope):*
```bash
ccsm scope <current-session> "<original scope> + <what's being added>"
ccsm note <current-session> "SCOPE-GATE: expanded scope to include <thing>"
# Proceed with the now-in-scope work.
```

*Option 3 (quick aside):*
Do the thing immediately. Then:
```bash
ccsm note <current-session> "ASIDE: <one-liner about what was done>"
```

### The hard rule

**Never** start off-scope work without triggering the gate. If in doubt, trigger it. A false positive costs one exchange; silent scope drift costs the session's coherence and wastes future resume attempts.

### Scope-Gate vs. Proactive Ideation

| Protocol | Trigger | Who initiates |
|----------|---------|--------------|
| **Scope-Gate** | Human makes an out-of-scope request | Human → Agent reacts |
| **Proactive Ideation** | Agent notices something during work | Agent → Human reacts |

They are complementary. Scope-gate creates sessions reactively; ideation creates them proactively.

## 🔴 Proactive Ideation Dashboard (MANDATORY)

The session registry is a **collective brain**. During work, you will discover things worth tracking: tech debt, improvement ideas, missing features, architectural smells. Capture these as **pending sessions** before they evaporate.

This section governs when and how you proactively offer to create sessions. See the comparison table in the Scope-Gate Protocol section above for how these protocols differ.

### When to Offer — The 5 Triggers

You **MUST** consider offering when you encounter **any** of these during work:

| # | Trigger | Example |
|---|---------|---------|
| 1 | **Friction** — a workaround, brittle code, or repeated manual step | "I had to manually export the widget after every rebuild" |
| 2 | **Gap** — something the project clearly needs is absent | "There's no command to bulk-tag sessions" |
| 3 | **Tech debt** — a TODO comment, a hack, an unfixed edge case, dead code | "This parser panics on edge cases; see comment on line 142" |
| 4 | **Architecture smell** — a design pattern would simplify multiple areas | "Three modules implement the same matching logic" |
| 5 | **End-Gate residue** — during Q3 ("What's LEFT?"), a deferred item that warrants its own session | "The END-GATE listed OSC edge cases — that's a standalone session" |

You are **NOT required** to offer for:
- Trivial one-liner fixes you can do immediately (`ccsm note` is enough)
- Items already tracked as a pending session
- Vague feelings without a concrete, specific observation

### The 3 Quality Gates

Every candidate MUST pass all three gates. If any gate fails, do NOT offer.

**Gate 1 — Specificity.** Can you name the exact thing and what work is needed?

```
PASS: "The config parser ignores TOML tables — a session to add table support"
FAIL: "This code feels messy" (what specifically? what work?)
```

**Gate 2 — Worth-a-Session.** Would someone reading the goal/scope understand what to do and why it matters? The work should represent at least 15-30 minutes.

```
PASS: A future developer could pick up the session and start working immediately.
FAIL: A two-minute fix better handled as `ccsm note <current-session>` or done now.
```

**Gate 3 — Not-Already-Tracked.** Verify the registry before offering:

```bash
ccsm list --status pending   # is this already captured?
```

If a pending session covers this ground, **skip the offer**. Instead, enrich that session:

```bash
ccsm note <existing-pending-name> "IDEATION: also noticed <finding>"
```

### The Offer Pattern

When a candidate passes all three gates, follow this exact pattern. **Never create the session unilaterally.**

**Phase 1 — Notice** (one sentence)

> "I noticed that <specific observation>."

Example: "I noticed the sidebar refreshes the entire list on every 2s poll instead of doing a targeted update of changed entries."

**Phase 2 — Diagnose** (one sentence naming the impact)

> "This means <why it matters>."

Example: "This means every 2s the UI flickers on large registries and wastes CPU redrawing unchanged rows."

**Phase 3 — Propose** (one sentence with a clear yes/no fork)

> "Want me to create a pending session for <this>?"

If yes: create the session with goal and scope.

```bash
ccsm new <name> -g "<goal>"
ccsm scope <name> "..."
ccsm tag <name> ideation <tags...>
```

If no: no pressure. Log it so it doesn't vanish:

```bash
ccsm note <current-session> "IDEA (not tracked): <observation>"
```

You do NOT reopen the topic unless the human does.

### Bundling Rule

- **Bundle related items** into one pending session. If two observations share the same module or category, group them.
- **Max 1 new pending session per agent session** for unrelated topics.
- If you discover a genuinely independent idea after already creating one, log it instead: `ccsm note <current-session> "IDEA (deferred): <observation>"`

| If items share... | Bundle into one session named... |
|-------------------|----------------------------------|
| The same module | `<module>-improvements` |
| The same category (all debt, all gaps) | `<category>-audit` |
| A dependency chain | An umbrella session with dependencies |

### Counter-Indications

- ❌ **Do NOT offer** during the human's first interaction with a new session — they haven't seen the problem space yet
- ❌ **Do NOT offer** when mid-task with the human actively directing you — finish first, then mention at the next natural pause
- ❌ **Do NOT offer** for "this would be nice" without articulating why it matters (fails specificity gate)
- ❌ **Do NOT create the session unilaterally** — always ask first
- ❌ **Do NOT re-offer** on a topic the human declined — accept the answer
- ❌ **Do NOT offer** more than one idea per natural break point — bundle or defer

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
