/// Project-level ccsm configuration at `~/.ccsm/<id>/config.toml`.
///
/// Controls branch tracking policy, WIP limits, and checklist templates.
/// Loaded automatically by the CLI — agents don't read this file directly.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Config ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchTracking {
    Required,
    Optional,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChecklistTemplate {
    pub items: Vec<String>,
}

impl Config {
    /// Load from `~/.ccsm/<id>/config.toml`. Returns defaults if missing/invalid.
    pub fn load() -> Self {
        let path = match crate::registry::resolve_identity() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_returns_correct_values() {
        let cfg = Config::defaults();
        assert_eq!(cfg.branch_tracking, BranchTracking::Optional);
        assert_eq!(cfg.wip_limit, 0);
        assert_eq!(cfg.worktrees, WorktreePolicy::Optional);
        assert!(cfg.checklist_templates.is_empty());
        assert!(cfg.default_checklist_type.is_none());
    }

    #[test]
    fn deserialize_full_config() {
        let toml_str = r#"
branch_tracking = "disabled"
wip_limit = 3
worktrees = "required"
default_checklist_type = "feat"

[checklist_templates.custom]
items = ["Step 1", "Step 2"]
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.branch_tracking, BranchTracking::Disabled);
        assert_eq!(cfg.wip_limit, 3);
        assert_eq!(cfg.worktrees, WorktreePolicy::Required);
        assert_eq!(cfg.default_checklist_type.as_deref(), Some("feat"));
        let tmpl = cfg.checklist_templates.get("custom").unwrap();
        assert_eq!(tmpl.items, vec!["Step 1".to_string(), "Step 2".to_string()]);
    }

    #[test]
    fn deserialize_partial_config_gets_defaults() {
        let toml_str = r#"wip_limit = 5"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.branch_tracking, BranchTracking::Optional);
        assert_eq!(cfg.wip_limit, 5);
        assert_eq!(cfg.worktrees, WorktreePolicy::Optional);
        assert!(cfg.checklist_templates.is_empty());
        assert!(cfg.default_checklist_type.is_none());
    }

    #[test]
    fn deny_unknown_fields_rejects_unknown_keys() {
        let toml_str = r#"
branch_tracking = "optional"
unknown_key = "value"
"#;
        let result: Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn get_checklist_items_feat() {
        let cfg = Config::defaults();
        let items = cfg.get_checklist_items("feat").unwrap();
        assert_eq!(
            items,
            vec![
                "Implementation plan drafted",
                "Tests written for new functionality",
                "Edge cases handled",
                "Documentation updated",
            ]
        );
    }

    #[test]
    fn get_checklist_items_fix() {
        let cfg = Config::defaults();
        let items = cfg.get_checklist_items("fix").unwrap();
        assert_eq!(
            items,
            vec![
                "Root cause confirmed",
                "Regression test added",
                "All callers handled",
                "Fix verified against edge cases",
            ]
        );
    }

    #[test]
    fn get_checklist_items_research() {
        let cfg = Config::defaults();
        let items = cfg.get_checklist_items("research").unwrap();
        assert_eq!(
            items,
            vec![
                "Sources found and reviewed",
                "Claims verified",
                "Tradeoffs documented",
                "Recommendation stated",
            ]
        );
    }

    #[test]
    fn get_checklist_items_chore() {
        let cfg = Config::defaults();
        let items = cfg.get_checklist_items("chore").unwrap();
        assert_eq!(
            items,
            vec![
                "No side effects confirmed",
                "Cleanup scope defined",
                "Dependencies updated (if any)",
                "Automation verified",
            ]
        );
    }

    #[test]
    fn get_checklist_items_default_returns_none() {
        let cfg = Config::defaults();
        assert!(cfg.get_checklist_items("default").is_none());
    }

    #[test]
    fn get_checklist_items_unknown_returns_none() {
        let cfg = Config::defaults();
        assert!(cfg.get_checklist_items("nonexistent_type").is_none());
    }

    #[test]
    fn custom_template_overrides_builtin() {
        let mut templates = HashMap::new();
        templates.insert(
            "feat".to_string(),
            ChecklistTemplate {
                items: vec!["Custom step".to_string()],
            },
        );
        let cfg = Config {
            branch_tracking: BranchTracking::Optional,
            wip_limit: 0,
            worktrees: WorktreePolicy::Optional,
            checklist_templates: templates,
            default_checklist_type: None,
        };
        let items = cfg.get_checklist_items("feat").unwrap();
        assert_eq!(items, vec!["Custom step"]);
    }

    #[test]
    fn branch_tracking_deserializes_required() {
        let cfg: Config = toml::from_str(r#"branch_tracking = "required""#).unwrap();
        assert_eq!(cfg.branch_tracking, BranchTracking::Required);
    }

    #[test]
    fn branch_tracking_deserializes_optional() {
        let cfg: Config = toml::from_str(r#"branch_tracking = "optional""#).unwrap();
        assert_eq!(cfg.branch_tracking, BranchTracking::Optional);
    }

    #[test]
    fn branch_tracking_deserializes_disabled() {
        let cfg: Config = toml::from_str(r#"branch_tracking = "disabled""#).unwrap();
        assert_eq!(cfg.branch_tracking, BranchTracking::Disabled);
    }

    #[test]
    fn worktree_policy_deserializes_required() {
        let cfg: Config = toml::from_str(r#"worktrees = "required""#).unwrap();
        assert_eq!(cfg.worktrees, WorktreePolicy::Required);
    }

    #[test]
    fn worktree_policy_deserializes_optional() {
        let cfg: Config = toml::from_str(r#"worktrees = "optional""#).unwrap();
        assert_eq!(cfg.worktrees, WorktreePolicy::Optional);
    }

    #[test]
    fn worktree_policy_deserializes_disabled() {
        let cfg: Config = toml::from_str(r#"worktrees = "disabled""#).unwrap();
        assert_eq!(cfg.worktrees, WorktreePolicy::Disabled);
    }

    #[test]
    fn invalid_branch_tracking_fails() {
        let result: Result<Config, _> = toml::from_str(r#"branch_tracking = "always""#);
        assert!(result.is_err());
    }

    #[test]
    fn round_trip_through_toml() {
        let cfg = Config {
            branch_tracking: BranchTracking::Required,
            wip_limit: 42,
            worktrees: WorktreePolicy::Disabled,
            checklist_templates: HashMap::from([(
                "test".to_string(),
                ChecklistTemplate {
                    items: vec!["a".to_string(), "b".to_string()],
                },
            )]),
            default_checklist_type: Some("feat".to_string()),
        };
        let toml_str = toml::to_string(&cfg).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(cfg.branch_tracking, deserialized.branch_tracking);
        assert_eq!(cfg.wip_limit, deserialized.wip_limit);
        assert_eq!(cfg.worktrees, deserialized.worktrees);
        assert_eq!(cfg.default_checklist_type, deserialized.default_checklist_type);
        assert_eq!(cfg.checklist_templates.len(), deserialized.checklist_templates.len());
        assert_eq!(
            cfg.checklist_templates.get("test").unwrap().items,
            deserialized.checklist_templates.get("test").unwrap().items
        );
    }
}
