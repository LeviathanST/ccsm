# Lesson Systems: Don't create parallel lesson stores

Symptom:
Implemented a `ccsm lessons` CLI command and `Lesson:` pattern in session detail files,
only to discover `.claude/lessons/` already existed with a structured format
(Symptom → Cause → Fix → Evidence) managed by the `learned-lesson-issue` skill.

Cause:
Didn't check existing skill infrastructure before building. The `learned-lesson-issue`
skill was already installed (via `ccsm setup`) with its own format, directory, and workflow.
Built a parallel system instead of extending the existing one.

Fix:
- Revert the `ccsm lessons` command entirely (git revert + force push)
- Direct agents to `.claude/lessons/` via the session-manager SKILL.md
- Reference `/skill:learned-lesson-issue` for the format and workflow
- Before building any new "lesson" feature, check `.claude/lessons/` and the
  `learned-lesson-issue` skill first

Evidence:
2026-06-25, `centralize-lessons` session. Built `ccsm lessons` CLI + `ccsm_lessons`
Pi tool before noticing `.claude/lessons/INDEX.md` and `session-lifecycle.md` already
existed. Session was force-pushed away.
