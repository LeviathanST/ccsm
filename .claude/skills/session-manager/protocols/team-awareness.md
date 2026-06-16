# Team Awareness

**When to read:** Session START. You're about to create or claim a session entry.

## Before Starting ANY Work

1. `ccsm list --summary` — quick counts
2. `ccsm list --active` — who's working?
3. Ask:

| Question | What to do |
|---|---|
| **Duplicate?** | STOP. Report: "Session X is already doing this." |
| **Dependency?** | Note in scope: "Depends on: <name> (status: ...)" |
| **Subtask?** | Join existing session, don't create new one |
| **No overlap?** | Create entry. Proceed. |

## Decision Flow

```
ccsm list --active
    │
    ├─→ Duplicate?     → STOP. Help that session or narrow scope.
    ├─→ Depends on?    → NOTE in scope. Check status before claiming done.
    └─→ No overlap?    → Create entry. Proceed.
```
