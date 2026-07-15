# Lessons Index

| Subsystem | File | Description | Updated |
|-----------|------|-------------|---------|
| Session Lifecycle | session-lifecycle.md | Registry, PTY spawn, resume, session_id harvesting | 2026-06-15 |
| Lesson Systems | lesson-systems.md | Don't create parallel lesson stores — use .claude/lessons/ | 2026-06-25 |
| Agent-First Docs | agent-first-docs.md | Decision tree + token budget table pattern for agent-facing docs | 2026-07-05 |
| Structured Errors | structured-errors.md | ErrorCode enum for agent-parsable CLI error codes | 2026-07-05 |
| CI/CD | ci-publish-tag-guard.md | Guard publish jobs to only run on tags that are ancestors of main | 2026-07-14 |
| CI/CD | github-secrets-environment-vs-repo.md | Environment secrets require `environment:` in job — use repo-level secrets for general CI | 2026-07-14 |
| OpenCode | opencode-session-timing.md | OpenCode creates session rows lazily (on first message), not at spawn — defer harvest to after child exit | 2026-07-15 |
| Migration | migration-version-check.md | run_identity_migrations should warn, not hard-block, on binary > project — safety guard is check_version() in main.rs | 2026-07-15 |
