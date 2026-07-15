/// Auto-chain migration: ccsm v0.0.0 → current.
///
/// Each `ChainLink` transforms data from one specific version to the next.
/// The runner reads `identity.version`, finds the matching link, applies it,
/// writes the new version, and loops until reaching the target.
///
/// This ensures incremental transformations even when jumping many versions:
///   v0.0.0 → v0.1.0 → v0.15.0 → v0.16.0 → v0.17.0

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

// ── Types ─────────────────────────────────────────────────────────────

pub struct MigrationContext<'a> {
    pub root: &'a Path,
    pub id: &'a str,
}

pub struct ChainLink {
    pub from: &'static str,
    pub to: &'static str,
    pub desc: &'static str,
    pub run: fn(&MigrationContext) -> Result<()>,
}

#[derive(Debug, Default)]
pub struct MigrationReport {
    pub steps_run: Vec<String>,
}

// ── The Chain (in order, earliest version first) ─────────────────────
// Every possible identity.version must have a matching `from` entry.
// Target is always env!("CARGO_PKG_VERSION") — the binary's own version.

const CHAIN: &[ChainLink] = &[
    ChainLink {
        from: "1",
        to: "0.1.0",
        desc: "normalize pre-semver identity version",
        run: step_normalize_identity,
    },
    ChainLink {
        from: "0.1.0",
        to: "0.15.0",
        desc: "rehome data from .ccsm/ dir to ~/.ccsm/<id>/",
        run: step_ccsm_dir_to_global,
    },
    ChainLink {
        from: "0.15.0",
        to: "0.16.0",
        desc: "strip stale worktree field from sessions.json",
        run: step_strip_worktree,
    },
    ChainLink {
        from: "0.16.0",
        to: "0.17.0",
        desc: "seed config, ensure data directory",
        run: step_seed_and_dir,
    },
];

// ── Step Functions ────────────────────────────────────────────────────

/// "1" → "0.1.0": normalize pre-semver identity version.
/// The version bump is handled by the chain runner — this step exists
/// as a placeholder for any future data transformations alongside the rename.
fn step_normalize_identity(_ctx: &MigrationContext) -> Result<()> {
    Ok(())
}

/// "0.1.0" → "0.15.0": migrate data from old .ccsm/ directory
/// (containing sessions.json) to ~/.ccsm/<id>/.
fn step_ccsm_dir_to_global(ctx: &MigrationContext) -> Result<()> {
    let legacy_dir = ctx.root.join(".ccsm");
    if !legacy_dir.is_dir() || !legacy_dir.join("sessions.json").exists() {
        return Ok(());
    }
    crate::registry::migrate_legacy_data(ctx.root, ctx.id)?;
    Ok(())
}

/// "0.15.0" → "0.16.0": re-read sessions.json and re-save to strip
/// the removed `worktree` field. No-op if no registry file exists.
fn step_strip_worktree(ctx: &MigrationContext) -> Result<()> {
    let reg_path = crate::registry::global_registry_path(ctx.id);
    if !reg_path.exists() {
        return Ok(());
    }
    let contents = std::fs::read_to_string(&reg_path)?;
    let mut reg: crate::registry::WorkspaceRegistry = serde_json::from_str(&contents)
        .context("parsing sessions.json to strip stale worktree fields")?;
    reg.updated = crate::registry::now_iso();
    let new_contents = serde_json::to_string_pretty(&reg)?;
    std::fs::write(&reg_path, new_contents)
        .context("writing cleaned sessions.json")?;
    Ok(())
}

/// "0.16.0" → "0.17.0": ensure data dir exists, seed default config.
fn step_seed_and_dir(ctx: &MigrationContext) -> Result<()> {
    let data_dir = crate::registry::global_data_dir(ctx.id);
    if !data_dir.is_dir() {
        std::fs::create_dir_all(data_dir.join("sessions"))
            .context("creating global sessions dir")?;
        std::fs::create_dir_all(data_dir.join("session-group"))
            .context("creating global session-group dir")?;
        std::fs::create_dir_all(data_dir.join("worktrees"))
            .context("creating global worktrees dir")?;
    }

    let config_path = crate::registry::global_config_path(ctx.id);
    if !config_path.exists() {
        let default_config = r#"# ccsm project configuration
branch_tracking = "optional"
wip_limit = 0
"#;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&config_path, default_config)?;
    }

    Ok(())
}

// ── Identity Helpers ──────────────────────────────────────────────────

/// Find the workspace root: walk up for .ccsm, then git root, then CWD.
/// Matches resolve_identity() semantics — the .ccsm location IS the root.
fn find_root(cwd: &Path) -> PathBuf {
    if let Ok(Some((root, _))) = crate::registry::find_project_root(cwd) {
        return root;
    }
    crate::registry::find_nearest_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf())
}

/// Read identity raw from disk — no migration logic, just parse.
fn read_identity(root: &Path) -> Result<Option<crate::registry::WorkspaceIdentity>> {
    let ccsm_path = root.join(".ccsm");
    if !ccsm_path.is_file() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&ccsm_path)
        .with_context(|| format!("reading {}", ccsm_path.display()))?;
    let identity: crate::registry::WorkspaceIdentity = toml::from_str(&content)
        .with_context(|| format!("parsing {}", ccsm_path.display()))?;
    Ok(Some(identity))
}

/// Write identity file.
fn write_identity(root: &Path, identity: &crate::registry::WorkspaceIdentity) -> Result<()> {
    let content = format!(
        "version = \"{}\"\nid = \"{}\"\n",
        identity.version, identity.id
    );
    std::fs::write(root.join(".ccsm"), &content)
        .with_context(|| format!("writing {}", root.join(".ccsm").display()))?;
    Ok(())
}

