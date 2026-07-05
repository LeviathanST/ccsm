# `-g ` Flag Leaks Into Goal Field

**Symptom:** `ccsm doctor` reports "cli artifact in goal" — the goal starts with `-g `. This also appears in `ccsm show` and `ccsm scan` output, polluting the goal text.

**Cause:** `ccsm sequence -q new <name> -g "goal"` stores the literal `-g ` as part of the goal value. The same happens with `ccsm new <name> -g "goal"` under certain shell parsing conditions. The goal field in `sessions.json` contains `"-g goal text"` instead of `"goal text"`.

**Fix (workaround):** Two-step rename clears the artifact:
```bash
ccsm rename <name> fix-temp-<name>
ccsm rename fix-temp-<name> <name> -g "<correct goal>"
```

**Evidence:**
- `ccsm show` shows `goal: -g Install CodeWhale...`
- Raw JSON inspection in `.ccsm/sessions.json` confirms `"goal": "-g goal text"`
- 3 sessions affected in the registry: `ccsm-audit-and-vision`, `session-group`, and our new sessions created via `sequence`

**Prevention:** Add validation in `ccsm new` to strip or reject `-g ` prefix in the goal value. Session `agent-first-enforce` (rank 6 in agent-first group) targets this.
