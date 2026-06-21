# Registry Schema + Session Detail Files

## Registry Schema

`.claude/sessions.json` at the workspace root:

```json
{
  "updated": "day20618T08:25Z",
  "sessions": [
    {
      "session_id": "",        // AUTO — ccsm manages this
      "name": "my-feature",    // MANUAL — kebab-case label
      "goal": "Add X to Y",    // MANUAL — one sentence
      "scope": "Details...",   // MANUAL — 2-4 sentences: approach, constraints, in/out
      "status": "in_progress", // MANUAL — pending|in_progress|completed|blocked|abandoned|trashed
      "pids": [],              // AUTO — ccsm manages this
      "tags": ["ui", "pty"],   // MANUAL — lowercase tags
      "started": "",           // AUTO — ccsm manages this
      "completed": "",          // MANUAL — set when status → completed
      "group": {               // MANUAL — optional group assignment
        "name": "group-name",  //   kebab-case group identifier
        "rank": "free"         //   "free" or number (lower = higher priority)
      },
      "depends_on": []         // MANUAL — session names that must complete first
    }
  ]
}
```

## Field Rules

| Field | Who | When |
|-------|-----|------|
| `session_id` | **ccsm** — NEVER touch | Harvested from `~/.claude/sessions/<pid>.json` on exit. Use `ccsm attach` to set manually |
| `pids` | **ccsm** — NEVER touch | Set at spawn, cleared on exit |
| `started` | **ccsm** — NEVER touch | Set on first spawn |
| `name`, `goal`, `scope`, `tags` | **You** | On session create, refine as needed |
| `status` | **You** | Update as work progresses |
| `completed` | **You** | When status → completed |
| `group` | **You** | Assign via `ccsm group <name> -g <group>`, clear via `--clear` |
| `depends_on` | **You** | Manage via `ccsm depend <name> --on <dep>` / `--clear` |

## Session Lifecycle

```
NEW → start → (work → note → work → note → ...) → END-GATE → complete
                                                       ↓
                                                   blocked/abandoned
                                                   (must note why)
```

## Session Detail Files

Detail files live at `.claude/sessions/<name>.md`. `ccsm new` auto-creates them from the template — your job is to **fill them in**, not create them.

```bash
ccsm show <name>          # check what's already filled
# Then Edit .claude/sessions/<name>.md to replace remaining {{placeholders}}
```

**Token-efficient reading:**
```bash
ccsm show <name>                    # headlines + line counts
ccsm show <name> --section progress-log   # just one section
ccsm show <name> --section dependencies   # just one section
```

Sections: `goal`, `scope-plan` (or `scope / plan`), `tags`, `live-session-data`, `progress-log`, `dependencies`, `notes`.

## When to update

| Trigger | Action |
|---|---|
| Session created | Copy template, fill ALL sections |
| Status changes | Update status badge line |
| ANY work done | `ccsm note <name> "<what + why>"` |
| New dependency | Add to Dependencies |
| Discovery | Add to Notes |
| Session completed | END-GATE note first, then `ccsm complete` |
