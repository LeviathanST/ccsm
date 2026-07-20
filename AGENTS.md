# ccsm-swarm: Multi-Agent Orchestration (Removed)

> **ccsm-swarm was removed in v0.21.0.** Multi-agent orchestration via tmux MCP is no longer shipped with ccsm. If you need this functionality, use ccsm v0.20.0 or earlier.
>
> The swarm removal includes: `ccsm-swarm` binary, `swarm-list-panes`, `swarm-capture`, `swarm-inject`, `swarm-wait`, `swarm-status`, `swarm-broadcast`, and `swarm-label` MCP tools.

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
