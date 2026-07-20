use std::io::IsTerminal;

/// Run the help subcommand: `ccsm help commands`, `ccsm help <command>`, `ccsm help tutorial`.
pub fn run_help(topic: &[String]) {
    let topic_str = topic
        .join(" ")
        .to_lowercase()
        .replace("  ", " ")
        .replace(' ', "-");

    match topic_str.as_str() {
        "" | "commands" => print_categorized(),
        "tutorial" => print_tutorial_info(),
        _ => print_command_help(&topic_str),
    }
}

fn print_categorized() {
    let categories = [
        ("Query", &["list", "scan", "show"] as &[&str]),
        (
            "Lifecycle",
            &["new", "start", "complete", "block", "abandon", "pending"],
        ),
        ("Content", &["scope", "tag", "note", "check", "checklist"]),
        ("Groups", &["group", "group-deps", "next", "depend"]),
        ("Resume & Attach", &["resume", "refresh", "attach"]),
        (
            "Cleanup",
            &[
                "trash",
                "recover",
                "clean",
                "clean-all",
                "archive",
                "archive-all",
            ],
        ),
        (
            "Maintenance",
            &[
                "doctor", "migrate", "init", "setup", "config", "branch", "close",
            ],
        ),
        (
            "Integration",
            &[
                "inject-scope",
                "gate-check",
                "note-check",
                "completions",
                "sequence",
            ],
        ),
        ("Help", &["help", "tutorial"]),
    ];

    let sep = style_dim("  │  ");
    println!("{}", style_primary("ccsm — Categorized Commands"));
    println!();
    for (cat, cmds) in &categories {
        let names: Vec<String> = cmds.iter().map(|c| style_cmd(c)).collect();
        println!("  {}  {}", style_cat(cat), names.join(&sep));
    }
    println!();
    println!(
        "  {}  ccsm help <command>  for detailed help with examples",
        style_dim("→")
    );
    println!(
        "  {}  ccsm --help         for the quick reference",
        style_dim("→")
    );
}

