use std::path::Path;

/// `ccsm doctor` — scan session registry and filesystem for health issues.
pub fn run_doctor(home: &Path, workspace: &Path) -> anyhow::Result<()> {
    let reg = crate::registry::WorkspaceRegistry::load(workspace)?;
    let slug = crate::registry::project_slug(workspace);
    let proj_dir = home.join(".claude").join("projects").join(&slug);
    let lock_path = workspace.join(".claude").join("sessions.json.lock");

    let mut warnings: Vec<String> = Vec::new();
    let mut infos: Vec<String> = Vec::new();
    let mut tips: Vec<String> = Vec::new();
    let mut healthy = 0usize;

    let in_progress_count = reg.sessions.iter()
        .filter(|s| s.status == crate::registry::SessionStatus::InProgress)
        .count();

    for s in &reg.sessions {
        let mut session_issues = 0usize;

        // 1. Orphaned session_id (non-empty but transcript missing)
        if !s.session_id.is_empty() {
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
                "  empty goal  {}\n    status is {} but goal is empty\n    → edit .claude/sessions/{}.md",
                s.name, s.status, s.name,
            ));
            session_issues += 1;
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
        let detail = workspace.join(".claude").join("sessions").join(format!("{}.md", s.name));
        if !detail.exists() && !s.name.is_empty() {
            infos.push(format!(
                "  no detail file  {}\n    → cp .claude/session-detail-template.md .claude/sessions/{}.md",
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
                        "  template residue  {}\n    detail file has unfilled {} — status is {}\n    → edit .claude/sessions/{}.md and fill the placeholder sections",
                        s.name,
                        residue.join(", "),
                        s.status,
                        s.name,
                    ));
                    session_issues += 1;
                }
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

    // 9. Large transcripts — candidates for archive
    let mut large: Vec<(String, u64)> = Vec::new();
    for s in &reg.sessions {
        if s.status == crate::registry::SessionStatus::Completed && !s.session_id.is_empty() {
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

    // ── Print results ───────────────────────────────────────────────
    let any_issues = !warnings.is_empty() || !infos.is_empty() || !tips.is_empty();

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
