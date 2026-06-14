# PTY Spawning and Process Management Lessons

## Shell Alias Resolution: Spawn Through Shell, Not Direct Binary

Symptom:
Spawning `CommandBuilder::new("claude")` misses all user-configured environment variables (API base URL, auth tokens, model settings), causing the spawned process to fail or use wrong configuration.

Cause:
The user's command name (`cds`) is a fish shell function, not a direct binary. Shell functions encapsulate critical runtime configuration (ANTHROPIC_BASE_URL, ANTHROPIC_AUTH_TOKEN, ANTHROPIC_MODEL, etc.) that direct binary execution bypasses.

Fix:
Always resolve user-specified commands before spawning. Use `fish -c 'type <cmd>'` (or equivalent) to check if it's a function, alias, or binary. For functions/aliases, spawn through the shell: `CommandBuilder::new("fish").arg("-c").arg("cds")`. For direct binaries, spawn normally.

Evidence:
2026-06-14 cc-tui Phase 1 — user requested switching from `claude` to `cds` because `cds` is a fish function setting `ANTHROPIC_BASE_URL=https://api.deepseek.com/anthropic` and other env vars. Fixed by spawning `fish -c cds` in the PTY.
