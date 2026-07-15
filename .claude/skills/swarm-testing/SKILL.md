---
name: swarm-testing
description: >
  Final verification gate for ccsm — run the built binary through tmux
  inside an unshare sandbox. Catches stdin blocking, terminal detection,
  and side-effect leaks that unit tests cannot. Use only after cargo test
  is green and binary is built.
---

# Swarm Testing — Final Verification Gate

## When to use this

**Last step before push.** Never during development.

Full pipeline order:
```
Write code → cargo test → cargo build --release → SWARM TEST → commit/push
```

If swarm test fails, fix, `cargo test`, rebuild, swarm test again. Never skip.

## Why sandbox + tmux?

| Approach | Catches |
|----------|---------|
| Unit tests | Logic, parsing, early returns |
| `cargo test` | Exit codes, stdout/stderr, fs output |
| **Swarm + sandbox** | Stdin blocking, `is_terminal()` behavior, signals, `~/.ccsm/` side-effects |

Real examples that only this catches:
- Silent `read_line()` blocking the parent process (the AGENTS.md rule)
- `ccsm attach` detection difference between pipe and real TTY
- `ccsm init` interactive prompt in a non-TTY subprocess
- Identity file corruption on Ctrl+C mid-migration
- Child processes orphaned after parent dies

## Isolation via `unshare -r`

Every test runs inside a **Linux user namespace** (`unshare -r`):
- Private user namespace (root inside, regular user outside)
- Private mount namespace (no filesystem leakage)
- `$HOME` points to `/tmp/` — `~/.ccsm/` never touches the real one
- Fixture projects live in `/tmp/` — gone when tmux session dies
- No Docker, no chroot, no setup

## Sentinel Protocol

Every injected command chain **must** end with a sentinel:

```bash
command && echo "##DONE##"
```

`swarm-wait sentinel="##DONE##"` blocks until it appears.

Use failure signaling for branch-aware waits:

```bash
dangerous_command && echo "##DONE##" || echo "##FAIL##"
```

### Rules
- One sentinel per concurrent chain
- Sentinel is always the **last line**
- Capture output AFTER wait returns (wait guarantees completion)

## Setup & Teardown

```bash
# Start
tmux new-session -d -s swarm -x 200 -y 60
tmux set-option -t swarm remain-on-exit on

# End
tmux kill-session -t swarm
```

## Reusable Sandbox Template

This is the common wrapper used in every test:

```bash
unshare -r bash -c '
  home=$(mktemp -d /tmp/swarm-home-XXXXXX)
  project=$(mktemp -d /tmp/swarm-project-XXXXXX)
  HOME=$home
  cd "$project"
  # --- inject commands here ---
  echo "##DONE##"
'
```

Both `$home` and `$project` are `/tmp/` — destroyed when tmux kills the session.

## The Only Pattern: Build & Smoke Test

Single pane, sequential. Build first, then run one or more commands in the sandbox.

```
PANE: tester
```

```bash
# 1. Label
swarm-label target=%0 label=tester

# 2. Build
swarm-inject target=tester \
  text="cd /home/leviathanst/workspaces/tools/ccsm/.claude/worktrees/auto-chain-migration && cargo build --release && echo \"##BUILT##\""
swarm-wait target=tester sentinel="##BUILT##" timeout_secs=120

# 3. Smoke test in sandbox
BINARY="/home/leviathanst/workspaces/tools/ccsm/.claude/worktrees/auto-chain-migration/target/release/ccsm"
swarm-inject target=tester \
  text="unshare -r bash -c 'home=\$(mktemp -d /tmp/swarm-home-XXXXXX) && project=\$(mktemp -d /tmp/swarm-project-XXXXXX) && HOME=\$home && cd \"\$project\" && $BINARY init && $BINARY list --active && echo \"##DONE##\"'"

# 4. Wait + capture
swarm-wait target=tester sentinel="##DONE##"
output = swarm-capture target=tester

# 5. Assert
#   - output contains expected strings
#   - no "ERROR", "panic", or "thread" in output
```

### Running multiple scenarios

Chain sandbox runs sequentially in the same pane:

```bash
# Test init
swarm-inject target=tester \
  text="unshare -r bash -c 'home=\$(mktemp -d /tmp/swarm-home-XXXXXX) && project=\$(mktemp -d /tmp/swarm-project-XXXXXX) && HOME=\$home && cd \"\$project\" && $BINARY init && echo \"##DONE##\"'"
swarm-wait target=tester sentinel="##DONE##"

# Test new
swarm-inject target=tester \
  text="unshare -r bash -c 'home=\$(mktemp -d /tmp/swarm-home-XXXXXX) && project=\$(mktemp -d /tmp/swarm-project-XXXXXX) && HOME=\$home && cd \"\$project\" && $BINARY init && $BINARY new smoke-test -g \"testing\" && echo \"##DONE##\"'"
swarm-wait target=tester sentinel="##DONE##"
```

Each run gets a fresh home + project. No state leaks between scenarios.

### Version check on the built binary

Quick sanity before any testing:

```bash
# Label + build (same as above), then:
swarm-inject target=tester text="$BINARY --version && echo \"##DONE##\""
swarm-wait target=tester sentinel="##DONE##"
version_output = swarm-capture target=tester
# Assert: version_output contains "0.18.0"
```

## Assertion Convention

```bash
# After capture, check:
swarm-capture target=tester | grep -q "expected string"      # ✅ success
swarm-capture target=tester | grep -qi "panic"               # ❌ crash
swarm-capture target=tester | grep -qi "error"               # ❌ error
swarm-capture target=tester | grep -qi "thread"              # ❌ panic thread output
```

## Capture Convention

Use delta mode (no `lines` arg) — returns only new content since last read:

```bash
swarm-capture target=tester    # only new content
swarm-capture target=tester lines=20  # last 20 lines (overrides delta)
```

Capture **after** wait — wait guarantees the command finished, so all output is available.
