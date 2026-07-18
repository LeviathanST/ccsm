use anyhow::Result;
use crate::registry::{SessionStatus, WorkspaceRegistry, WorkspaceSession};

// ── SeqOp: a single batched mutation ────────────────────────────────

/// One operation in a `ccsm sequence` batch.
///
/// Each variant maps to the corresponding standalone subcommand,
/// but operates on an already-loaded `WorkspaceRegistry` in memory
/// rather than doing its own I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeqOp {
    Start { name: String },
    Complete { name: String },
    Block { name: String },
    Abandon { name: String },
    Pending { name: String },
    Scope { name: String, text: String },
    Tag { name: String, tags: Vec<String> },
    New { name: String, goal: String },
    Trash { name: String },
    Recover { name: String },
    Attach { name: String, session_id: String },
    Group { name: String, group: Option<String>, rank: Option<String>, clear: bool },
    Next { group: String },
}

impl SeqOp {
    /// Parse one `-q <command> <args...>` group.
    ///
    /// `tokens` is the list of words after one `-q` flag up to the next
    /// `-q` (or end of args).  The first token is the command name.
    pub fn parse(tokens: &[String]) -> Result<Self> {
        if tokens.is_empty() {
            anyhow::bail!("expected a command after -q");
        }

        let cmd = tokens[0].as_str();
        let args = &tokens[1..];

        match cmd {
            "start" => {
                ensure_at_least(args, 1, "start", "<name>")?;
                Ok(Self::Start { name: args[0].clone() })
            }
            "complete" => {
                ensure_at_least(args, 1, "complete", "<name>")?;
                Ok(Self::Complete { name: args[0].clone() })
            }
            "block" => {
                ensure_at_least(args, 1, "block", "<name>")?;
                Ok(Self::Block { name: args[0].clone() })
            }
            "abandon" => {
                ensure_at_least(args, 1, "abandon", "<name>")?;
                Ok(Self::Abandon { name: args[0].clone() })
            }
            "pending" => {
                ensure_at_least(args, 1, "pending", "<name>")?;
                Ok(Self::Pending { name: args[0].clone() })
            }
            "scope" => {
                ensure_at_least(args, 1, "scope", "<name> [text]")?;
                Ok(Self::Scope {
                    name: args[0].clone(),
                    text: args[1..].join(" "),
                })
            }
            "tag" => {
                ensure_at_least(args, 2, "tag", "<name> <tag> [<tag>...]")?;
                Ok(Self::Tag {
                    name: args[0].clone(),
                    tags: args[1..].to_vec(),
                })
            }
            "new" => {
                ensure_at_least(args, 1, "new", "<name> [goal]")?;
                Ok(Self::New {
                    name: args[0].clone(),
                    goal: args[1..].join(" "),
                })
            }
            "trash" => {
                ensure_at_least(args, 1, "trash", "<name>")?;
                Ok(Self::Trash { name: args[0].clone() })
            }
            "recover" => {
                ensure_at_least(args, 1, "recover", "<name>")?;
                Ok(Self::Recover { name: args[0].clone() })
            }
            "attach" => {
                ensure_exact(args, 2, "attach", "<name> <session-id>")?;
                validate_uuid(&args[1])?;
                Ok(Self::Attach {
                    name: args[0].clone(),
                    session_id: args[1].clone(),
                })
            }
            "group" => {
                ensure_at_least(args, 1, "group", "<name> [--group <g>] [--rank <r>] [--clear]")?;
                let name = args[0].clone();
                let mut group = None;
                let mut rank = None;
                let mut clear = false;
                let mut i = 1;
                while i < args.len() {
                    match args[i].as_str() {
                        "--group" | "-g" => {
                            i += 1;
                            if i < args.len() { group = Some(args[i].clone()); }
                        }
                        "--rank" | "-r" => {
                            i += 1;
                            if i < args.len() { rank = Some(args[i].clone()); }
                        }
                        "--clear" => clear = true,
                        other => anyhow::bail!("unknown flag '{}' in group", other),
                    }
                    i += 1;
                }
                Ok(Self::Group { name, group, rank, clear })
            }
            "next" => {
                ensure_exact(args, 1, "next", "<group>")?;
                Ok(Self::Next { group: args[0].clone() })
            }
            unknown => {
                anyhow::bail!(
                    "unknown sequence command '{}'. Supported: start, complete, block, abandon, \
                     pending, scope, tag, new, trash, recover, attach, group, next",
                    unknown
                );
            }
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn ensure_at_least(args: &[String], min: usize, cmd: &str, usage: &str) -> Result<()> {
    if args.len() < min {
        anyhow::bail!("'{}' requires {} {}", cmd, usage_quantifier(min), usage);
    }
    Ok(())
}

fn ensure_exact(args: &[String], count: usize, cmd: &str, usage: &str) -> Result<()> {
    if args.len() != count {
        anyhow::bail!("'{}' requires {}", cmd, usage);
    }
    Ok(())
}

fn usage_quantifier(min: usize) -> &'static str {
    if min == 1 { "a" } else { "at least" }
}

/// Apply a single sequence operation to a loaded registry in memory.
/// Returns the output line(s) to print — these mirror the standalone
/// `run_*` output formats exactly.
pub(crate) fn apply_op(
    reg: &mut WorkspaceRegistry,
    op: &SeqOp,
    now: &str,
) -> Result<Vec<String>> {
    match op {
        SeqOp::Start { name } => {
            let s = get_mut(&mut reg.sessions, name)?;
            if !SessionStatus::transition_allowed(s.status, SessionStatus::InProgress) {
                anyhow::bail!(
                    "cannot transition session '{}' from {} to in_progress",
                    name, s.status
                );
            }
            s.status = SessionStatus::InProgress;
            Ok(vec![status_line(s.status, name, "start")])
        }
        SeqOp::Complete { name } => {
            let s = get_mut(&mut reg.sessions, name)?;
            if !SessionStatus::transition_allowed(s.status, SessionStatus::Completed) {
                anyhow::bail!(
                    "cannot transition session '{}' from {} to completed",
                    name, s.status
                );
            }
            s.status = SessionStatus::Completed;
            if s.completed.is_empty() {
                s.completed = now.to_string();
            }
            Ok(vec![status_line(s.status, name, "complete")])
        }
        SeqOp::Block { name } => {
            let s = get_mut(&mut reg.sessions, name)?;
            if !SessionStatus::transition_allowed(s.status, SessionStatus::Blocked) {
                anyhow::bail!(
                    "cannot transition session '{}' from {} to blocked",
                    name, s.status
                );
            }
            s.status = SessionStatus::Blocked;
            Ok(vec![status_line(s.status, name, "block")])
        }
        SeqOp::Abandon { name } => {
            let s = get_mut(&mut reg.sessions, name)?;
            if !SessionStatus::transition_allowed(s.status, SessionStatus::Abandoned) {
                anyhow::bail!(
                    "cannot transition session '{}' from {} to abandoned",
                    name, s.status
                );
            }
            s.status = SessionStatus::Abandoned;
            if s.completed.is_empty() {
                s.completed = now.to_string();
            }
            Ok(vec![status_line(s.status, name, "abandon")])
        }
        SeqOp::Pending { name } => {
            let s = get_mut(&mut reg.sessions, name)?;
            s.status = SessionStatus::Pending;
            s.session_id.clear();
            s.pids.clear();
            s.started.clear();
            s.completed.clear();
            s.consumer.clear();
            s.retired_session_ids.clear();
            s.group = None;
            s.tags.clear();
            s.depends_on.clear();
            Ok(vec![format!(
                "pending     {}  ← reset (identity fields cleared)",
                name
            )])
        }
        SeqOp::Scope { name, text } => {
            let s = get_mut(&mut reg.sessions, name)?;
            s.scope = text.clone();
            Ok(vec![format!("{:12}  {}  ← scope updated", s.status, name)])
        }
        SeqOp::Tag { name, tags } => {
            let s = get_mut(&mut reg.sessions, name)?;
            s.tags = tags.clone();
            Ok(vec![
                format!("{:12}  {}  ← tagged", s.status, name),
                format!("  tags: {}", tags.join(", ")),
            ])
        }
        SeqOp::New { name, goal } => {
            if !crate::registry::is_kebab_case(name) {
                anyhow::bail!("session name '{}' must be kebab-case (lowercase, digits, hyphens only)", name);
            }
            if reg.sessions.iter().any(|s| s.name == *name) {
                anyhow::bail!("session '{}' already exists", name);
            }
            reg.sessions.push(WorkspaceSession {
                session_id: String::new(),
                name: name.clone(),
                goal: goal.clone(),
                scope: String::new(),
                status: SessionStatus::Pending,
                pids: vec![],
                tags: vec![],
                started: String::new(),
                completed: String::new(),
                group: None,
                depends_on: vec![],
                branch: String::new(),
                use_worktree: false,
                is_orchestrator: false,
                retired_session_ids: vec![],
                consumer: String::new(),
            });
            Ok(vec![format!("pending     {}  ← created", name)])
        }
        SeqOp::Trash { name } => {
            let s = get_mut(&mut reg.sessions, name)?;
            s.status = SessionStatus::Trashed;
            Ok(vec![format!(
                "trashed     {}  ← soft-deleted (recover with `ccsm recover {}`)",
                name, name
            )])
        }
        SeqOp::Recover { name } => {
            let s = get_mut(&mut reg.sessions, name)?;
            if !SessionStatus::transition_allowed(s.status, SessionStatus::InProgress) {
                anyhow::bail!(
                    "cannot recover session '{}' from {}",
                    name, s.status
                );
            }
            s.status = SessionStatus::InProgress;
            Ok(vec![format!("recovered   {}  ← in_progress", name)])
        }
        SeqOp::Attach { name, session_id } => {
            let s = get_mut(&mut reg.sessions, name)?;
            s.session_id = session_id.clone();
            let short = &session_id[..session_id.len().min(8)];
            Ok(vec![format!("attached    {}  ← session {}", name, short)])
        }
        SeqOp::Group { name, group, rank, clear } => {
            use crate::registry::{Group, GroupRank};
            let s = get_mut(&mut reg.sessions, name)?;
            if *clear {
                if let Some(old) = s.group.take() {
                    Ok(vec![format!("{}  ← removed from group '{}'", name, old.name)])
                } else {
                    Ok(vec![format!("{} is not in a group", name)])
                }
            } else if let Some(group_name) = group {
                let rank = match rank.as_deref() {
                    None => GroupRank::Free,
                    Some("free") => GroupRank::Free,
                    Some(n) => {
                        let num: u32 = n.parse()
                            .map_err(|_| anyhow::anyhow!("rank must be 'free' or a number, got '{}'", n))?;
                        GroupRank::Number(num)
                    }
                };
                s.group = Some(Group { name: group_name.clone(), rank });
                Ok(vec![format!("{}  ← group '{}' (rank: {})", name, group_name, rank)])
            } else {
                // Overview: list sessions in this group
                let members: Vec<_> = reg.sessions.iter()
                    .filter(|s| s.group.as_ref().is_some_and(|g| g.name == *name))
                    .collect();
                let mut lines = vec![format!("group '{}':", name)];
                for m in &members {
                    let rank_str = m.group.as_ref().map(|g| g.rank.to_string()).unwrap_or_default();
                    lines.push(format!("  {:12}  {:30}  rank: {}", m.status.to_string(), m.name, rank_str));
                }
                lines.push(format!("{} member{}", members.len(), if members.len() == 1 { "" } else { "s" }));
                Ok(lines)
            }
        }
        SeqOp::Next { group } => {
            use crate::registry::{GroupRank, SessionStatus};
            let mut members: Vec<_> = reg.sessions.iter()
                .filter(|s| s.group.as_ref().is_some_and(|g| g.name == *group))
                .collect();
            if members.is_empty() {
                anyhow::bail!("no sessions in group '{}'", group);
            }
            members.sort_by(|a, b| {
                let ra = a.group.as_ref().map(|g| &g.rank);
                let rb = b.group.as_ref().map(|g| &g.rank);
                match (ra, rb) {
                    (Some(GroupRank::Number(na)), Some(GroupRank::Number(nb))) => na.cmp(nb),
                    (Some(GroupRank::Number(_)), Some(GroupRank::Free)) => std::cmp::Ordering::Greater,
                    (Some(GroupRank::Free), Some(GroupRank::Number(_))) => std::cmp::Ordering::Less,
                    _ => a.name.cmp(&b.name),
                }
            });
            let in_progress: Vec<_> = members.iter().filter(|m| m.status == SessionStatus::InProgress).collect();
            let pick = if in_progress.len() == 1 {
                in_progress[0]
            } else if in_progress.len() > 1 {
                in_progress.iter().max_by_key(|m| &m.started).unwrap_or(&in_progress[0])
            } else {
                let pending: Vec<_> = members.iter().filter(|m| m.status == SessionStatus::Pending).collect();
                match pending.first() {
                    Some(p) => *p,
                    None => return Ok(vec![]), // all done — no output
                }
            };
            Ok(vec![pick.name.clone()])
        }
    }
}

/// Reject strings that don't look like UUIDs (8-4-4-4-12 hex) or OpenCode ses_* format.
fn validate_uuid(s: &str) -> Result<()> {
    // Accept OpenCode ses_* format (e.g. ses_abc123...)
    if s.starts_with("ses_") && s.len() > 4
        && s[4..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Ok(());
    }
    // Accept standard 8-4-4-4-12 UUID
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() == 5
        && parts[0].len() == 8
        && parts[1].len() == 4
        && parts[2].len() == 4
        && parts[3].len() == 4
        && parts[4].len() == 12
        && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
    {
        Ok(())
    } else {
        anyhow::bail!(
            "'{}' does not look like a session UUID (e.g. f493397b-...-4d5f15da0311).\n\
             Use --pid <pid> instead: ccsm attach <name> --pid <pid>",
            s
        );
    }
}

fn get_mut<'a>(
    sessions: &'a mut [WorkspaceSession],
    name: &str,
) -> Result<&'a mut WorkspaceSession> {
    sessions
        .iter_mut()
        .find(|s| s.name == name)
        .ok_or_else(|| anyhow::anyhow!("no session named '{}'", name))
}