/// Bootstrap: create a fresh identity for a workspace that has none.
/// Writes version "1" (the earliest chain entry), then the chain progresses.
fn bootstrap_identity(root: &Path) -> Result<String> {
    let id = crate::registry::uuid_v4();
    let identity = crate::registry::WorkspaceIdentity {
        version: "1".into(),
        id: id.clone(),
    };
    write_identity(root, &identity)?;
    eprintln!("  ✓ created .ccsm identity (starting from v1)");
    Ok(id)
}

/// Migrate `.claude/sessions.json` and detail files to `~/.ccsm/<id>/`.
/// Handles the legacy format from before ccsm had its own global data directory.
fn migrate_claude_legacy(root: &Path, id: &str) -> Result<bool> {
    let claude = root.join(".claude");
    if !claude.join("sessions.json").exists() {
        return Ok(false);
    }

    let ccsm = crate::registry::global_data_dir(id);
    if !ccsm.exists() {
        std::fs::create_dir_all(&ccsm)?;
    }

    let mut copied = 0u32;

    // sessions.json
    let src_json = claude.join("sessions.json");
    let dst_json = ccsm.join("sessions.json");
    if src_json.exists() && !dst_json.exists() {
        let contents = std::fs::read_to_string(&src_json)?;
        let mut reg: crate::registry::WorkspaceRegistry =
            serde_json::from_str(&contents).context("parsing legacy .claude registry")?;
        for s in &mut reg.sessions {
            if s.consumer.is_empty() {
                s.consumer = "claude".into();
            }
        }
        reg.save()?;
        copied += 1;
    }

    // sessions/ detail files
    let src_sessions = claude.join("sessions");
    let dst_sessions = ccsm.join("sessions");
    if src_sessions.is_dir() {
        if !dst_sessions.exists() {
            std::fs::create_dir_all(&dst_sessions)?;
        }
        for entry in std::fs::read_dir(&src_sessions)?.flatten() {
            let src = entry.path();
            if src.extension().is_some_and(|e| e == "md") {
                let name = src.file_stem().and_then(|n| n.to_str()).unwrap_or("");
                let dst = dst_sessions.join(format!("{name}.md"));
                if !dst.exists() {
                    std::fs::copy(&src, &dst)?;
                    copied += 1;
                }
            }
        }
    }

    // session-group/
    let src_group = claude.join("session-group");
    let dst_group = ccsm.join("session-group");
    if src_group.is_dir() {
        if !dst_group.exists() {
            std::fs::create_dir_all(&dst_group)?;
        }
        for entry in std::fs::read_dir(&src_group)?.flatten() {
            let src = entry.path();
            if src.extension().is_some_and(|e| e == "md") {
                let name = src.file_stem().and_then(|n| n.to_str()).unwrap_or("");
                let dst = dst_group.join(format!("{name}.md"));
                if !dst.exists() {
                    std::fs::copy(&src, &dst)?;
                    copied += 1;
                }
            }
        }
    }

    // session-detail-template.md
    let src_tpl = claude.join("session-detail-template.md");
    let dst_tpl = ccsm.join("session-detail-template.md");
    if src_tpl.exists() && !dst_tpl.exists() {
        std::fs::copy(&src_tpl, &dst_tpl)?;
        copied += 1;
    }

    eprintln!("  ✓ migrated {} items from .claude/", copied);
    Ok(true)
}

// ── Public API ────────────────────────────────────────────────────────

/// Run the full migration chain: whatever version the project is on → current.
pub fn run_migrate() -> Result<MigrationReport> {
    let cwd = std::env::current_dir()?;
    let root = find_root(&cwd);
    let root = root.canonicalize().unwrap_or(root);
    let target = env!("CARGO_PKG_VERSION");

    let mut report = MigrationReport::default();

    eprintln!(
        "ccsm: auto-chain migration → v{}",
        target
    );
    eprintln!("  workspace root: {}", root.display());
    eprintln!();

    // Load or bootstrap identity
    let mut identity = match read_identity(&root)? {
        Some(id) => id,
        None => {
            let id = bootstrap_identity(&root)?;
            crate::registry::WorkspaceIdentity {
                version: "1".into(),
                id,
            }
        }
    };

    let first_version = identity.version.clone();

    // Subsumed migrate-ccsm: migrate .claude/ legacy data if present.
    if migrate_claude_legacy(&root, &identity.id)? {
        report.steps_run.push("migrate .claude/ legacy data".into());
    }

    let mut ran_any = false;

    while identity.version != target {
        if let Some(link) = CHAIN.iter().find(|l| l.from == identity.version) {
            let ctx = MigrationContext {
                root: &root,
                id: &identity.id,
            };
            eprintln!("  [{} → {}] {}...", link.from, link.to, link.desc);
            (link.run)(&ctx)?;
            identity.version = link.to.to_string();
            report.steps_run.push(link.desc.into());
            ran_any = true;
        } else {
            // No breaking change for this version gap — fast-forward.
            eprintln!(
                "  [{}  →  {}] no data changes — fast-forward",
                identity.version, target
            );
            identity.version = target.to_string();
            ran_any = true;
        }
        write_identity(&root, &identity)?;
    }

    if !ran_any {
        eprintln!("  ✓ already at v{} — nothing to migrate", first_version);
    } else {
        eprintln!();
        eprintln!("  ✓ migrated from v{} → v{}", first_version, target);
    }

    Ok(report)
}
