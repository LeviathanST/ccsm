# ccsm Pi Extension

Registers all [ccsm](https://github.com/user/ccsm) session management operations as native Pi tools.

## How It Works

When Pi runs in this workspace, it auto-discovers `.pi/extensions/ccsm/index.ts` and loads 20+ custom tools. Each tool wraps the `ccsm` CLI behind a clean interface — no shell commands needed.

The extension also hooks `before_agent_start` to auto-inject the active session's goal and scope into Pi's system prompt, so the agent always knows what it's working on.

## Tools Available

| Tool | Description |
|------|-------------|
| `ccsm_list` | List sessions (active, summary, status filter, group filter) |
| `ccsm_scan` | Compact scan with full-text search across name/goal/tags |
| `ccsm_show` | Full session details: goal, scope, tags, session_id, pids |
| `ccsm_new` | Create a new pending session entry |
| `ccsm_start` | Transition session to in_progress |
| `ccsm_complete` | Mark session as completed (with gate checks) |
| `ccsm_block` | Mark session as blocked |
| `ccsm_abandon` | Mark session as abandoned |
| `ccsm_pending` | Reset session to pending (clears identity fields) |
| `ccsm_scope` | Set session scope (approach, constraints) |
| `ccsm_tag` | Replace session tags |
| `ccsm_note` | Append timestamped progress note |
| `ccsm_check` | Add or update a checklist item |
| `ccsm_next` | Get the next session to work on in a group |
| `ccsm_inject_scope` | Output active session's goal/scope as system-reminder |
| `ccsm_close` | Pre-completion gate (detail file completeness check) |
| `ccsm_resume` | Spawn pi with `--session <uuid>` for this session |
| `ccsm_doctor` | Scan for health issues |
| `ccsm_group` | Manage session groups |
| `ccsm_depend` | Manage session dependencies |
| `ccsm_attach` | Link a session UUID to a ccsm entry |
| `ccsm_gate_check` | Check if work aligns with session scope |
| `ccsm_sequence` | Batch multiple mutations in one lock/save cycle |

## Commands

| Command | Description |
|---------|-------------|
| `/ccsm <subcommand> [args...]` | Run any ccsm command interactively |

## Auto-Injection

On every agent start, the extension runs `ccsm inject-scope` and injects the result into the system prompt. This means Pi always knows:

- Which ccsm session is active
- The session's goal and scope
- Any pending checklist items

## Workflow

```
ccsm_new → ccsm_start → ccsm_scope → ccsm_note → ccsm_check ... → ccsm_close → ccsm_complete
```

When resuming: `ccsm_resume` spawns `pi --session <uuid> -n <name>`, giving Pi the session context directly.
