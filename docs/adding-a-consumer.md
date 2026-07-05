# Adding a New Consumer

Checklist for integrating a new AI coding agent (consumer) into ccsm.

> **Reference implementation**: See [`docs/codewhale-consumer.md`](codewhale-consumer.md) for a complete session model and CLI analysis of CodeWhale, which was the most recent consumer added.

## Overview

Adding a new consumer means teaching ccsm how to:
- **Detect** which agent is running (binary name, config directory)
- **Spawn** the agent (resume with session, fresh start)
- **Find** its session/transcript files on disk
- **Harvest** the session UUID (PID polling, filename parsing, etc.)
- **Clean up** transcript files on archive/clean

The Consumer enum in `src/consumer.rs` abstracts all agent-specific paths and
binary names. Everything else is wired through match arms.

---

## Checklist

### 1. `src/consumer.rs` — Add the variant

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consumer {
    Claude,
    Pi,
    NewAgent,  // ← ADD
}
```

Implement every method for the new variant:

| Method | Returns | What it does |
|--------|---------|--------------|
| `binary()` | `&str` | CLI binary name (e.g. `"new-agent"`) |
| `home_config_dir()` | `&str` | Config dir under `$HOME` (e.g. `".new-agent"`) |
| `project_slug()` | `String` | Workspace path → slug for namespace |
| `sessions_dir()` | `PathBuf` | Dir containing live session files |
| `projects_dir()` | `PathBuf` | Dir containing transcript/session data per-project |
| `live_session_file()` | `Option<PathBuf>` | PID-based session file, or `None` if not used |
| `find_session_file()` | `Option<PathBuf>` | Find transcript by session UUID |
| `list_session_files()` | `Vec<PathBuf>` | List all transcript files for a slug |
| `system_prompt_tags()` | `(&str, &str)` | Tags wrapping injected scope (e.g. `<system-reminder>`) |
| `constraint_line()` | `&str` | Line appended to injected scope |
| `parse()` | `Result<Self>` | Parse `"new-agent"` string to variant |
| `Display` | `&str` | Same as `parse()` (e.g. `"new-agent"`) |
| `is_new_agent()` | `bool` | Quick check method |
| `spawn_args()` | `Vec<String>` | Translate [`SpawnOp`] to CLI args. Default handles `--resume`/`--session` + `-n <name>`. Override when agent uses different grammar (e.g. subcommands instead of flags). |
| `scope_injection_arg()` | `Option<Vec<String>>` | Extra args for scope injection at spawn (e.g. `--append-system-prompt`). Return `None` if agent handles scope via hooks. |

**Note:** [`SpawnOp`] is defined at the end of `src/consumer.rs` with three
variants: `Resume { id }`, `Fresh`, and `Refresh`. The default `spawn_args()`
implementation in the `Consumer` `impl` block serves Claude and Pi — override
only for agents with different CLI grammar (like CodeWhale's subcommand-based
resume).

Update `auto_detect()`:

```rust
fn auto_detect(home: &Path) -> Self {
    let pi_dir = home.join(".pi").join("agent");
    let claude_dir = home.join(".claude");
    let new_dir = home.join(".new-agent");    // ← ADD
    let candidates = [
        (Self::Pi, &pi_dir),
        (Self::Claude, &claude_dir),
        (Self::NewAgent, &new_dir),            // ← ADD
    ];
    // ...
}
```

Update `parse()`:

```rust
fn parse(s: &str) -> anyhow::Result<Self> {
    match s.to_lowercase().as_str() {
        "claude" => Ok(Self::Claude),
        "pi" => Ok(Self::Pi),
        "new-agent" => Ok(Self::NewAgent),   // ← ADD
        other => anyhow::bail!("..."),
    }
}
```

Add session file reader if the agent uses a custom file format (like `PiSessionMeta`).

### 2. `src/main.rs` — Wire consumer-specific behavior

Search for every `match consumer` or `consumer.is_*()` and add your variant:

| Location | What to add |
|----------|-------------|
| `run_attach()` | Auto-discovery: how to find the live session UUID (PID file vs. directory scan vs. env var) |
| `SpawnOp` in `src/consumer.rs` | **First** — add agent's CLI grammar to `spawn_args()` (resume/fresh flags or subcommands). Then `run_resume()` and `run_refresh()` automatically use it. |
| `run_clean()` / `run_archive()` | Transcript file naming convention |
| `run_inject_scope()` | System prompt format (if different from `<system-reminder>`) |
| `run_setup()` | Skill installation paths, hook registration |

**Spawn args are no longer added as raw `match` arms in `resume.rs` or
`main.rs`.** The `SpawnOp` + `spawn_args()` + `scope_injection_arg()` pattern
in `consumer.rs` centralizes CLI grammar. Add your agent's flags there.

### 3. `src/commands/resume.rs` — Harvest logic

If the agent writes PID-based session files (like Claude):
```rust
Consumer::NewAgent => {
    // Poll for live session file
    let session_file = consumer.live_session_file(home, child_pid);
    // Read session_id from it
}
```

If the agent provides the UUID upfront (like Pi):
```rust
Consumer::NewAgent => {
    // UUID known from --session flag, no harvest needed
}
```

Update the spawn args in `run_resume()`:
```rust
crate::consumer::Consumer::NewAgent => {
    if let Some(ref id) = sid {
        cmd.arg("--session").arg(id);  // or --resume, etc.
    }
    cmd.arg("-n").arg(name);
}
```

### 4. `src/commands/doctor.rs` — Health scan

The doctor auto-detects the consumer. If your agent has unique session file
patterns, add checks:

```rust
// Check for orphaned session files specific to this agent
if consumer.is_new_agent() {
    // custom checks
}
```

### 5. `src/registry.rs` — Transcript cleanup

Update `clean()` and `archive()` for the agent's transcript file naming:

```rust
let transcript = if consumer.is_pi() {
    // Pi: <timestamp>_<uuid>.jsonl
    consumer.find_session_file(home, &slug, session_id)
        .unwrap_or_else(|| proj_dir.join(format!("_{session_id}.jsonl")))
} else if consumer.is_new_agent() {  // ← ADD
    // NewAgent: specific pattern
} else {
    // Claude: <uuid>.jsonl
    proj_dir.join(format!("{session_id}.jsonl"))
};
```

### 6. Pi Extension (if the agent has one)

If the new consumer has a Pi-like extension system, create a similar extension
at `.pi/extensions/ccsm-<agent>/index.ts`. The extension always passes
`--consumer <agent>` to every ccsm call.

Key patterns from the Pi extension:
- `ccsm()` helper always adds `--consumer pi`
- `before_agent_start` hook: auto-attach + inject scope
- `getCurrentPiSessionUuid()`: agent-specific UUID discovery
- Register all 20+ tools with `pi.registerTool()`

### 7. Documentation

Update these docs:

| File | What to update |
|------|----------------|
| `CLAUDE.md` | Consumer Detection table, Data Sources table, What changes per consumer table |
| `.claude/skills/session-manager/reference/cli-commands.md` | Consumer table (flags, binary, sessions dir), attach/resume/refresh rows |
| `.claude/skills/session-manager/reference/registry-schema.md` | Any new fields added to session entries |
| `src/consumer.rs` doc comments | Module-level docs listing all consumers |

### 8. CLI help text

Update the `#[command(name = "ccsm")]` doc comment or long_about if consumer enum is documented there.

### 9. Test

- `ccsm list --consumer new-agent` — queries work
- `ccsm resume <name> --consumer new-agent` — spawns with correct binary + flags
- `ccsm attach <name> --consumer new-agent` — auto-discovers session UUID
- `ccsm clean <name> --consumer new-agent` — removes transcript files
- `ccsm doctor --consumer new-agent` — health scan

---

## File Index

| File | What it does |
|------|-------------|
| `src/consumer.rs` | Consumer enum + all agent-specific paths/formats |
| `src/main.rs` | CLI dispatch — routes to consumer-aware handlers |
| `src/commands/resume.rs` | Spawn logic + harvest |
| `src/commands/doctor.rs` | Health scan |
| `src/registry.rs` | Registry CRUD — transcript cleanup |
| `.pi/extensions/ccsm/index.ts` | Pi native tools + auto-inject |
