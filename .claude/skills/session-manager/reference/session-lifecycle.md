# Session Lifecycle & Resume

## Lifecycle

```
NEW → start → (work → note → work → note → ...) → ccsm close → ccsm complete
                                                       ↓            ↓
                                                   blocked/      --force
                                                   abandoned
                                                   (must note why)

Bloat? → ccsm refresh (retire stale Claude session, spawn fresh — same ccsm session)
```

## Refresh (context bloat rescue)

When the context window fills up and the model gets biased:
```bash
ccsm refresh <name> -r "context at 45%, stuck on auth bug"
```
Moves current session_id to `retired_session_ids`, spawns fresh `claude` without `--resume`. The ccsm session continues — only the Claude session is refreshed.

## Close Gate (before complete)

```bash
ccsm close <name>     # hard checks + self-review checklist
ccsm complete <name>  # auto-runs same gate, refuses unless --force
```

Gate checks: template residue, empty scope/tags, <2 progress notes, hollow Live Session Data.

## How Resume Works

`ccsm resume <name>`:

1. **Spawn**: captures child PID, writes to registry, polls `~/.claude/sessions/<pid>.json` (up to 5s), harvests `sessionId` BEFORE Claude exits
2. **Wait**: blocks on `child.wait()` — Claude runs interactively
3. **Cleanup**: clears stale pids, saves registry
4. **Next resume**: finds session_id → `claude --resume <id> -n <name>`

Session_id is persisted before Claude exits — Claude v2.1+ deletes the session file on graceful exit, so harvesting must happen while the process is alive.

## Statuses

```
pending      — planned, not started yet
in_progress  — actively working on
completed    — finished successfully
blocked      — can't proceed, waiting on a dependency
abandoned    — gave up, no longer relevant
trashed      — soft-deleted, recoverable with `ccsm recover <name>`
```
