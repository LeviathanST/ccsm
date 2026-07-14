/// Project-level ccsm configuration at `~/.ccsm/<id>/config.toml`.
///
/// Controls branch tracking policy, WIP limits, and checklist templates.
/// Loaded automatically by the CLI — agents don't read this file directly.

use serde::Deserialize;
use std::collections::HashMap;

// ── Config ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Branch tracking policy: "required", "optional" (default), or "disabled".
    #[serde(default = "default_branch_tracking")]
    pub branch_tracking: BranchTracking,

    /// Max in_progress sessions before `ccsm new` warns. 0 = disabled.
    #[serde(default = "default_wip_limit")]
    pub wip_limit: usize,

    /// Worktree policy: "required" (all branch sessions), "optional" (only with --worktree flag, default), or "disabled".
    #[serde(default = "default_worktree_policy")]
    pub worktrees: WorktreePolicy,

    /// Custom checklist templates (override built-in defaults).
    #[serde(default)]
    pub checklist_templates: HashMap<String, ChecklistTemplate>,

    /// Default template type when `-c` is used without a value.
    /// If unset, `-c` alone produces an empty checklist.
    #[serde(default)]
    pub default_checklist_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchTracking {
    Required,
    Optional,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreePolicy {
    Required,
    Optional,
    Disabled,
}

fn default_branch_tracking() -> BranchTracking {
    BranchTracking::Optional
}

fn default_worktree_policy() -> WorktreePolicy {
    WorktreePolicy::Optional
}

fn default_wip_limit() -> usize {
    0
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChecklistTemplate {
    pub items: Vec<String>,
}

impl Config {
    /// Load from `~/.ccsm/<id>/config.toml`. Returns defaults if missing/invalid.
    pub fn load() -> Self {
        let path = match crate::registry::resolve_or_create_identity() {
            Ok(ctx) => crate::registry::global_config_path(&ctx.id),
            Err(_) => return Config::defaults(),
        };
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(config) => return config,
                    Err(e) => {
                        eprintln!("warning: {} parse error: {e}", path.display());
                        eprintln!("  using defaults");
                    }
                },
                Err(e) => {
                    eprintln!("warning: could not read {}: {e}", path.display());
                }
            }
        }
        Config::defaults()
    }

    pub fn defaults() -> Self {
        Self {
            branch_tracking: BranchTracking::Optional,
            wip_limit: 0,
            worktrees: WorktreePolicy::Optional,
            checklist_templates: HashMap::new(),
            default_checklist_type: None,
        }
    }

    /// Get checklist items for a template type. Checks config overrides first,
    /// falls back to built-in defaults.
    pub fn get_checklist_items(&self, typ: &str) -> Option<Vec<String>> {
        if let Some(tmpl) = self.checklist_templates.get(typ) {
            return Some(tmpl.items.clone());
        }
        builtin_template(typ).map(|items| items.iter().map(|s| s.to_string()).collect())
    }
}

// ── Built-in checklist templates ────────────────────────────────────────

/// Built-in checklist items per type. Config overrides take precedence.
fn builtin_template(typ: &str) -> Option<&'static [&'static str]> {
    match typ {
        "feat" => Some(&[
            "Implementation plan drafted",
            "Tests written for new functionality",
            "Edge cases handled",
            "Documentation updated",
        ]),
        "fix" => Some(&[
            "Root cause confirmed",
            "Regression test added",
            "All callers handled",
            "Fix verified against edge cases",
        ]),
        "research" => Some(&[
            "Sources found and reviewed",
            "Claims verified",
            "Tradeoffs documented",
            "Recommendation stated",
        ]),
        "chore" => Some(&[
            "No side effects confirmed",
            "Cleanup scope defined",
            "Dependencies updated (if any)",
            "Automation verified",
        ]),
        "default" => None, // -c with no type = empty checklist
        _ => None,
    }
}
