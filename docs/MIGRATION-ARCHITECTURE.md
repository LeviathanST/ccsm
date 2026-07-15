# Migration Architecture: Auto-Chain from v0.0.0 → Current

## Motivation

ccsm stores session data on disk (identity, sessions.json, detail files, config). As ccsm evolves, the data format changes — fields are added, removed, renamed, or relocated. A project might be using an older ccsm version with older data formats.

We need a system that:

1. **Detects the current data version** of a project
2. **Transforms the data incrementally** through each version boundary
3. **Ends at the current ccsm version** — no manual steps

## Core Design: Version-Keyed Chain

The identity file (`<root>/.ccsm`) contains a `version` field that tracks which ccsm version last wrote this project's data:

```toml
version = "0.16.0"
id = "0af54e00-..."
```

The migration system is a **chain** — an ordered list of `ChainLink` entries. Each link knows how to transform data from one specific version to the next:

```rust
struct ChainLink {
    from: &'static str,   // identity.version before this step
    to: &'static str,     // identity.version after this step
    desc: &'static str,   // human-readable description
    run: fn(&MigrationContext) -> Result<()>,
}
```

The runner reads `identity.version`, finds the matching `from`, applies the transformation, writes `to` back, and loops until `identity.version == target`:

```
identity.version = "0.15.0"
target           = "0.17.0"

Loop:
  "0.15.0" → "0.16.0"  (strip worktree field)
  identity.version = "0.16.0"
  "0.16.0" → "0.17.0"  (seed config, ensure data dir)
  identity.version = "0.17.0"
  identity.version == target → done
```

## Version as Position

The `from` field in each `ChainLink` is **unique** — it's the version a workspace would have after the *previous* step ran. This means:

- **Insert-safe**: A new step between `0.15.0` and `0.16.0` creates a synthetic version like `"0.15.1"`. Existing workspaces at `"0.15.0"` will hit the new step next, then continue to `"0.16.0"`.
- **Total order**: Every possible identity version has exactly one `from` match, so the runner never forks or skips.
- **Forward-only**: No downgrade path. If the binary is older than the project's version, we block and tell the user to upgrade.

## Three-Way Version Check

When a user runs ccsm in a project, we compare:

| Case | Meaning | Action |
|------|---------|--------|
| `binary == project` | Same version | ✅ Proceed normally |
| `binary > project` | Project is behind | ⚠️ Block + "Run `ccsm migrate` to upgrade this project from v{project} → v{binary}" |
| `binary < project` | Binary too old | 🛑 Block + "This project uses v{project} which is newer than ccsm v{binary}. Upgrade ccsm first" |

Migration only runs in the **binary > project** case. The **binary < project** guard prevents data corruption from an older binary interpreting newer data formats.

## Chain Steps

```rust
const CHAIN: &[ChainLink] = &[
    // Non-semver legacy:
    ChainLink { from: "1",       to: "0.1.0",  desc: "normalize pre-semver identity",        run: step_normalize_identity },
    // Semver chain:
    ChainLink { from: "0.1.0",   to: "0.15.0", desc: "migrate .ccsm/ dir → ~/.ccsm/<id>/",  run: step_ccsm_dir_to_global },
    ChainLink { from: "0.15.0",  to: "0.16.0", desc: "strip stale worktree field",           run: step_strip_worktree },
    ChainLink { from: "0.16.0",  to: "0.17.0", desc: "seed config, ensure data dir",         run: step_seed_and_dir },
];
```

The target is always `env!("CARGO_PKG_VERSION")` — the binary's own version. The chain always runs forward from wherever the project is.

## Runner Algorithm

```rust
pub fn run_migrate() -> Result<MigrationReport> {
    let root = find_migration_root(&cwd)?;
    let ctx = resolve_or_bootstrap(&root)?;
    let target = env!("CARGO_PKG_VERSION");

    while ctx.identity.version != target {
        let link = CHAIN.iter()
            .find(|l| l.from == ctx.identity.version)
            .context("no migration path from version {ctx.identity.version}")?;

        eprintln!("  [{} → {}] {}...", link.from, link.to, link.desc);
        (link.run)(&ctx)?;

        ctx.identity.version = link.to.to_string();
        persist_identity(&root, &ctx.identity)?;
        report.steps_run.push(link.desc.into());
    }

    Ok(report)
}
```

## When to Add a Chain Entry

Only add a `ChainLink` when there's an **actual breaking change** to the data format: field renamed, field removed, data moved to a new location, etc. Version bumps without data changes fast-forward automatically — the runner skips unlisted versions.

This keeps the chain lean: entries exist only where transformations are needed.

## Adding a New Migration Step

When introducing a breaking data change:

1. Write a new step function (e.g., `step_fix_retired_ids`)
2. Insert a `ChainLink` with a `from` version between the existing step and the next
3. Use a synthetic patch version for the `to` field (e.g., `"0.17.1"`)

Example — inserting a step between `0.15.0` and `0.16.0`:

```rust
// Before:
ChainLink { from: "0.15.0",  to: "0.16.0", desc: "strip worktree", run: step_strip_worktree },

// After:
ChainLink { from: "0.15.0",  to: "0.15.1", desc: "strip worktree",              run: step_strip_worktree },
ChainLink { from: "0.15.1",  to: "0.16.0", desc: "normalize retired ids",       run: step_fix_retired_ids },
```

No re-indexing, no renaming of functions. Workspaces at `"0.15.0"` will run both steps in order. Workspaces already at `"0.16.0"` are unaffected.

## Key Properties

| Property | How it's achieved |
|----------|------------------|
| Idempotent | Version persists after each step; re-runs skip applied steps |
| Insert-safe | New steps use a `from` version between two existing versions |
| Crash-safe | Each step writes `to` version on success; re-run picks up there |
| Forward-only | No downgrade; binary < project blocks with error |
| Self-documenting | Identity version tells you exactly where the project is |
| Fast-forward | Version gaps without chain entries jump directly to target — only data-altering steps need entries |
