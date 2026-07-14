use std::path::Path;

use crate::registry;

/// Canonical session detail template — embedded so doctor can recreate it.
pub(crate) const TEMPLATE_CONTENT: &str = r#"# Session: {{name}}

> **{{status}}** | started {{started}} | completed {{completed}}

## Goal

{{goal}}

## Scope / Plan

{{scope}}

## Tags

{{tags}}

## Progress Log

<!--
  Append dated entries as work happens. Keep newest at top.
  Format: [YYYY-MM-DD HH:MM] <note>
-->

- [{{now}}] {{note}}

## Dependencies

<!-- Sessions this work depends on or is blocked by -->

{{dependencies}}

## Notes

<!-- Free-form: decisions, discoveries, gotchas, links -->
"#;

/// `ccsm doctor` — scan session registry and filesystem for health issues.
pub fn run_doctor(home: &Path, workspace: &Path) -> anyhow::Result<()> {
    let ctx = registry::resolve_or_create_identity()?;
    let consumer = crate::consumer::Consumer::detect(home, None);
    let reg = match crate::registry::WorkspaceRegistry::load() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("⚠ registry file is corrupt — some checks skipped\n   {:#}", e);
            eprintln!("   → fix the JSON manually, delete the file to start fresh, or use a JSON formatter\n");
            crate::registry::WorkspaceRegistry::empty()
        }
    };
    let proj_dir = consumer.projects_dir_for(home, workspace);
    let lock_path = registry::global_lock_path(&ctx.id);

    let mut warnings: Vec<String> = Vec::new();
    let mut infos: Vec<String> = Vec::new();
    let mut tips: Vec<String> = Vec::new();
    let mut healthy = 0usize;
    let mut auto_created: Vec<String> = Vec::new();

    let in_progress_count = reg.sessions.iter()
        .filter(|s| s.status == crate::registry::SessionStatus::InProgress)
        .count();

    for s in &reg.sessions {
        let mut session_issues = 0usize;

        // 1. Orphaned session_id (non-empty but transcript missing)
        if !s.session_id.is_empty() && !consumer.is_opencode() {
            let transcript = proj_dir.join(format!("{}.jsonl", s.session_id));
            if !transcript.exists() {
                warnings.push(format!(
                    "  orphaned session_id  {}\n    session_id {} — transcript not found\n    → ccsm pending {}",
                    s.name,
                    &s.session_id[..s.session_id.len().min(8)],
                    s.name,
                ));
                session_issues += 1;
            }
        }

        // 2. Dead PIDs
        for pid in &s.pids {
            let proc_path = std::path::PathBuf::from(format!("/proc/{pid}"));
            if !proc_path.exists() {
                infos.push(format!(
                    "  dead pid  {}\n    pid {} is no longer running (auto-cleaned on next resume)",
                    s.name, pid,
                ));
                session_issues += 1;
            }
        }

        // 3. Empty goal
        if s.goal.is_empty() && s.status != crate::registry::SessionStatus::Pending {
            warnings.push(format!(
                "  empty goal  {}\n    status is {} but goal is empty\n    → ccsm scope {} \"<keyword-rich description>\"",
                s.name, s.status, s.name,
            ));
            session_issues += 1;
        }

        // 3b. Vague goal — too short to be searchable (< 20 chars, non-empty)
        if !s.goal.is_empty() && s.goal.len() < 20 {
            tips.push(format!(
                "  vague goal  {}\n    goal is only {} chars — not searchable for agents\n    → ccsm scope {} \"<keyword-rich description>\"",
                s.name, s.goal.len(), s.name,
            ));
        }

        // 3c. Goal is identical to session name (no real description)
        if !s.goal.is_empty() && s.goal.trim() == s.name {
            tips.push(format!(
                "  name-as-goal  {}\n    goal equals session name — carries no searchable meaning\n    → ccsm scope {} \"<keyword-rich description>\"",
                s.name, s.name,
            ));
        }

        // 3d. CLI artifact in goal — e.g. "-g Audit ccsm stability..."
        if s.goal.starts_with("-g ") || s.goal.starts_with("-c ") {
            tips.push(format!(
                "  cli artifact in goal  {}\n    goal starts with '{}' — flag text leaked into goal field\n    → ccsm scope {} \"<keyword-rich description>\"",
                s.name, &s.goal[..3], s.name,
            ));
        }

        // 4. Empty scope on completed sessions
        if s.scope.is_empty() && s.status == crate::registry::SessionStatus::Completed {
            infos.push(format!(
                "  empty scope  {}\n    completed but no scope documented\n    → ccsm scope {} \"<approach>\"",
                s.name, s.name,
            ));
            session_issues += 1;
        }

        // 5. Missing detail file
        let detail = registry::global_detail_path(&ctx.id, &s.name);
        if !detail.exists() && !s.name.is_empty() {
            infos.push(format!(
                "  no detail file  {}\n    → ccsm scope {} \"<description>\" to create one",
                s.name, s.name,
            ));
            session_issues += 1;
        }

        // 6. Template residue in detail file
        if detail.exists()
            && let Ok(contents) = std::fs::read_to_string(&detail) {
                let mut residue: Vec<&str> = Vec::new();
                for line in contents.lines() {
                    let trimmed = line.trim();
                    // Skip HTML comments (template instructions)
                    if trimmed.starts_with("<!--") || trimmed.starts_with("-->") || trimmed == "-->" {
                        continue;
                    }
                    if trimmed.contains("(fill in") {
                        residue.push("(fill in)");
                    }
                    if trimmed.contains("{{") && trimmed.contains("}}") {
                        residue.push("{{placeholder}}");
                    }
                }
                if !residue.is_empty() {
                    residue.dedup();
                    warnings.push(format!(
                        "  template residue  {}\n    detail file has unfilled {} — status is {}\n    → edit detail file and fill the placeholder sections",
                        s.name,
                        residue.join(", "),
                        s.status,
                    ));
                    session_issues += 1;
                }
            }
        // 7. Worktree checks
        if !s.worktree.is_empty() {
            let wt_path = std::path::Path::new(&s.worktree);
            let mut wt_issue = false;
            if !wt_path.is_dir() {
                infos.push(format!(
                    "  stale worktree  {}\n    worktree path '{}' recorded but directory missing\n    → ccsm pending {}  or  set worktree to empty",
                    s.name, s.worktree, s.name,
                ));
                wt_issue = true;
            } else if s.status != crate::registry::SessionStatus::InProgress {
                warnings.push(format!(
                    "  orphaned worktree  {}\n    worktree at {} exists but session is {}\n    → ccsm worktree remove {}  or  ccsm start {}",
                    s.name, s.worktree, s.status, s.name, s.name,
                ));
                wt_issue = true;
            }
            if wt_issue { session_issues += 1; }
        } else if s.status == crate::registry::SessionStatus::InProgress
            && !s.branch.is_empty()
            && s.use_worktree
        {
            tips.push(format!(
                "  worktree not created  {}\n    session targets branch '{}' with --worktree but no worktree exists\n    → ccsm worktree create {}",
                s.name, s.branch, s.name,
            ));
        }

        if session_issues == 0 {
            healthy += 1;
        }
    }

    // 7. Excessive in_progress — hype mode detected
    if in_progress_count >= 20 {
        warnings.push(format!(
            "  {} in_progress sessions — hype mode detected. Close stale sessions with `ccsm complete <name>` or `ccsm abandon <name>`",
            in_progress_count,
        ));
    }

    // 8. Stale lock file
    if lock_path.exists() {
        infos.push(format!(
            "  stale lock file  {}\n    → rm {}  (if no ccsm command is running)",
            lock_path.display(),
            lock_path.display(),
        ));
    }

    // 9. Session aging — stale in_progress sessions
    for s in &reg.sessions {
        if s.status != crate::registry::SessionStatus::InProgress {
            continue;
        }
        let age_days = crate::registry::session_age_days(&s.started);
        if age_days >= 7 {
            warnings.push(format!(
                "  stale session ({}d)  {}\n    started {} — no activity in {} days\n    → ccsm close {} to review, or ccsm complete/abandon to close",
                age_days, s.name, &s.started[..s.started.len().min(16)], age_days, s.name,
            ));
        } else if age_days >= 2 {
            infos.push(format!(
                "  aging session ({}d)  {}\n    started {} — in_progress for {} days\n    → ccsm close {} if done",
                age_days, s.name, &s.started[..s.started.len().min(16)], age_days, s.name,
            ));
        }
    }
    // 11. Orphaned worktree directories (no matching session)
    let wt_dir = registry::global_data_dir(&ctx.id).join("worktrees");
    if wt_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&wt_dir) {
            for entry in entries.flatten() {
                let dir_name = entry.file_name();
                let name_str = dir_name.to_string_lossy();
                if entry.path().is_dir() && !reg.sessions.iter().any(|s| s.name == name_str) {
                    tips.push(format!(
                        "  orphaned worktree dir  {}\n    {} has no matching session\n    → git worktree remove \"{}\"",
                        name_str, entry.path().display(), entry.path().display(),
                    ));
                }
            }
        }
    }


    // 10. Large transcripts — candidates for archive
    let mut large: Vec<(String, u64)> = Vec::new();
    for s in &reg.sessions {
        if s.status == crate::registry::SessionStatus::Completed && !s.session_id.is_empty() && !consumer.is_opencode() {
            let transcript = proj_dir.join(format!("{}.jsonl", s.session_id));
            if let Ok(meta) = std::fs::metadata(&transcript) {
                let mb = meta.len() / 1_000_000;
                if mb > 0 {
                    large.push((s.name.clone(), mb));
                }
            }
        }
    }
    if !large.is_empty() {
        large.sort_by_key(|b| std::cmp::Reverse(b.1));
        let names: Vec<String> = large.iter().map(|(n, mb)| format!("{} ({} MB)", n, mb)).collect();
        let total: u64 = large.iter().map(|(_, mb)| *mb).sum();
        if total >= 5 {
            tips.push(format!(
                "  {} completed session{} with transcripts: {}\n    → ccsm archive-all to free {} MB",
                large.len(),
                if large.len() == 1 { "" } else { "s" },
                names.join(", "),
                total,
            ));
        }
    }

    // 10. Auto-create essential files if missing ────────────────────────
    let global = registry::global_data_dir(&ctx.id);
    let sessions_dir = global.join("sessions");
    let template_path = registry::global_template_path(&ctx.id);
    let group_dir = global.join("session-group");

    // 10a. sessions/ directory under global data dir
    if !sessions_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&sessions_dir) {
            warnings.push(format!(
                "  cannot create sessions dir  {}\n    {}",
                sessions_dir.display(), e,
            ));
        } else {
            auto_created.push(format!("  {}sessions/", global.display()));
        }
    }

    // 10b. session-detail-template.md in global data dir
    if !template_path.exists() {
        if let Err(e) = std::fs::write(&template_path, TEMPLATE_CONTENT) {
            warnings.push(format!(
                "  cannot create template  {}\n    {}",
                template_path.display(), e,
            ));
        } else {
            auto_created.push(format!("  {}", template_path.display()));
        }
    }

    // 10c. session-group/ directory under global data dir (non-essential)
    if !group_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&group_dir) {
            warnings.push(format!(
                "  cannot create group dir  {}\n    {}",
                group_dir.display(), e,
            ));
        }
    }

    // ── Print results ───────────────────────────────────────────────
    let any_issues = !warnings.is_empty() || !infos.is_empty() || !tips.is_empty() || !auto_created.is_empty();

    if !auto_created.is_empty() {
        println!("🔧 auto-created");
        for a in &auto_created { println!("{}", a); }
        println!();
    }

    if !warnings.is_empty() {
        println!("⚠ warnings (should fix)");
        for w in &warnings { println!("{}", w); }
        println!();
    }
    if !infos.is_empty() {
        println!("⚡ info");
        for i in &infos { println!("{}", i); }
        println!();
    }
    if !tips.is_empty() {
        println!("💡 tips");
        for t in &tips { println!("{}", t); }
        println!();
    }
    if healthy > 0 && any_issues {
        println!("✓ {} healthy session{}", healthy, if healthy == 1 { "" } else { "s" });
    } else if !any_issues {
        println!("✓ all {} session{} healthy", healthy, if healthy == 1 { "" } else { "s" });
    }

    Ok(())
}
