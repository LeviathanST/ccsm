# CodeWhale Consumer — Session Model & CLI Reference

> CodeWhale v0.8.66 — Open-source terminal coding agent (Rust TUI/CLI)
> Source: [github.com/Hmbown/CodeWhale](https://github.com/Hmbown/CodeWhale)

## Quick Summary

| Property | Value |
|----------|-------|
| Binary | `codewhale` (also `codew`, `codewhale-tui`) |
| Home config dir | `~/.codewhale/` (legacy: `~/.deepseek/`) |
| Session storage | `~/.codewhale/sessions/<uuid>.json` (JSON + crash recovery checkpoints) |
| State DB | `~/.codewhale/state.db` (SQLite — threaded/durable sessions) |
| Session index | `~/.codewhale/session_index.jsonl` (append-only JSONL alongside state.db) |
| Resume flags | `--resume <id>`, `--session-id <id>`, `--continue` (most recent in workspace), `--fresh` (skip checkpoint) |
| Env override | `CODEWHALE_HOME` — hard override of `~/.codewhale` |
| Inject scope | Project `.codewhale/constitution.json` + `exec --append-system-prompt` |
| Spawn behavior | Plain `codewhale` → TUI (fresh start); TUI is interactive, needs raw terminal |

## Config Directory Layout

### User-global (`~/.codewhale/`)

| Path | Purpose |
|------|---------|
| `config.toml` | TOML config — provider, model, API key reference, profiles |
| `secrets/secrets.json` | JSON — `{ entries: { "<provider>": "<api_key>" } }` |
| `state.db` | SQLite database — threads, messages, checkpoints, jobs (created lazily on first TUI run) |
| `session_index.jsonl` | Append-only JSONL index alongside state.db (one JSON object per session) |
| `sessions/<uuid>.json` | Full session JSON — messages, metadata, system prompt |
| `sessions/checkpoints/latest.json` | Crash-recovery checkpoint (in-flight turn snapshot) |
| `sessions/checkpoints/offline_queue.json` | Offline/degraded mode queue state |
| `skills/` | Loadable skill definitions |
| `constitution.json` | User-global constitution (structured, advisory prose only) |
| `prompts/constitution.md` | Expert base-prompt override (opt-in, replaces bundled constitution) |

### Project-level (`<workspace>/.codewhale/`)

| Path | Purpose |
|------|---------|
| `config.toml` | Project-level config overlay |
| `constitution.json` | Project law — authority, protected invariants, enforcement rules |
| `fleet.jsonl` | Fleet runs append-only ledger |
| `hooks.toml` | Pre/post tool execution lifecycle hooks |

## Session Storage Format

### Primary: `~/.codewhale/sessions/<uuid>.json`

Full conversation history stored as pretty-printed JSON:

```json
{
  "schema_version": 1,
  "metadata": {
    "id": "uuid...",
    "title": "Session title from first message",
    "created_at": "2026-07-05T12:00:00Z",
    "updated_at": "2026-07-05T13:00:00Z",
    "message_count": 42,
    "total_tokens": 15000,
    "model": "deepseek-v4-pro",
    "workspace": "/home/user/project",
    "mode": "agent",
    "cost": {
      "session_cost_usd": 0.15,
      "session_cost_cny": 0.0,
      "subagent_cost_usd": 0.0,
      "subagent_cost_cny": 0.0,
      "displayed_cost_high_water_usd": 0.15,
      "displayed_cost_high_water_cny": 0.0
    },
    "parent_session_id": null,
    "forked_from_message_count": null,
    "cumulative_turn_secs": 120
  },
  "messages": [
    {
      "role": "user",
      "content": [
        {
          "type": "text",
          "text": "Hello!"
        }
      ]
    },
    {
      "role": "assistant",
      "content": [
        {
          "type": "text",
          "text": "Hi there!"
        }
      ]
    }
  ],
  "system_prompt": "Optional system prompt text...",
  "context_references": [],
  "artifacts": []
}
```

**Key data types in messages:**

Content blocks follow a `{ type, text | id | name | input | ... }` pattern:
- `text` — plain text content
- `tool_use` — tool call (id, name, input)
- `tool_result` — tool result (tool_use_id, content, is_error)
- `thinking` — reasoning content
- `redacted_thinking` — signature-based reasoning

### SQLite: `~/.codewhale/state.db` (threaded/durable sessions)

Used by the `crates/state` module for durable thread persistence with tree-structured messages.

**Tables:**

| Table | Columns | Purpose |
|-------|---------|---------|
| `threads` | `id TEXT PK`, `rollout_path TEXT`, `preview TEXT`, `ephemeral INT`, `model_provider TEXT`, `created_at INT`, `updated_at INT`, `status TEXT`, `path TEXT`, `cwd TEXT`, `cli_version TEXT`, `source TEXT`, `title TEXT`, `sandbox_policy TEXT`, `approval_mode TEXT`, `archived INT`, `archived_at INT`, `git_sha TEXT`, `git_branch TEXT`, `git_origin_url TEXT`, `memory_mode TEXT` | Conversation thread metadata |
| `thread_dynamic_tools` | `thread_id TEXT`, `position INT`, `name TEXT`, `description TEXT`, `input_schema TEXT` | Per-thread dynamic tool registrations |
| `messages` | `id INT PK AUTOINCREMENT`, `thread_id TEXT`, `role TEXT`, `content TEXT`, `item_json TEXT`, `created_at INT` | Append-only messages with optional structured payload |
| `checkpoints` | `thread_id TEXT`, `checkpoint_id TEXT`, `state_json TEXT`, `created_at INT` | Named state snapshots for restore |
| `jobs` | `id TEXT PK`, `name TEXT`, `status TEXT`, `progress INT`, `detail TEXT`, `created_at INT`, `updated_at INT` | Background task tracking |

**Thread status values:** `running`, `idle`, `completed`, `failed`, `paused`, `archived`
**Session source values:** `interactive`, `resume`, `fork`, `api`, `unknown`

### Session Index: `session_index.jsonl`

Append-only JSONL file alongside state.db. One JSON object per session, indexed for fast listing without querying the DB.

### Fleet Ledger: `.codewhale/fleet.jsonl`

Append-only JSONL per project. Each worker records a typed receipt: `pass`, `fail`, `partial`, `skip`, `timeout`.

### Checkpoints (crash recovery)

Written to `~/.codewhale/sessions/checkpoints/latest.json`:
- Saved before every turn in interactive TUI
- Cleared on successful session save
- On resume with `--continue`, restored from checkpoint (not full session save)
- `--fresh` skips checkpoint recovery

## CLI Flags Reference

### Top-level flags

| Flag | Description |
|------|-------------|
| `-r, --resume <SESSION_ID>` | Resume session by UUID or prefix |
| `--session-id <SESSION_ID>` | Alias for `--resume` |
| `-c, --continue` | Continue most recent session for workspace |
| `--fresh` | Start fresh, ignore crash-recovery checkpoint |
| `--yolo` | Auto-approve all tools (YOLO mode) |
| `--skip-onboarding` | Skip onboarding screens |
| `--no-project-config` | Skip project-level `.codewhale/config.toml` |
| `-C, --workspace <DIR>` | Workspace directory |
| `--config <FILE>` | Config file path |
| `--profile <NAME>` | Config profile name |
| `--provider <PROVIDER>` | Provider override (deepseek, anthropic, openai, etc.) |
| `--model <MODEL>` | Model override |
| `--approval-policy <POLICY>` | Tool approval policy |
| `--sandbox-mode <MODE>` | Sandbox mode |
| `--output-mode <MODE>` | Verbosity (normal, concise) |
| `--telemetry <BOOL>` | Telemetry toggle |

### Subcommands

| Command | Description |
|---------|-------------|
| `codewhale` | Launch TUI (interactive, requires raw terminal) |
| `codewhale exec` | Non-interactive prompt |
| `codewhale exec --auto` | Tool-backed agent mode with auto-approvals |
| `codewhale resume <ID>` | Resume a saved TUI session |
| `codewhale resume --last` | Resume the most recent session |
| `codewhale fork <ID>` | Fork a saved session (copy messages up to current) |
| `codewhale fork --last` | Fork the most recent session |
| `codewhale sessions` | List all saved sessions |
| `codewhale thread list` | List threads in the SQLite state DB |
| `codewhale thread read <ID>` | Read thread metadata |
| `codewhale thread resume <ID>` | Resume a thread |
| `codewhale thread fork <ID>` | Fork a thread |
| `codewhale thread archive <ID>` | Archive a thread |
| `codewhale thread unarchive <ID>` | Unarchive a thread |
| `codewhale thread set-name <ID> <NAME>` | Set a thread's display name |
| `codewhale doctor` | Run diagnostics |
| `codewhale run` | Run interactive/non-interactive via TUI binary |
| `codewhale fleet run` | Launch Fleet multi-worker run |
| `codewhale fleet status` | Report Fleet run state |
| `codewhale fleet resume <ID>` | Resume interrupted Fleet run |
| `codewhale execpolicy` | Execution policy tooling |
| `codewhale config get/set/list/path/unset` | Config management |
| `codewhale mcp` | Manage MCP servers |
| `codewhale serve` | Run local server mode |
| `codewhale app-server` | HTTP/SSE runtime API |
| `codewhale sandbox` | Evaluate sandbox/approval policy |
| `codewhale metrics` | Usage rollup report |
| `codewhale review` | Code review over git diff |
| `codewhale apply` | Apply patch file |
| `codewhale update` | Check for and apply updates |

### `exec` flags (headless mode)

| Flag | Description |
|------|-------------|
| `--auto` | Tool-backed agent with auto-approvals |
| `--resume <SESSION_ID>` | Resume by ID |
| `--session-id <SESSION_ID>` | Alias for `--resume` |
| `--continue` | Continue most recent session for workspace |
| `--allowed-tools <LIST>` | Comma-separated allowlist |
| `--disallowed-tools <LIST>` | Comma-separated denylist (deny wins) |
| `--max-turns <N>` | Turn limit for headless run |
| `--append-system-prompt <TEXT>` | Extra text appended to system prompt |
| `--json` | Emit summary JSON |
| `--output-format <FORMAT>` | `text` or `stream-json` |

## Resume / Continue / Spawn Behavior

### Resume flows

| Scenario | How it works |
|----------|--------------|
| `codewhale` (plain) | Fresh interactive TUI. Ignores crash-recovery checkpoints but preserves them for explicit `--continue` |
| `codewhale --continue` / `-c` | Resumes most recent session in workspace. Restores from checkpoint if available, otherwise from latest session save |
| `codewhale --resume <id>` | Resume specific session by UUID or prefix |
| `codewhale --session-id <id>` | Same as `--resume` |
| `codewhale --fresh` | Fresh start, explicitly discarding any crash-recovery checkpoint |
| `codewhale resume <id>` | Same as `--resume` but as subcommand |
| `codewhale resume --last` | Resume most recent session |
| `codewhale exec --resume <id>` | Headless resume |
| `codewhale exec --continue` | Headless continue (errors if no session found) |
| `codewhale fleet resume <id>` | Resumes interrupted Fleet run (idempotent — replays ledger) |

### Fork behavior

`codewhale fork <id>` creates a new SavedSession with:
- Same messages up to current point
- `parent_session_id` set to original session ID
- `forked_from_message_count` recorded
- Cost fields copied from parent

### Session workspace scoping

Sessions are scoped to the git repository root of the workspace where they were created. `--continue` finds the most recent session in the current workspace's git repo. The `sessions` picker in TUI defaults to workspace-scoped view (press `a` for all workspaces).

### Crash recovery

1. Before sending user input, TUI writes checkpoint to `sessions/checkpoints/latest.json`
2. On normal exit, checkpoint is cleared
3. On crash/restart, `--continue` restores from checkpoint
4. `--fresh` or plain `codewhale` preserves checkpoint (for explicit recovery) but starts fresh

## Inject-Scope → Constitution Mapping

CodeWhale has a multi-layered constitution system. Here's how ccsm's `inject-scope` maps to it:

### Layer priority (highest to lowest)

1. **Base myth** — built-in `constitution.md` (the "CONSTITUTION OF CODEWHALE" preamble)
2. **User-global constitution** — `$CODEWHALE_HOME/constitution.json` (structured, advisory only)
3. **Repo constitution** — `<workspace>/.codewhale/constitution.json` (can be mechanically enforced)
4. **Project instructions** — `AGENTS.md`, `CLAUDE.md`, or other project docs
5. **Current user request**
6. **Memory/handoffs**

### Project constitution schema (`.codewhale/constitution.json`)

```json
{
  "schema_version": 1,
  "authority": ["AGENTS.md", "CONTRIBUTING.md"],
  "protected_invariants": [
    "Prose-only advisory rule (no mechanical enforcement)",
    {
      "text": "The wire format is frozen",
      "paths": ["crates/protocol/**"],
      "action": "block"
    },
    {
      "text": "Release notes need human review",
      "paths": ["CHANGELOG.md", "RELEASE_NOTES.md"],
      "action": "ask"
    }
  ],
  "branch_policy": "PRs target main",
  "verification_policy": {
    "before_claiming_done": [
      "Run cargo test",
      "Check for clippy warnings"
    ]
  },
  "escalate_when": [
    "A protected file would be modified by the agent"
  ]
}
```

**Rules:**
- `authority` (array of strings): Ordered list of sources to trust when conflicts arise (highest first)
- `protected_invariants` (array of string or object):
  - **String**: Advisory prose only — rendered into prompt, no mechanical enforcement
  - **Object**: `{ text, paths, action }` — compiled into write holds:
    - `text`: Rule description
    - `paths`: Glob patterns (workspace-relative)
    - `action`: `"ask"` (force-prompt in all modes, even YOLO) or `"block"` (deny outright)
    - Default action: `"ask"`
- `branch_policy`, `verification_policy`, `escalate_when`: Advisory prose blocks
- Law can only ADD holds (tighten), never remove them — no "allow" shape exists

### How to inject scope

For **session-level** scope injection (ccsm's `inject-scope` command):

**Recommended approach:** Use `--append-system-prompt` with `<system-reminder>` tags:

```bash
codewhale --continue --append-system-prompt "<system-reminder>
Session scope: Install CodeWhale and document its session model.
Constraint: Work in a branch off main.
</system-reminder>"
```

This is equivalent to ccsm's current Claude/Pi injection strategy.

For **project-level** standing scope, write `.codewhale/constitution.json` with the scope as a `protected_invariant`. This is the CodeWhale-native equivalent of ccsm's `inject-scope` permanent mode.

### Render format

The project constitution is rendered into a `<codewhale_repo_constitution>` block in the system prompt:

```xml
<codewhale_repo_constitution source="/workspace/.codewhale/constitution.json">
CodeWhale-specific repo authority policy (local law)...

When local sources conflict, trust them in this order (highest first):
1. AGENTS.md

Protected invariants — do not break:
- The wire format is frozen (mechanically enforced for: crates/protocol/**)
- Release notes need human review (mechanically enforced for: CHANGELOG.md)
</codewhale_repo_constitution>
```

## Session UUID Harvesting

CodeWhale uses UUID v4 for session IDs. For auto-attach/discovery:

### Method 1: Recent session file (simplest)

```bash
# List saved sessions, take the most recent one for this workspace
codewhale sessions                    # human-readable list
codewhale thread list --all           # all threads in state.db
```

The session UUID is the filename stem in `~/.codewhale/sessions/<uuid>.json`.

### Method 2: For exec (headless) sessions

`exec` mode with `--auto` does **not** create saved session files by default. To use resumable exec:
```bash
codewhale exec --resume <uuid> --auto "prompt..."
```

### Method 3: Thread mode

The newer threaded system stores sessions in SQLite `state.db`. Thread IDs are the primary key in the `threads` table:
```bash
codewhale thread list
codewhale thread resume <thread-id>
```

## Attach / Auto-Discovery

For ccsm's `attach` command:

| Approach | How | Status |
|----------|-----|--------|
| PID-based polling | CodeWhale doesn't write PID-based session files like Claude | ❌ Not available |
| Recent session scan | List `~/.codewhale/sessions/*.json` sorted by metadata.updated_at | ✅ Works |
| `codewhale sessions` CLI | Parses the `sessions` command output | ✅ Works |
| Thread listing | `codewhale thread list --all` for SQLite-backed sessions | ✅ Works |
| `/workspace` scoping | Sessions store `workspace` in metadata — can match by workspace path | ✅ Works |

## Known Quirks & Differences from Claude/Pi

1. **No PID-based session files.** CodeWhale doesn't write `~/.codewhale/sessions/<pid>.json`. Session UUIDs must be harvested from saved session filenames or the thread list. `live_session_file()` → `None`.

2. **`exec` mode doesn't save sessions.** Unlike Claude's persistent interactive mode, `exec --auto` creates no saved session entry. Sessions are only saved during interactive TUI usage.

3. **Two session storage systems.** CodeWhale has both the legacy JSON session files (`sessions/<uuid>.json`) and the newer SQLite state DB (`state.db` with `threads` table). The SQLite system is the preferred path for new development.

4. **`--continue` scopes to workspace git root.** The most recent session is matched by git repository root, not by directory path. Sessions created in a subdirectory of a repo count as "for this workspace."

5. **`--fresh` vs plain launch.** Plain `codewhale` preserves crash-recovery checkpoints for explicit recovery but starts fresh. `--fresh` explicitly tells CodeWhale you want no recovery — it both clears and ignores checkpoints.

6. **Constitution is not system prompt.** CodeWhale has a dedicated constitution system that's distinct from the agent's system prompt. ccsm's `<system-reminder>` injection still works via `--append-system-prompt`.

7. **Project slug format.** CodeWhale uses the workspace path directly (no slugification like Pi's `--...--` wrapping). The `project_slug()` method should match how CodeWhale scopes sessions internally.

8. **Binary resolution order.** `codewhale` tries the bundled release binary first, then `PATH`. The npm-installed binary lives at `~/.local/share/codewhale/bin/codewhale` by default.

## Spawn Args Reference

```rust
// Fresh interactive session
"codewhale" ["--skip-onboarding"]

// Fresh with scope injection  
"codewhale" ["--skip-onboarding", "--append-system-prompt", "<system-reminder>...</system-reminder>"]

// Resume by ID
"codewhale" ["--resume", "<uuid>"]

// Continue most recent
"codewhale" ["--continue"]

// Headless exec with resume
"codewhale" ["exec", "--resume", "<uuid>", "--auto", "prompt text"]

// Headless exec with continue
"codewhale" ["exec", "--continue", "--auto", "prompt text"]

// Fork a session
"codewhale" ["fork", "<uuid>"]
```
