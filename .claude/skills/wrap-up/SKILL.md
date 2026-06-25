---
name: wrap-up
description: Use when user says "wrap up", "close session", "end session",
  "wrap things up", "close out this task", or invokes /wrap-up — runs
  end-of-session Ship + Learn checklist
---

# Session Wrap-Up

Two phases. Run in order. Present results at the end.

## Phase 1: Ship

**Git:**
1. `git status` — if uncommitted changes, commit with descriptive message
2. Push to remote

**ccsm end-gate** *(if this is a ccsm project: `ccsm --version` succeeds + `.claude/sessions.json` exists):*
3. `ccsm doctor` — detect template residue, empty fields, stale locks
4. Fill any remaining placeholder fields (timestamps, scope, tags)
5. `ccsm close <name>` — pre-completion gate
6. `ccsm note <name> "<what was built, what was NOT done, what's left>"`
7. `ccsm complete <name>` (use `--force` only if template residue is intentional)

**Task cleanup:**
8. Mark completed tasks as done, flag orphaned ones

## Phase 2: Learn

Recheck the session context. Did you fix a bug that matches a reusable failure mode?
Did you discover a project fact, pattern, or gotcha worth keeping?

→ Record to `.claude/lessons/` using the format in `/learned-lesson-issue` skill
  (Symptom → Cause → Fix → Evidence). Update `lessons/INDEX.md` if new file added.

→ For project conventions or rules, update the appropriate skill or CLAUDE.md.
