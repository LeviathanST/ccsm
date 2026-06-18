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

### 🔴 On Session RESUME (DO THIS FIRST — before ANY other output)

When you wake up in a resumed session, you do NOT know which ccsm session you belong to.
The registry knows. You MUST discover it before speaking.

```bash
# Fast path: ccsm resume injects CCSM_SESSION into the environment
test -n "$CCSM_SESSION" && ccsm show "$CCSM_SESSION" && return
# Fallback: scan the registry
ccsm list --active          # find the in_progress session → that's you
ccsm show <name>            # load goal, scope, tags, progress log
ccsm show <name> --section progress-log  # what was the last thing done?
```

**Never open with "What are we working on?"** — the session registry already knows.
If there are multiple active sessions, ask which one you're continuing.
If there are zero active sessions, ask the human what to start.

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
ccsm attach <name>              # auto-discover & link live session (--pid <pid> or <uuid> also accepted)
ccsm resume <name>          # spawn claude (--resume if session_id exists)
ccsm sequence -q <cmd> <args...> ...  # batch mutations in single lock/save
ccsm --help                 # full command list
```
### Attach modes (why UUID, PID, and auto-discover)

Claude Code identifies sessions by UUID (e.g. `f493397b-...-4d5f15da0311`).
This UUID is stored in `~/.claude/sessions/<pid>.json` and names the transcript
file at `~/.claude/projects/<slug>/<uuid>.jsonl`. ccsm uses this UUID to link
registry entries to their transcripts so `resume` can pass `--resume <uuid>`.

Three ways to attach, from simplest to most explicit:

| Mode | Command | When |
|---|---|---|
| **Auto-discover** | `ccsm attach <name>` | You're in a live Claude session. ccsm scans session files, prefers name match (from `/rename`), falls back to most recent in workspace. |
| **By PID** | `ccsm attach <name> --pid <pid>` | You know the process ID (from `ps aux \| grep claude`). ccsm reads the session file and harvests the UUID. |
| **By UUID** | `ccsm attach <name> <uuid>` | Scripting, cross-workspace, or when you already have the UUID from `~/.claude/sessions/<pid>.json`. |

Names like "smith-system" are NOT session IDs — they're session names set by
`/rename` or `-n`. ccsm rejects non-UUID strings to prevent the exact bug where
`ccsm attach smith-system smith-system` silently wrote a name where a UUID was expected.

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
- Use `ccsm attach <name>` to link a live Claude session (auto-discover by name match or recency)

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

# ── 2b. Install seed-session skill globally ─────────────────────────────

SEED_SKILL_DIR="${HOME}/.claude/skills/seed-session"
mkdir -p "$SEED_SKILL_DIR"
cp "$PROJECT_DIR/.claude/skills/seed-session/SKILL.md" "$SEED_SKILL_DIR/"
echo "[updated] seed-session skill installed at $SEED_SKILL_DIR"

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

# ── 4. Install scope injection hooks in global settings ──────────────────

GLOBAL_SETTINGS="${HOME}/.claude/settings.json"

install_hooks() {
    # Use jq to merge ccsm hooks into existing settings, preserving all other config.
    if command -v jq &>/dev/null; then
        local tmp
        tmp=$(mktemp)
        jq '
            # Add SessionStart if not already present
            if .hooks.SessionStart then . else
                .hooks.SessionStart = [{
                    "matcher": "",
                    "hooks": [{
                        "type": "command",
                        "command": "ccsm inject-scope 2>/dev/null || true"
                    }]
                }]
            end |
            # Add UserPromptSubmit if not already present
            if .hooks.UserPromptSubmit then . else
                .hooks.UserPromptSubmit = [{
                    "matcher": "",
                    "hooks": [{
                        "type": "command",
                        "command": "ccsm inject-scope 2>/dev/null || true"
                    }]
                }]
            end
        ' "$GLOBAL_SETTINGS" > "$tmp" && mv "$tmp" "$GLOBAL_SETTINGS"
        echo "[updated] Scope injection hooks installed in $GLOBAL_SETTINGS"
    else
        echo "[skipped] jq not found — add hooks manually:"
        echo "  SessionStart + UserPromptSubmit → ccsm inject-scope"
    fi
}

if [ -f "$GLOBAL_SETTINGS" ]; then
    # Ensure .hooks exists
    if command -v jq &>/dev/null; then
        jq '.hooks //= {}' "$GLOBAL_SETTINGS" > "${GLOBAL_SETTINGS}.tmp" \
            && mv "${GLOBAL_SETTINGS}.tmp" "$GLOBAL_SETTINGS" 2>/dev/null || true
    fi
    install_hooks
elif [ "$(uname)" = "Darwin" ] || [ "$(uname)" = "Linux" ]; then
    # Create a fresh settings.json with just the ccsm hooks
    cat > "$GLOBAL_SETTINGS" <<'JSON'
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "ccsm inject-scope 2>/dev/null || true"
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "ccsm inject-scope 2>/dev/null || true"
          }
        ]
      }
    ]
  }
}
JSON
    echo "[created] Fresh settings.json with ccsm scope injection hooks"
fi

echo ""
echo "ccsm setup complete."
echo "  Global CLAUDE.md  ←  session tracking section + CLI reference"
echo "  Global skill      ←  /session-manager"
echo "  Global hooks      ←  SessionStart + UserPromptSubmit (ccsm inject-scope)"
echo "  Workspace registry←  .claude/sessions.json"

# ── 4. Ensure ccsm hooks in global settings.json ─────────────────────────

GLOBAL_SETTINGS="${HOME}/.claude/settings.json"

if command -v jq &>/dev/null; then
    if [ ! -f "$GLOBAL_SETTINGS" ]; then
        echo '{}' > "$GLOBAL_SETTINGS"
    fi

    # Merge ccsm hooks: inject-scope on SessionStart+UserPromptSubmit, note-check on Stop.
    # Only adds if the hook block isn't already present for that event.
    TMP_SETTINGS=$(mktemp)
    jq '
      # SessionStart: inject-scope
      .hooks.SessionStart //= []
      | .hooks.SessionStart |= (
          if any(.[].hooks?[]?.command?; . == "ccsm inject-scope 2>/dev/null || true") then .
          else . + [{
              matcher: "",
              hooks: [{
                type: "command",
                command: "ccsm inject-scope 2>/dev/null || true"
              }]
            }]
          end
        )
      # UserPromptSubmit: inject-scope
      | .hooks.UserPromptSubmit //= []
      | .hooks.UserPromptSubmit |= (
          if any(.[].hooks?[]?.command?; . == "ccsm inject-scope 2>/dev/null || true") then .
          else . + [{
              matcher: "",
              hooks: [{
                type: "command",
                command: "ccsm inject-scope 2>/dev/null || true"
              }]
            }]
          end
        )
      # Stop: note-check
      | .hooks.Stop //= []
      | .hooks.Stop |= (
          if any(.[].hooks?[]?.command?; . == "ccsm note-check 2>/dev/null || true") then .
          else . + [{
              matcher: "",
              hooks: [{
                type: "command",
                command: "ccsm note-check 2>/dev/null || true"
              }]
            }]
          end
        )
    ' "$GLOBAL_SETTINGS" > "$TMP_SETTINGS" && mv "$TMP_SETTINGS" "$GLOBAL_SETTINGS"
    echo "  Global hooks     ←  SessionStart + UserPromptSubmit (inject-scope), Stop (note-check)"
else
    echo "  ⚠ jq not found — skipped hook installation. Install jq and re-run setup."
fi
