use std::io::{self, BufRead, IsTerminal, Write};

use crate::registry::{SessionStatus, WorkspaceRegistry};
use crate::style;

/// Run the interactive tutorial: walks through the full session lifecycle
/// using a throwaway sandbox session named "ccsm-tutorial".
///
/// Completely silent (no-op) when stderr is not a terminal.
pub fn run_tutorial() -> anyhow::Result<()> {
    if !std::io::stderr().is_terminal() {
        return Ok(());
    }

    let tutorial_name = "ccsm-tutorial";

    // Check or warn about existing tutorial session
    let reg = WorkspaceRegistry::load()?;
    let existing = reg.sessions.iter().find(|s| s.name == tutorial_name);
    if let Some(sess) = existing {
        match sess.status {
            SessionStatus::InProgress => {
                eprintln!(
                    "{} A tutorial session is already in progress.",
                    style::emoji("ℹ", "[i]")
                );
                eprintln!("  Finish it with `ccsm complete {}` first.", tutorial_name);
                eprintln!(
                    "  Or: `ccsm pending {}` to reset and restart.",
                    tutorial_name
                );
                return Ok(());
            }
            SessionStatus::Completed | SessionStatus::Abandoned | SessionStatus::Blocked => {
                // Clean up old tutorial sessions
                let _ = std::process::Command::new("ccsm")
                    .args(["clean", tutorial_name])
                    .output();
            }
            _ => {}
        }
    }

    eprintln!(
        "{}",
        style::primary("╭─────────────────────────────────────────────╮")
    );
    eprintln!(
        "{}",
        style::primary("│       ccsm Tutorial — Session Lifecycle     │")
    );
    eprintln!(
        "{}",
        style::primary("╰─────────────────────────────────────────────╯")
    );
    eprintln!();
    eprintln!("  This walkthrough creates a throwaway session and guides you");
    eprintln!("  through the full lifecycle. Each step explains the command,");
    eprintln!("  runs it, and shows the result.");
    eprintln!();

    // Step 0: Remove leftover tutorial session
    let _ = std::process::Command::new("ccsm")
        .args(["clean", tutorial_name])
        .output();

    // Step 1: new
    step(
        1,
        "Create a new session",
        "ccsm new <name> -g \"<goal>\"",
        &[
            "ccsm",
            "new",
            tutorial_name,
            "-g",
            "Learn the session lifecycle",
        ],
    )?;

    // Step 2: start
    step(
        2,
        "Start the session",
        "ccsm start <name>",
        &["ccsm", "start", tutorial_name],
    )?;

    // Step 3: note
    step(
        3,
        "Take a progress note",
        "ccsm note <name> \"<text>\"",
        &[
            "ccsm",
            "note",
            tutorial_name,
            "Started the tutorial — this note documents the first step",
        ],
    )?;

    // Step 4: scope
    step(
        4,
        "Set the session scope",
        "ccsm scope <name> \"<2-4 sentences>\"",
        &[
            "ccsm",
            "scope",
            tutorial_name,
            "Follow the ccsm tutorial walkthrough to learn the session lifecycle. Covers new, start, note, scope, tag, close, and complete commands.",
        ],
    )?;

    // Step 5: tag
    step(
        5,
        "Tag the session",
        "ccsm tag <name> <tag1> <tag2> ...",
        &[
            "ccsm",
            "tag",
            tutorial_name,
            "tutorial",
            "learning",
            "onboarding",
        ],
    )?;

    // Step 6: close (gate review)
    step(
        6,
        "Review before completing",
        "ccsm close <name>",
        &["ccsm", "close", tutorial_name],
    )?;

    // Step 7: complete
    step(
        7,
        "Mark the session complete",
        "ccsm complete <name>",
        &["ccsm", "complete", tutorial_name],
    )?;

    // Done!
    eprintln!();
    eprintln!("{}", style::success(style::emoji("✓", "[*]")));
    eprintln!(
        "{}",
        style::primary("Tutorial complete! You've learned the full session lifecycle.")
    );
    eprintln!();
    eprintln!(
        "  {}  {}",
        style::emoji("📄", "[*]"),
        style::primary("ccsm new <name>")
    );
    eprintln!(
        "  {}  {}",
        style::emoji("▶", "[*]"),
        style::primary("ccsm start <name>")
    );
    eprintln!(
        "  {}  {}",
        style::emoji("📝", "[*]"),
        style::primary("ccsm note <name>")
    );
    eprintln!(
        "  {}  {}",
        style::emoji("🎯", "[*]"),
        style::primary("ccsm scope <name>")
    );
    eprintln!(
        "  {}  {}",
        style::emoji("🏷", "[*]"),
        style::primary("ccsm tag <name>")
    );
    eprintln!(
        "  {}  {}",
        style::emoji("✓", "[*]"),
        style::primary("ccsm complete <name>")
    );
    eprintln!();
    eprintln!(
        "  {}  ccsm help commands    — browse all commands by category",
        style::emoji("ℹ", "[?]")
    );
    eprintln!(
        "  {}  ccsm help <command>   — detailed help with examples",
        style::emoji("ℹ", "[?]")
    );
    eprintln!();
    eprintln!(
        "  {}",
        style::dim("The tutorial session 'ccsm-tutorial' is now completed and archived.")
    );
    eprintln!(
        "  {}",
        style::dim("To create real sessions, use `ccsm new <name> -g \"<goal>\"`.")
    );
    eprintln!();

    Ok(())
}

/// Run a tutorial step: print explanation, prompt, run command, show output.
fn step(
    step_num: usize,
    description: &str,
    command_example: &str,
    args: &[&str],
) -> anyhow::Result<()> {
    eprintln!();
    eprintln!(
        "{}",
        style::primary(&format!("─── Step {}: {} ───", step_num, description))
    );
    eprintln!();
    eprintln!(
        "  Command: {}",
        crate::commands::help::style_cmd(command_example)
    );
    eprintln!();

    eprint!("  Press Enter to run this command... ");
    io::stderr().flush().ok();

    // Read a line of input (or just pause)
    let stdin = io::stdin();
    let mut line = String::new();
    if stdin.is_terminal() {
        stdin.lock().read_line(&mut line).ok();
    }

    eprintln!();

    // Run the command
    let output = std::process::Command::new(args[0])
        .args(&args[1..])
        .output()?;

    // Show output
    if !output.stdout.is_empty() {
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        for line in stdout_str.lines() {
            eprintln!("  {}", line);
        }
    }
    if !output.stderr.is_empty() {
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        for line in stderr_str.lines() {
            eprintln!("  {}", line);
        }
    }

    if !output.status.success() {
        eprintln!(
            "  {} command exited with status {}",
            style::emoji("⚠", "[!]"),
            output.status
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tutorial_noop_when_non_terminal() {
        // Should return Ok immediately without any side effects
        let result = run_tutorial();
        assert!(result.is_ok());
    }
}