fn print_command_help(topic: &str) {
    let detail = match topic {
        "list" | "ls" => Some((
            "list",
            "List sessions. Multiple display modes: default (table), --active, --summary, --verbose, --json.",
            vec![
                "ccsm list                      # all sessions, table view",
                "ccsm list --active             # in_progress + blocked",
                "ccsm list --summary            # counts per status",
                "ccsm list -S in_progress       # filter by status",
                "ccsm list --json               # JSON output",
                "ccsm list -g my-group          # filter by group",
            ],
        )),
        "scan" | "sc" => Some((
            "scan",
            "Compact, grep-friendly output grouped by group. Built-in --search for text filtering.",
            vec![
                "ccsm scan                      # all sessions, grouped",
                "ccsm scan --search \"auth\"      # filter by name/goal/tags",
                "ccsm scan -g my-group          # filter by group",
                "ccsm scan --json               # JSON array",
                "ccsm scan -S in_progress       # filter by status",
            ],
        )),
        "show" => Some((
            "show",
            "Show session details: goal, scope, tags, session_id, pids, timestamps. Extract a single section with --section.",
            vec![
                "ccsm show my-session           # full detail",
                "ccsm show my-session --section progress-log  # extract one section",
                "ccsm show my-session --json     # JSON output",
            ],
        )),
        "new" => Some((
            "new",
            "Create a pending session entry. Optionally embed a ## Checklist section with -c. Use -b to associate with a branch, -w for worktree.",
            vec![
                "ccsm new my-feature -g \"Add dark mode\"",
                "ccsm new my-feature -c feat -g \"Add auth\"",
                "ccsm new my-feature -b feat/my-branch -g \"Work on feature\"",
            ],
        )),
        "start" => Some((
            "start",
            "Move a session from pending → in_progress. With --worktree (-w), also creates a git worktree on the session's target branch.",
            vec![
                "ccsm start my-feature          # mark active",
                "ccsm start my-feature -w       # create git worktree too",
            ],
        )),
        "complete" => Some((
            "complete",
            "Mark a session completed. Runs gate checks (detail file completeness) and refuses unless --force. Run ccsm close first.",
            vec![
                "ccsm close my-session          # review gate",
                "ccsm complete my-session       # mark done",
                "ccsm complete my-session --force  # skip gate",
            ],
        )),
        "block" => Some((
            "block",
            "Mark a session as blocked — waiting on a dependency.",
            vec!["ccsm block my-session          # mark blocked"],
        )),
        "abandon" => Some((
            "abandon",
            "Mark a session as abandoned — no longer relevant.",
            vec!["ccsm abandon my-session        # mark abandoned"],
        )),
        "pending" => Some((
            "pending",
            "Reset to pending, clears session_id + pids + timestamps.",
            vec!["ccsm pending my-session        # reset to pending"],
        )),
        "scope" => Some((
            "scope",
            "Set the session scope: 2-4 sentences on approach, constraints, what's in/out. Replaces any existing scope.",
            vec!["ccsm scope my-session Implement the auth module, add JWT support, write tests"],
        )),
        "tag" => Some((
            "tag",
            "Replace all tags on a session. Space-separated list. Overwrites existing tags.",
            vec!["ccsm tag my-session auth security rust"],
        )),
        "note" => Some((
            "note",
            "Append a timestamped entry to the session's Progress Log. Use -x for cross-session notes.",
            vec![
                "ccsm note my-session Fixed the auth bug",
                "ccsm note my-session -x other-session Cross-reference: shared dependency",
            ],
        )),
        "check" => Some((
            "check",
            "Toggle or add checklist items. ITEM can be a 1-based index, text substring, or new item text.",
            vec![
                "ccsm check my-session \"write tests\" -s pending   # add new item",
                "ccsm check my-session 1 -s done                  # mark #1 done",
                "ccsm check my-session \"write tests\" -s skipped   # mark by text",
            ],
        )),
        "checklist" => Some((
            "checklist",
            "Initialize the ## Checklist section in a session detail file. Use -i to create if missing.",
            vec![
                "ccsm checklist my-session      # view items",
                "ccsm checklist my-session -i   # init section",
            ],
        )),
        "group" => Some((
            "group",
            "Manage session groups. Assign, set goals, render roadmaps.",
            vec![
                "ccsm group --list              # list all groups",
                "ccsm group my-group            # show sessions in group",
                "ccsm group my-group --roadmap   # render roadmap (table + deps)",
                "ccsm group my-session --group backend  # assign to group",
                "ccsm group my-session --clear  # remove from group",
            ],
        )),
        "group-deps" => Some((
            "group-deps",
            "Show the dependency graph for all sessions in a group.",
            vec!["ccsm group-deps my-group       # render dep graph"],
        )),
        "next" => Some((
            "next",
            "Print the next session to work on in a group.",
            vec!["ccsm next my-group             # next session to work on"],
        )),
        "depend" => Some((
            "depend",
            "Manage session dependencies. Add or remove blocking dependencies.",
            vec![
                "ccsm depend my-session --on prereq  # depends on prereq",
                "ccsm depend my-session             # list dependencies",
                "ccsm depend my-session --clear      # remove all deps",
            ],
        )),
        "resume" => Some((
            "resume",
            "Spawn OpenCode with session context. Harvests session_id on exit.",
            vec![
                "ccsm resume my-session         # spawn OpenCode",
                "ccsm resume my-session -w      # spawn inside git worktree",
            ],
        )),
        "refresh" => Some((
            "refresh",
            "Retire current agent session, spawn fresh. Use when context is bloated.",
            vec![
                "ccsm refresh my-session                    # fresh spawn",
                "ccsm refresh my-session -r \"context 45%\"   # with reason",
            ],
        )),
        "attach" => Some((
            "attach",
            "Manually link a session UUID to a ccsm entry. Auto-discover, explicit UUID, or --pid.",
            vec![
                "ccsm attach my-session              # auto-discover",
                "ccsm attach my-session <uuid>       # explicit UUID",
                "ccsm attach my-session --pid <pid>  # harvest from PID",
            ],
        )),
        "trash" => Some((
            "trash",
            "Soft-delete a session. Recoverable with ccsm recover.",
            vec!["ccsm trash my-session           # soft-delete"],
        )),
        "recover" => Some((
            "recover",
            "Recover a trashed session. Undoes a trash.",
            vec!["ccsm recover my-session         # restore from trash"],
        )),
        "clean" => Some((
            "clean",
            "Permanently delete a session's transcript and files. Irreversible.",
            vec!["ccsm clean my-session           # permanent delete"],
        )),
        "clean-all" => Some((
            "clean-all",
            "Permanently delete ALL trashed entries. Irreversible.",
            vec!["ccsm clean-all                  # nuke all trashed"],
        )),
        "archive" => Some((
            "archive",
            "Archive: delete transcript + session files, keep registry entry.",
            vec!["ccsm archive my-session         # archive one"],
        )),
        "archive-all" => Some((
            "archive-all",
            "Archive all completed sessions with transcripts.",
            vec!["ccsm archive-all                # archive all done"],
        )),
        "doctor" => Some((
            "doctor",
            "Scan for health issues. Pass --fix to auto-clean fixable issues.",
            vec![
                "ccsm doctor                     # check health",
                "ccsm doctor --fix               # auto-fix issues",
            ],
        )),
        "migrate" => Some((
            "migrate",
            "Auto-chain migration from v0.0.0 to current. Safe to re-run.",
            vec!["ccsm migrate                    # run migrations"],
        )),
        "init" => Some((
            "init",
            "Initialize a .ccsm identity in this project.",
            vec!["ccsm init                       # set up identity"],
        )),
        "setup" => Some((
            "setup",
            "Install session tracking into global skills.",
            vec!["ccsm setup                      # install integration"],
        )),
        "config" => Some((
            "config",
            "View or modify project configuration.",
            vec![
                "ccsm config                     # show current config",
                "ccsm config set wip_limit 3     # set value",
                "ccsm config reset               # restore defaults",
            ],
        )),
        "branch" => Some((
            "branch",
            "Set or clear the target git branch for a session.",
            vec![
                "ccsm branch my-session feat/my-branch  # set branch",
                "ccsm branch my-session --clear         # clear it",
            ],
        )),
        "close" => Some((
            "close",
            "Pre-completion gate check.",
            vec!["ccsm close my-session          # gate review"],
        )),
        "inject-scope" => Some((
            "inject-scope",
            "Output <system-reminder> with goal, scope, checklist.",
            vec![
                "ccsm inject-scope              # for current session",
                "ccsm inject-scope my-session   # for specific session",
            ],
        )),
        "gate-check" => Some((
            "gate-check",
            "Check if work aligns with session scope.",
            vec![
                "ccsm gate-check                # check alignment",
                "ccsm gate-check --strict       # strict mode",
            ],
        )),
        "note-check" => Some((
            "note-check",
            "Stop-hook: remind to note progress when tree is dirty.",
            vec!["ccsm note-check                # check if note needed"],
        )),
        "completions" => Some((
            "completions",
            "Generate shell completion script (bash, fish, zsh).",
            vec![
                "ccsm completions bash          # bash completions",
                "ccsm completions fish          # fish completions",
                "ccsm completions zsh           # zsh completions",
            ],
        )),
        "sequence" => Some((
            "sequence",
            "Run multiple mutations in a single lock/load/save cycle.",
            vec!["ccsm sequence -q start foo -q scope foo bar -q complete foo"],
        )),
        "rename" => Some((
            "rename",
            "Rename a session across registry, detail file, transcript.",
            vec![
                "ccsm rename old-name new-name",
                "ccsm rename old-name new-name -g \"New goal\" -s \"New scope\"",
            ],
        )),
        "help" => Some((
            "help",
            "Browse help by category or get detailed help for a command.",
            vec![
                "ccsm help commands              # categorized list",
                "ccsm help <command>             # detailed help with examples",
                "ccsm help tutorial              # tutorial info",
                "ccsm --help                     # quick reference",
            ],
        )),
        "tutorial" => Some((
            "tutorial",
            "Interactive walkthrough of the session lifecycle.",
            vec!["ccsm tutorial                   # start the tutorial"],
        )),
        _ => None,
    };

    if let Some((name, desc, examples)) = detail {
        println!("{}  —  {}", style_cmd(name), desc);
        println!();
        for ex in &examples {
            println!("  {}  {}", style_dim("$"), style_code(ex));
        }
    } else {
        eprintln!("unknown help topic '{}'", topic);
        eprintln!("Try: ccsm help commands");
    }
}

