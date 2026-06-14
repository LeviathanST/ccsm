---
name: learned-lesson-issue
description: Use in this repo when debugging or changing PTY, VT100 rendering, ratatui layout, or process spawning; check prior lessons before investigating and capture new verified lessons after fixes.
metadata:
  short-description: Repo-local recurring issue lessons for cc-tui
---

# Learned Lesson Issue

Use this before debugging PTY embedding, terminal rendering, process spawning, or any fragile workflow that has failed before in `cc-tui`.

## Workflow

1. Identify the symptom, command, platform, and touched subsystem.
2. Open only the relevant reference:
   - PTY/ratatui/vt100 rendering: `references/pty-rendering.md`
   - PTY spawning/process management: `references/pty-spawning.md`
3. Match the current symptom against known lessons.
4. Apply a known fix only when the evidence matches. If it does not match, say why.
5. After fixing a new issue, append a compact lesson to the relevant reference:
   - Symptom
   - Cause
   - Fix
   - Evidence

Keep entries short. Do not paste full logs; include the deciding log line or command result.

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
- If the issue belongs in code comments, tests, or docs instead, update those too; this skill is not a replacement for project truth.