/// Format a status line matching the `mutate_session` output in main.rs:
/// `"{:12}  {}  ← {}"` with status, name, action.
fn status_line(status: SessionStatus, name: &str, action: &str) -> String {
    format!("{:12}  {}  ← {}", status, name, action)
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parse tests ───────────────────────────────────────────────

    fn tokens(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parse_start() {
        let op = SeqOp::parse(&tokens("start my-session")).unwrap();
        assert_eq!(op, SeqOp::Start { name: "my-session".into() });
    }

    #[test]
    fn parse_complete() {
        let op = SeqOp::parse(&tokens("complete my-session")).unwrap();
        assert_eq!(op, SeqOp::Complete { name: "my-session".into() });
    }

    #[test]
    fn parse_block() {
        let op = SeqOp::parse(&tokens("block my-session")).unwrap();
        assert_eq!(op, SeqOp::Block { name: "my-session".into() });
    }

    #[test]
    fn parse_abandon() {
        let op = SeqOp::parse(&tokens("abandon my-session")).unwrap();
        assert_eq!(op, SeqOp::Abandon { name: "my-session".into() });
    }

    #[test]
    fn parse_pending() {
        let op = SeqOp::parse(&tokens("pending my-session")).unwrap();
        assert_eq!(op, SeqOp::Pending { name: "my-session".into() });
    }

    #[test]
    fn parse_scope_multi_word() {
        let op = SeqOp::parse(&tokens("scope foo implement the X feature")).unwrap();
        assert_eq!(op, SeqOp::Scope {
            name: "foo".into(),
            text: "implement the X feature".into(),
        });
    }

    #[test]
    fn parse_scope_empty_text() {
        let op = SeqOp::parse(&tokens("scope foo")).unwrap();
        assert_eq!(op, SeqOp::Scope { name: "foo".into(), text: String::new() });
    }

    #[test]
    fn parse_tag_multiple() {
        let op = SeqOp::parse(&tokens("tag foo urgent frontend bug")).unwrap();
        assert_eq!(op, SeqOp::Tag {
            name: "foo".into(),
            tags: vec!["urgent".into(), "frontend".into(), "bug".into()],
        });
    }

    #[test]
    fn parse_tag_missing_tags() {
        let err = SeqOp::parse(&tokens("tag foo")).unwrap_err().to_string();
        assert!(err.contains("tag"), "expected error about tag, got: {}", err);
    }

    #[test]
    fn parse_new_with_goal() {
        let op = SeqOp::parse(&tokens("new foo implement the feature")).unwrap();
        assert_eq!(op, SeqOp::New {
            name: "foo".into(),
            goal: "implement the feature".into(),
        });
    }

    #[test]
    fn parse_new_no_goal() {
        let op = SeqOp::parse(&tokens("new foo")).unwrap();
        assert_eq!(op, SeqOp::New { name: "foo".into(), goal: String::new() });
    }

    #[test]
    fn parse_trash() {
        let op = SeqOp::parse(&tokens("trash foo")).unwrap();
        assert_eq!(op, SeqOp::Trash { name: "foo".into() });
    }

    #[test]
    fn parse_recover() {
        let op = SeqOp::parse(&tokens("recover foo")).unwrap();
        assert_eq!(op, SeqOp::Recover { name: "foo".into() });
    }

    #[test]
    fn parse_attach() {
        let op = SeqOp::parse(&tokens("attach foo f493397b-456a-426d-92e1-4d5f15da0311")).unwrap();
        assert_eq!(op, SeqOp::Attach {
            name: "foo".into(),
            session_id: "f493397b-456a-426d-92e1-4d5f15da0311".into(),
        });
    }

    #[test]
    fn parse_attach_rejects_non_uuid() {
        let err = SeqOp::parse(&tokens("attach foo smith-system")).unwrap_err().to_string();
        assert!(err.contains("does not look like a session UUID"), "got: {}", err);
    }

    #[test]
    fn parse_attach_missing_id() {
        let err = SeqOp::parse(&tokens("attach foo")).unwrap_err().to_string();
        assert!(err.contains("attach"), "expected error about attach, got: {}", err);
    }

    #[test]
    fn parse_attach_too_many() {
        let err = SeqOp::parse(&tokens("attach foo a b")).unwrap_err().to_string();
        assert!(err.contains("attach"), "expected error about attach, got: {}", err);
    }

    #[test]
    fn parse_unknown_command() {
        let err = SeqOp::parse(&tokens("explode foo")).unwrap_err().to_string();
        assert!(err.contains("unknown") && err.contains("explode"),
            "expected unknown command error, got: {}", err);
    }

    #[test]
    fn parse_empty_tokens() {
        let err = SeqOp::parse(&[]).unwrap_err().to_string();
        assert!(err.contains("expected a command"), "got: {}", err);
    }

    #[test]
    fn parse_missing_name() {
        let err = SeqOp::parse(&tokens("start")).unwrap_err().to_string();
        assert!(err.contains("start") && err.contains("requires"),
            "got: {}", err);
    }

    // ── apply_op tests ────────────────────────────────────────────

    fn make_reg() -> WorkspaceRegistry {
        let mut reg = WorkspaceRegistry::empty();
        reg.sessions.push(WorkspaceSession {
            session_id: "abc-123".into(),
            name: "test-session".into(),
            goal: "test goal".into(),
            scope: String::new(),
            status: SessionStatus::Pending,
            pids: vec![],
            tags: vec![],
            started: String::new(),
            completed: String::new(),
            group: None,
            depends_on: vec![],
            branch: String::new(),
            use_worktree: false,
            is_orchestrator: false,
            retired_session_ids: vec![],
            consumer: String::new(),
        });
        reg
    }

    #[test]
    fn apply_start_sets_in_progress() {
        let mut reg = make_reg();
        let op = SeqOp::Start { name: "test-session".into() };
        let lines = apply_op(&mut reg, &op, "now").unwrap();
        assert_eq!(reg.sessions[0].status, SessionStatus::InProgress);
        assert!(!lines.is_empty());
    }

    #[test]
    fn apply_complete_sets_completed_and_timestamp() {
        let mut reg = make_reg();
        // Start it first so status transitions make sense
        reg.sessions[0].status = SessionStatus::InProgress;
        let op = SeqOp::Complete { name: "test-session".into() };
        let lines = apply_op(&mut reg, &op, "day1T12:00:00Z").unwrap();
        assert_eq!(reg.sessions[0].status, SessionStatus::Completed);
        assert_eq!(reg.sessions[0].completed, "day1T12:00:00Z");
        assert!(!lines.is_empty());
    }

    #[test]
    fn apply_complete_preserves_existing_completed() {
        let mut reg = make_reg();
        reg.sessions[0].status = SessionStatus::InProgress;
        reg.sessions[0].completed = "day0T00:00:00Z".into();
        let op = SeqOp::Complete { name: "test-session".into() };
        apply_op(&mut reg, &op, "day1T12:00:00Z").unwrap();
        // Should NOT overwrite existing completed timestamp
        assert_eq!(reg.sessions[0].completed, "day0T00:00:00Z");
    }

    #[test]
    fn apply_new_creates_entry() {
        let mut reg = make_reg();
        let op = SeqOp::New { name: "new-session".into(), goal: "do stuff".into() };
        let lines = apply_op(&mut reg, &op, "now").unwrap();
        assert_eq!(reg.sessions.len(), 2);
        assert_eq!(reg.sessions[1].name, "new-session");
        assert_eq!(reg.sessions[1].goal, "do stuff");
        assert_eq!(reg.sessions[1].status, SessionStatus::Pending);
        assert!(!lines.is_empty());
    }

    #[test]
    fn apply_new_duplicate_fails() {
        let mut reg = make_reg();
        let op = SeqOp::New { name: "test-session".into(), goal: String::new() };
        let err = apply_op(&mut reg, &op, "now").unwrap_err().to_string();
        assert!(err.contains("already exists"), "got: {}", err);
    }

    #[test]
    fn apply_tag_returns_two_lines() {
        let mut reg = make_reg();
        let op = SeqOp::Tag { name: "test-session".into(), tags: vec!["a".into(), "b".into()] };
        let lines = apply_op(&mut reg, &op, "now").unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(reg.sessions[0].tags, vec!["a", "b"]);
    }

    #[test]
    fn apply_scope_sets_text() {
        let mut reg = make_reg();
        let op = SeqOp::Scope { name: "test-session".into(), text: "new scope text".into() };
        apply_op(&mut reg, &op, "now").unwrap();
        assert_eq!(reg.sessions[0].scope, "new scope text");
    }

    #[test]
    fn apply_trash_then_recover() {
        let mut reg = make_reg();
        let trash = SeqOp::Trash { name: "test-session".into() };
        apply_op(&mut reg, &trash, "now").unwrap();
        assert_eq!(reg.sessions[0].status, SessionStatus::Trashed);

        let recover = SeqOp::Recover { name: "test-session".into() };
        apply_op(&mut reg, &recover, "now").unwrap();
        assert_eq!(reg.sessions[0].status, SessionStatus::InProgress);
    }

    #[test]
    fn apply_pending_clears_identity_fields() {
        let mut reg = make_reg();
        reg.sessions[0].status = SessionStatus::InProgress;
        reg.sessions[0].session_id = "should-be-cleared".into();
        reg.sessions[0].pids = vec![42];
        reg.sessions[0].started = "day0".into();
        reg.sessions[0].completed = "day1".into();
        reg.sessions[0].consumer = "claude".into();
        reg.sessions[0].retired_session_ids.push(crate::registry::RetiredSession {
            id: "old-id".into(),
            retired_at: "day0".into(),
            reason: "test".into(),
        });
        reg.sessions[0].group = Some(crate::registry::Group {
            name: "old-group".into(),
            rank: crate::registry::GroupRank::Free,
        });
        reg.sessions[0].tags = vec!["old-tag".into()];
        reg.sessions[0].depends_on = vec!["old-dep".into()];

        let op = SeqOp::Pending { name: "test-session".into() };
        apply_op(&mut reg, &op, "now").unwrap();
        let s = &reg.sessions[0];
        assert_eq!(s.status, SessionStatus::Pending);
        assert!(s.session_id.is_empty());
        assert!(s.pids.is_empty());
        assert!(s.started.is_empty());
        assert!(s.completed.is_empty());
        assert!(s.consumer.is_empty());
        assert!(s.retired_session_ids.is_empty());
        assert!(s.group.is_none());
        assert!(s.tags.is_empty());
        assert!(s.depends_on.is_empty());
    }

    #[test]
    fn apply_session_not_found() {
        let mut reg = make_reg();
        let op = SeqOp::Start { name: "nonexistent".into() };
        let err = apply_op(&mut reg, &op, "now").unwrap_err().to_string();
        assert!(err.contains("no session named"), "got: {}", err);
    }

    #[test]
    fn pipeline_new_start_complete() {
        let mut reg = make_reg();
        let now = "day99T99:99:99Z";

        // New
        let op = SeqOp::New { name: "pipeline-test".into(), goal: "pipeline".into() };
        apply_op(&mut reg, &op, now).unwrap();
        assert_eq!(reg.sessions.len(), 2);

        // Start
        let op = SeqOp::Start { name: "pipeline-test".into() };
        apply_op(&mut reg, &op, now).unwrap();
        assert_eq!(reg.sessions[1].status, SessionStatus::InProgress);

        // Complete
        let op = SeqOp::Complete { name: "pipeline-test".into() };
        apply_op(&mut reg, &op, now).unwrap();
        assert_eq!(reg.sessions[1].status, SessionStatus::Completed);
        assert_eq!(reg.sessions[1].completed, now);
    }
}