fn print_tutorial_info() {
    if !std::io::stderr().is_terminal() {
        return;
    }
    eprintln!("{}", style_primary("ccsm Tutorial"));
    eprintln!();
    eprintln!("  Interactive walkthrough of the session lifecycle.");
    eprintln!();
    eprintln!("  Steps:  new → start → note → scope → tag → close → complete");
    eprintln!();
    eprintln!("  Uses a throwaway sandbox session named 'ccsm-tutorial'.");
    eprintln!("  At the end, you can keep it by renaming it.");
    eprintln!();
    eprintln!("  {}", style_cmd("ccsm tutorial"));
    eprintln!();
    eprintln!(
        "  {}",
        style_dim("Requires a terminal. Silent in non-interactive contexts.")
    );
}

// ── Styling helpers (thin wrappers around crate::style) ────────────

fn style_primary(s: &str) -> String {
    crate::style::primary(s)
}

fn style_dim(s: &str) -> String {
    crate::style::dim(s)
}

pub fn style_cmd(s: &str) -> String {
    format!("\x1b[1;36m{}\x1b[0m", s)
}

fn style_cat(s: &str) -> String {
    format!("\x1b[33m{}\x1b[0m", s)
}

fn style_code(s: &str) -> String {
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_commands_does_not_panic() {
        print_categorized();
    }

    #[test]
    fn help_all_known_topics() {
        let topics = [
            "list",
            "scan",
            "show",
            "new",
            "start",
            "complete",
            "block",
            "abandon",
            "pending",
            "scope",
            "tag",
            "note",
            "check",
            "checklist",
            "group",
            "group-deps",
            "next",
            "depend",
            "resume",
            "refresh",
            "attach",
            "trash",
            "recover",
            "clean",
            "clean-all",
            "archive",
            "archive-all",
            "doctor",
            "migrate",
            "init",
            "setup",
            "config",
            "branch",
            "close",
            "inject-scope",
            "gate-check",
            "note-check",
            "completions",
            "sequence",
            "rename",
            "help",
            "tutorial",
        ];
        for t in &topics {
            // Should not panic
            print_command_help(t);
        }
    }

    #[test]
    fn help_unknown_topic_prints_error() {
        // Just check it doesn't panic
        print_command_help("nonexistent-command");
    }

    #[test]
    fn help_empty_topic_shows_categorized() {
        print_categorized();
    }
}
