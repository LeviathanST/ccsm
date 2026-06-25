---
name: learned-lesson-issue
description: Use when debugging any subsystem in this repo — check prior
  lessons in .claude/lessons/ before investigating, capture new verified
  lessons after fixes.
---

# Learned Lesson Issue

Before debugging anything, scan `.claude/lessons/INDEX.md` or grep the
lessons directory for your symptom. Don't re-debug solved problems.

## Workflow

1. Identify the symptom and the touched subsystem.
2. Scan `lessons/INDEX.md` — match subsystem against your issue.
3. Read the relevant lesson file. Apply a known fix only when the evidence
   matches. If it does not match, say why.
4. After fixing a new issue, append a compact lesson to `.claude/lessons/`:
   - Create a new subsystem file if none exists, and update INDEX.md
   - Append to an existing file if it covers the same subsystem

## Lesson Format

```md
## Short Issue Name

Symptom:
Observable failure.

Cause:
Root cause in one sentence.

Fix:
Concrete action that resolved it.

Evidence:
Date, command, log line, or verification result.
```

## Capture Rules

- Save a lesson only after the cause is verified.
- Prefer one entry per reusable failure mode, not one per session.
- If a lesson is superseded, mark the old entry as superseded and link to the newer entry.

## At Session End

wrap-up (Phase 2) will nudge you to record lessons. That's the push side —
this skill is the pull side (check before debugging). Both use the same
`.claude/lessons/` data store.
