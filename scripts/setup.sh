#!/usr/bin/env bash
# cc-tui setup — installs session tracking into the global Claude Code config.
# Run once: ./scripts/setup.sh
set -euo pipefail

GLOBAL_CLAUDE="${HOME}/.claude/CLAUDE.md"
SKILL_DIR="${HOME}/.claude/skills/session-manager"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

MARKER_START="<!-- cc-tui:session-manager:START -->"
MARKER_END="<!-- cc-tui:session-manager:END -->"

SECTION=$(cat <<'CLAUDEMD'

<!-- cc-tui:session-manager:START -->

## 🔴 HIGHEST PRIORITY: Session Registry

**Every session MUST be tracked.** Each project has a `.claude/sessions.json` registry.
cc-tui's sidebar reads this file — empty entries mean the human can't see what's happening.

### On Session START
1. Read `<repo>/.claude/sessions.json` (create if missing)
2. Create or claim an entry with `name`, `goal`, `scope`, `status: "in_progress"`
3. Leave `session_id` and `pids` empty — cc-tui's merge_live_sessions fills them

### On Session END
- Update `status` to `completed`, set `completed` timestamp
- Or `blocked` / `abandoned` if appropriate

### Schema (the fields you control)
```json
{
  "session_id": "",         // AUTO — NEVER touch
  "name": "kebab-case",     // Short label
  "goal": "One sentence",   // What are we doing?
  "scope": "2-4 sentences", // Approach, constraints, what's in/out
  "status": "in_progress",  // pending|in_progress|completed|blocked|abandoned
  "pids": [],               // AUTO — NEVER touch
  "tags": ["tag1", "tag2"],
  "started": "",            // AUTO — NEVER touch
  "completed": ""           // Set when done
}
```

### Rules
- Status lifecycle: `pending → in_progress → completed` (or `blocked`/`abandoned`)
- Only ONE `in_progress` per workspace
- NEVER write to `session_id`, `pids`, or `started` — cc-tui manages those
- NEVER set `trashed` — that's for the human via TUI keybindings

### Team Awareness
- **Before starting:** scan for existing `in_progress` sessions — check if someone already claimed this work
- **Duplicate detected?** Report it. Don't create a competing entry.
- **Dependency?** Note it in `scope`: "Depends on: <session-name> (status: ...)"
- **Subtask?** Join the existing session instead of creating a new one.
- Invoke `/session-manager` for the full protocol, decision flow, and examples

<!-- cc-tui:session-manager:END -->
CLAUDEMD
)

# ── 1. Append session tracking to global CLAUDE.md ────────────────────────

if [ ! -f "$GLOBAL_CLAUDE" ]; then
    echo "No global CLAUDE.md found at $GLOBAL_CLAUDE"
    echo "Creating one..."
    mkdir -p "$(dirname "$GLOBAL_CLAUDE")"
    touch "$GLOBAL_CLAUDE"
fi

if grep -qF "$MARKER_START" "$GLOBAL_CLAUDE"; then
    echo "[skip] Session tracking already in $GLOBAL_CLAUDE"
else
    echo "$SECTION" >> "$GLOBAL_CLAUDE"
    echo "[done] Session tracking appended to $GLOBAL_CLAUDE"
fi

# ── 2. Install session-manager skill globally ─────────────────────────────

if [ -d "$SKILL_DIR" ]; then
    echo "[skip] Skill already installed at $SKILL_DIR"
else
    mkdir -p "$SKILL_DIR"
    cp "$PROJECT_DIR/.claude/skills/session-manager/SKILL.md" "$SKILL_DIR/"
    cp "$PROJECT_DIR/.claude/skills/session-manager/skill.json" "$SKILL_DIR/"
    echo "[done] Skill installed at $SKILL_DIR"
fi

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
echo "cc-tui setup complete."
echo "  Global CLAUDE.md  ←  session tracking section"
echo "  Global skill      ←  /session-manager"
echo "  Workspace registry←  .claude/sessions.json"
