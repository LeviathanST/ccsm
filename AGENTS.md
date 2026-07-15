# ccsm-swarm: Tmux MCP Server for Multi-Agent Orchestration

ccsm-swarm is an MCP server that wraps tmux + ccsm for multi-agent orchestration. It's registered in OpenCode's MCP config — tools are available to any agent in this workspace.

## Available Tools

| Tool | Description |
|------|-------------|
| `swarm-list-panes` | List all tmux panes with session, window, process |
| `swarm-capture` | Read pane output (delta-aware — only new content) |
| `swarm-inject` | Type text into a pane |
| `swarm-wait` | Block until a sentinel string appears |
| `swarm-status` | Consolidated status of all panes |
| `swarm-broadcast` | Same text to every pane |
| `swarm-label` | Name a pane for role-based targeting |

## Workflow: Agent Orchestration

### 1. Create a swarm session
```bash
tmux new-session -d -s swarm -x 200 -y 60
# Split into a grid
tmux split-window -h -t swarm
tmux split-window -v -t swarm:0.0
tmux split-window -v -t swarm:0.1
tmux select-layout -t swarm tiled
```

### 2. Label panes for targeting
Use `swarm-label` to give panes role names so you can target them without raw pane IDs.

### 3. Spawn agents in each pane
Use `swarm-inject` to run agents (e.g. `opencode`, `claude`) in labeled panes.

### 4. Coordinate work
- **Distribute tasks**: `swarm-inject` to send prompts to individual panes
- **Broadcast**: `swarm-broadcast` for the same message to all agents
- **Monitor**: `swarm-status` for a consolidated dashboard
- **Wait**: `swarm-wait sentinel="##DONE##"` to block until an agent finishes
- **Read**: `swarm-capture` to get new output (delta mode)

### 5. Close it down
```bash
tmux kill-session -t swarm
```

## Delta Tracking

`swarm-capture` tracks byte offsets per pane. Repeated calls return only new content since the last read. Uses `lines: N` to override and get explicit last N lines instead.

## Sentinel Pattern

Agents should print `##DONE##` (or a custom sentinel) as their last line when they finish a task. Then `swarm-wait` returns immediately — no polling needed from the orchestrator.

## Build & Install

```bash
cargo build --release -p ccsm-swarm
cp target/release/ccsm-swarm ~/.local/bin/
```

## Engineering Rules

### Never read stdin without a visible prompt

A silent `std::io::stdin().read_line()` blocks the parent process (opencode, Claude, Pi) with zero feedback — no output, no cursor, nothing. The user sees a frozen terminal and assumes the tool hung.

**Always print a prompt before reading stdin.** Even better: don't prompt at all. Print a warning and a remediation command instead. This works identically whether the user is at a terminal or running inside an agent subprocess.

Bad:
```rust
// Silent block — steals stdin from parent
let mut input = String::new();
std::io::stdin().read_line(&mut input)?;
```

Good:
```rust
// Warn and tell user what to do
eprintln!("ccsm: cannot resolve X. Run `ccsm fix` to repair.");
return Ok(());
```

If you must prompt interactively, guard with `std::io::stdin().is_terminal()` and only prompt when TTY. On non-TTY (pipe, subprocess), fall through with a warning.
