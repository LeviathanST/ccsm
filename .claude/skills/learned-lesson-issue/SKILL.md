---
name: learned-lesson-issue
description: Use in this repo when debugging session lifecycle, registry, PTY spawn, or resume issues; check prior lessons before investigating and capture new verified lessons after fixes.
metadata:
  short-description: cc-tui recurring issue lessons
---

# Learned Lesson Issue

Use this before debugging session resume, registry state, PTY lifecycle, or spawn issues that have failed before in `cc-tui`.

## Workflow

1. Identify the symptom and the touched subsystem.
2. Open the relevant reference:
   - Session/registry/resume/PTY: `references/session-lifecycle.md`
3. Match the current symptom against known lessons.
4. Apply a known fix only when the evidence matches. If it does not match, say why.
5. After fixing a new issue, append a compact lesson to the relevant reference:
   - Symptom
   - Cause
   - Fix
   - Evidence

Keep entries short. Do not paste full logs; include the deciding evidence.

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
