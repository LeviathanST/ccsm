# OpenCode Session Timing

## Symptom

`ccsm resume` with OpenCode consumer always reported "could not detect new opencode session in DB within 5s". The session_id was never harvested, so every resume started fresh.

## Cause

OpenCode creates its `session` row in SQLite **lazily** — on first user message, not at spawn time. Claude creates its PID-based session file eagerly at spawn. The 5-second harvest poll after spawn always timed out because no session existed yet.

## Fix

Defer the OpenCode harvest to **after the child exits** (`Phase 6b`). By then, the user has interacted with the agent and the session row exists. Use `opencode_find_session_since` (single SQL query, no polling) instead of `opencode_harvest_session` (50 retries × 100ms).

## Evidence

PR #22, commit `c16117d` (`src/commands/resume.rs`). Verified end-to-end in tmux: fresh harvest + resume with title persistence both work.
