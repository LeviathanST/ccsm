# Session Lifecycle & Resume

## Lifecycle

```
NEW → start → (work → note → work → note → ...) → END-GATE → complete
                                                       ↓
                                                   blocked/abandoned
                                                   (must note why)
```

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
