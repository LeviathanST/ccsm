# ccsm-swarm: MCP Tool Reference

## Overview

ccsm-swarm is an MCP server for orchestrating multiple AI agents via tmux. It runs as a single-binary stdio MCP server (no runtime deps beyond tmux) and exposes 7 tools.

## Tools

### swarm-list-panes

List all tmux panes with session, window, and process info.

**Arguments:** None

**Returns:** JSON array of panes

```json
[
  {
    "session": "swarm",
    "window": "0",
    "pane_index": "0",
    "pane_id": "%0",
    "process": "bash"
  }
]
```

### swarm-capture

Read pane output. Delta-aware by default — returns only new content since the last read.

**Arguments:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `target` | string | required | Pane ID (%0), label, or session:window.pane |
| `lines` | int | null | Omit or pass `-1` for delta mode. Pass `N` (positive) for explicit last N lines, which bypasses delta tracking |

**Delta mode:** first call returns all content, subsequent calls return only new bytes. Delta tracking uses byte offsets — content that hasn't changed returns empty string.

### swarm-inject

Send text to a pane. Optionally press Enter.

**Arguments:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `target` | string | required | Pane ID, label, or session:window.pane |
| `text` | string | required | Text to send (max 65536 bytes) |
| `enter` | bool | true | Press Enter after typing |

### swarm-wait

Block until a sentinel string appears in pane output. Polls every 2 seconds.

**Arguments:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `target` | string | required | Pane ID, label, or session:window.pane |
| `sentinel` | string | required | String to wait for |
| `timeout_secs` | uint | 300 | Max wait time (max 3600) |

**Returns:** `{"ok": true, "content": "<pane output>"}` on match. Returns MCP error on timeout.

### swarm-status

Consolidated status of all panes or a specific one.

**Arguments:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `target` | string | null | Optional — filter to one pane or session. Omit for all |

**Returns:** `{panes: [{session, pane_id, process, last_line, label?, error?}], count}`

### swarm-broadcast

Send the same text to every pane.

**Arguments:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `text` | string | required | Text to send |
| `enter` | bool | true | Press Enter after typing |

**Returns:** Per-pane results with ok/error per target.

### swarm-label

Assign a name to a pane for role-based targeting in other tools.

**Arguments:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `target` | string | required | Pane ID or session:window.pane |
| `label` | string | required | Name to assign |

Labels persist for the lifetime of the MCP server process. Use meaningful names like `agent-a`, `reviewer`, `tester`.

## Architecture

```
MCP Client (OpenCode)
    │  MCP/stdio
    ▼
ccsm-swarm  ──tmux commands──►  tmux
(binary)                      (sessions/panes)
    │
    ▼
ccsm CLI (planned Phase 3: direct library integration)
```

## Delta Tracking

Internal state tracks byte offsets per pane:

- **First read:** returns full content, stores total bytes
- **Subsequent reads:** calculates delta from stored offset
- **Shrink detection:** if content shrinks (buffer clear), resets baseline and returns full content
- **Explicit `lines: N`:** bypasses delta tracking entirely

## Sentinel Pattern

The recommended workflow for task coordination:

1. Inject task with `##DONE##` sentinel requirement
2. Call `swarm-wait sentinel="##DONE##"` — blocks server-side
3. Server captures pane every 2s, returns immediately on match
4. Zero polling from the orchestrator — one MCP call

## Error Handling

- Missing tmux: clear error on first tool call (not crash)
- Dead panes: per-pane error field in `swarm-status`
- Broadcast failures: per-pane results, no partial state
- Timeouts: returned as MCP errors (not success with error payload)
- Text length: capped at 64KB
- Wait timeout: capped at 3600s
