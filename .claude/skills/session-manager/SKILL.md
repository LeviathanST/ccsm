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
On session END    →  ccsm close <name> (gate checks) → ccsm complete <name> (or --force to bypass)
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

## 🔴 Mandatory Protocols

**Read the relevant protocol when its trigger fires.** Don't read all of them at once.

| # | Protocol | Trigger | File |
|---|----------|---------|------|
| 1 | **Team Awareness** | Session START — scan for duplicates, dependencies, overlaps | `protocols/team-awareness.md` |
| 2 | **Scope-Gate** | Human asks for work outside current session's scope | `protocols/scope-gate.md` |
| 3 | **Proactive Ideation** | You notice friction, gaps, tech debt, or architecture smells | `protocols/proactive-ideation.md` |
| 4 | **Cross-Session Teammate** | Your work touches another session's domain or creates a dependency | `protocols/cross-session-teammate.md` |

## Reference Index

Pull these on demand — don't read them all.

| Topic | File | When |
|-------|------|------|
| Full CLI commands + flags | `reference/cli-commands.md` | You need exact command syntax |
| Registry schema + field rules | `reference/registry-schema.md` | You need field-level detail |
| 5 Laws + End-Gate Protocol | `reference/mutation-laws.md` | You're about to mutate a field or complete a session |
| Session lifecycle + resume | `reference/session-lifecycle.md` | You need lifecycle or resume mechanics |
| Anti-patterns | `reference/anti-patterns.md` | You want to avoid common mistakes |

## Checklist

Track sub-tasks within a session with checkbox items. The `ccsm close` gate blocks completion while pending or blocked items remain.

```
# Create checklist-ready session, or add section later
ccsm new <name> -c -g "goal"          # with ## Checklist section
ccsm checklist <name> --init          # add section to existing session

# Add items (any status)
ccsm check <name> "Write integration tests" -s pending    # add new item
ccsm check <name> "Blocked on API" -s blocked             # add blocked item

# Toggle items (by index or text match)
ccsm check <name> 1 -s done                               # mark #1 done
ccsm check <name> "Write integration tests" -s skipped    # mark by text

# List
ccsm checklist <name>                  # all items with summary
```

- `ccsm check` auto-creates the `## Checklist` section when it doesn't exist.
- Item ref is a 1-based index or case-insensitive text substring.
- If no item matches, the text is added as a new item.
- Close gate: counts pending + blocked items → blocks `ccsm close`.

## Grouping & Dependencies

Group related sessions together with ordering, dependencies, and roadmap rendering.

```bash
ccsm group <session> -g <group> [-r free|<n>]  # assign session to group
ccsm group <session> --clear                    # remove from group
ccsm group <name>                               # overview — members + goal
ccsm group <name> --goal <text>                 # set group goal
ccsm group <name> --roadmap                     # markdown table + mermaid dep graph → stdout
ccsm group --list                               # list all groups in workspace
ccsm next <group>                               # next session to work on (respects deps)
ccsm group-deps <group>                         # ASCII dependency tree
ccsm depend <name> --on <dep>                   # add dependency
ccsm depend <name> --clear                      # clear all dependencies
```

Group detail files live at `.claude/session-group/<name>.md`. Auto-created on first join, auto-deleted when last session leaves.

**Roadmap** (`ccsm group <name> --roadmap`) renders a live markdown document from registry state:
- Markdown table: rank, session, status icon (✓→○!·), goal, scope
- Mermaid `graph TD` if any session has `depends_on`
- Pipeable: `ccsm group sprint-5 --roadmap > ROADMAP.md`
- Always current — reads from registry, never drifts

## Context Budget Rules

- **`ccsm list --summary`** — sub-50 tokens, quickest overview
- **`ccsm list --active --verbose`** — ~80 tokens, full teammate scan with goals + tags
- **`ccsm show <name>`** — ~200 tokens, shows registry + section headlines
- **`ccsm show <name> --section <s>`** — pull one section, save tokens
- **Detail files are for deep work** — read only YOUR session's file + explicit dependencies
- **Protocol files on demand** — read the index above, pull only the protocol that triggered
