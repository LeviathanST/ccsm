#!/usr/bin/env bash
# ccsm setup — installs session tracking into the global Claude Code config.
# Run once: ./scripts/setup.sh
set -euo pipefail

GLOBAL_CLAUDE="${HOME}/.claude/CLAUDE.md"
SKILL_DIR="${HOME}/.claude/skills/session-manager"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

MARKER_START="<!-- ccsm:session-manager:START -->"
MARKER_END="<!-- ccsm:session-manager:END -->"

SECTION=$(cat <<'CLAUDEMD'

<!-- ccsm:session-manager:START -->

## 🔴 HIGHEST PRIORITY: Session Registry

**Every session MUST be tracked.** Each project has a `.claude/sessions.json` registry.
ccsm reads this file — empty entries mean no session context is recorded.

### On Session START
1. Read `<repo>/.claude/sessions.json` (create if missing)
2. Create or claim an entry with `name`, `goal`, `scope`, `status: "in_progress"`
3. Leave `session_id` and `pids` empty — ccsm manages those automatically

### On Session END
- Update `status` to `completed`, set `completed` timestamp
- Or `blocked` / `abandoned` if appropriate

### Schema (the fields you control)
```json
{
  "session_id": "",         // AUTO — ccsm manages this
  "name": "kebab-case",     // Short label
  "goal": "One sentence",   // What are we doing?
  "scope": "2-4 sentences", // Approach, constraints, what's in/out
  "status": "in_progress",  // pending|in_progress|completed|blocked|abandoned
  "pids": [],               // AUTO — ccsm manages this
  "tags": ["tag1", "tag2"],
  "started": "",            // AUTO — ccsm manages this
  "completed": ""           // Set when done
}
```

### CLI Quick Reference
```bash
ccsm list                  # all sessions
ccsm list --active          # in_progress + blocked
ccsm list --summary         # counts only
ccsm show <name>            # full detail
ccsm show <name> --section <s>  # extract one section from detail file
ccsm new <name> -g <goal>  # create entry
ccsm start <name>           # promote to in_progress
ccsm complete <name>        # mark done
ccsm block <name>           # mark blocked
ccsm abandon <name>         # mark abandoned
ccsm pending <name>         # reset to pending
ccsm scope <name> <text>    # set scope
ccsm tag <name> <tags...>   # set tags
ccsm note <name> <text>     # append to progress log
ccsm attach <name> <sid>    # link session_id
ccsm resume <name>          # spawn claude (--resume if session_id exists)
ccsm sequence -q <cmd> <args...> ...  # batch mutations in single lock/save
ccsm --help                 # full command list
```

### Session Lifecycle
```
NEW → start → (work → note → note → ...) → END-GATE → complete
                                                 ↓
                                             blocked/abandoned
```

### Rules
- Status lifecycle: `pending → in_progress → completed` (or `blocked`/`abandoned`)
- Only ONE `in_progress` per workspace
- Use `ccsm` CLI to mutate — never edit JSON directly
- **`ccsm note <name> <text>` after every non-trivial change** — progress log is mandatory
- **Before `ccsm complete`:** answer the END-GATE: what was built? what was NOT done? what's left?
- Use `ccsm attach <name> <session-id>` to link an existing Claude session

### Team Awareness
- **Before starting:** `ccsm list --active` — check if someone already claimed this work
- **Duplicate detected?** Report it. Don't create a competing entry.
- **Dependency?** Note it in `scope`: "Depends on: <session-name> (status: ...)"
- **Subtask?** Join the existing session instead of creating a new one.
- Invoke `/session-manager` for the full protocol, decision flow, and examples

<!-- ccsm:session-manager:END -->
CLAUDEMD
)

# ── 1. Upsert session tracking section in global CLAUDE.md ─────────────

if [ ! -f "$GLOBAL_CLAUDE" ]; then
    echo "No global CLAUDE.md found at $GLOBAL_CLAUDE"
    echo "Creating one..."
    mkdir -p "$(dirname "$GLOBAL_CLAUDE")"
    touch "$GLOBAL_CLAUDE"
fi

if grep -qF "$MARKER_START" "$GLOBAL_CLAUDE"; then
    # Section exists — strip old and replace with current version.
    # sed range delete: from MARKER_START line through MARKER_END line.
    sed -i "/$MARKER_START/,/$MARKER_END/d" "$GLOBAL_CLAUDE"
    # Remove trailing blank lines left by deletion.
    sed -i '${/^$/d}' "$GLOBAL_CLAUDE"
    echo "$SECTION" >> "$GLOBAL_CLAUDE"
    echo "[updated] Session tracking section replaced in $GLOBAL_CLAUDE"
else
    echo "$SECTION" >> "$GLOBAL_CLAUDE"
    echo "[done] Session tracking appended to $GLOBAL_CLAUDE"
fi

# ── 2. Install/update session-manager skill globally ───────────────────

mkdir -p "$SKILL_DIR"
cp "$PROJECT_DIR/.claude/skills/session-manager/SKILL.md" "$SKILL_DIR/"
cp "$PROJECT_DIR/.claude/skills/session-manager/skill.json" "$SKILL_DIR/"
echo "[updated] Skill installed at $SKILL_DIR"

# ── 3. Create a minimal .claude/sessions.json if none exists ──────────────

WORKSPACE_REGISTRY="${PROJECT_DIR}/.claude/sessions.json"
if [ ! -f "$WORKSPACE_REGISTRY" ]; then
    cat > "$WORKSPACE_REGISTRY" <<'JSON'
{
  "updated": "",
  "sessions": []
}
JSON
    echo "[done] Created empty registry at $WORKSPACE_REGISTRY"
fi

echo ""
echo "ccsm setup complete."
echo "  Global CLAUDE.md  ←  session tracking section + CLI reference"
echo "  Global skill      ←  /session-manager"
echo "  Workspace registry←  .claude/sessions.json"
