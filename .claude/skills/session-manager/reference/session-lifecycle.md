# Session Lifecycle & Resume

## Lifecycle

```
NEW → start → (scope → checklist → work → note → ...) → ccsm close → ccsm complete
                                                           ↓            ↓
                                                       blocked/      --force
                                                       abandoned
                                                       (must note why)

Complex? → ccsm new -c — embed ## Checklist section. ccsm check to add/toggle items.
Bloat? → ccsm refresh (retire stale agent session, spawn fresh — same ccsm session)
```

## Refresh (context bloat rescue)

When the context window fills up and the model gets biased:
```bash
ccsm refresh <name> -r "context at 45%, stuck on auth bug"
```
Moves current session_id to `retired_session_ids`, spawns fresh agent without `--resume`. The ccsm session continues — only the agent session is refreshed.

## Close Gate (before complete)

```bash
ccsm close <name>     # hard checks + self-review checklist
ccsm complete <name>  # auto-runs same gate, refuses unless --force
```

Gate checks: template residue, empty scope/tags, <2 progress notes, hollow Live Session Data.

## How Resume Works

`ccsm resume <name>`:

1. **Spawn**: captures child PID, writes to registry, polls for session_id (up to 5s), harvests BEFORE agent exits
2. **Wait**: blocks on `child.wait()` — agent runs interactively
3. **Cleanup**: clears stale pids, saves registry
4. **Next resume**: finds session_id → `opencode -s <id> -n <name>`

Session_id is persisted before the agent exits — most agents delete their session file on graceful exit, so harvesting must happen while the process is alive.

## Statuses

```
pending      — planned, not started yet
in_progress  — actively working on
completed    — finished successfully
blocked      — can't proceed, waiting on a dependency
abandoned    — gave up, no longer relevant
trashed      — soft-deleted, recoverable with `ccsm recover <name>`
```
