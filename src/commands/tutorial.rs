use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use crate::style;

const TUTORIAL_DIR: &str = "ccsm-tutorial";

/// Run the interactive tutorial: walks through the full session lifecycle
/// using a throwaway sandbox so nothing touches your real project.
///
/// Completely silent (no-op) when stderr is not a terminal.
pub fn run_tutorial() -> anyhow::Result<()> {
    if !std::io::stderr().is_terminal() {
        return Ok(());
    }

    let cwd = std::env::current_dir()?;
    let project_dir = cwd.join(TUTORIAL_DIR);

    // Clean leftover sandbox from a previous run
    if project_dir.exists() {
        eprintln!(
            "  {} removing leftover sandbox from previous tutorial...",
            style::dim("[cleanup]")
        );
        std::fs::remove_dir_all(&project_dir)?;
    }

    // Create sandbox project directory
    std::fs::create_dir_all(&project_dir)?;

    // Create sandbox data directory
    let data_dir = std::env::temp_dir().join(format!("ccsm-tutorial-data-{}", std::process::id()));
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).ok();
    }
    std::fs::create_dir_all(&data_dir)?;

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
    eprintln!("  Sandbox project:  ./{}/", TUTORIAL_DIR);
    eprintln!("  Sandbox data:     {}", data_dir.display());
    eprintln!(
        "  {}",
        style::dim("Nothing touches your real project — the sandbox is deleted at the end.")
    );
    eprintln!();

    // Step 1: init
    sandboxed_step(
        &project_dir,
        &data_dir,
        1,
        "Initialize a project",
        "ccsm init",
        &["init"],
    )?;

    // Step 2: new
    sandboxed_step(
        &project_dir,
        &data_dir,
        2,
        "Create a new session",
        "ccsm new <name> -g \"<goal>\"",
        &["new", "ccsm-tutorial", "-g", "Learn the session lifecycle"],
    )?;

    // Step 3: start
    sandboxed_step(
        &project_dir,
        &data_dir,
        3,
        "Start the session",
        "ccsm start <name>",
        &["start", "ccsm-tutorial"],
    )?;

    // Step 4: note
    sandboxed_step(
        &project_dir,
        &data_dir,
        4,
        "Take a progress note",
        "ccsm note <name> \"<text>\"",
        &[
            "note",
            "ccsm-tutorial",
            "Started the tutorial — this note documents the first step",
        ],
    )?;

    // Step 5: scope
    sandboxed_step(
        &project_dir,
        &data_dir,
        5,
        "Set the session scope",
        "ccsm scope <name> \"<2-4 sentences>\"",
        &[
            "scope",
            "ccsm-tutorial",
            "Follow the ccsm tutorial walkthrough to learn the session lifecycle. Covers new, start, note, scope, tag, close, and complete commands.",
        ],
    )?;

    // Step 6: tag
    sandboxed_step(
        &project_dir,
        &data_dir,
        6,
        "Tag the session",
        "ccsm tag <name> <tag1> <tag2> ...",
        &["tag", "ccsm-tutorial", "tutorial", "learning", "onboarding"],
    )?;

    // Step 7: close (gate review)
    sandboxed_step(
        &project_dir,
        &data_dir,
        7,
        "Review before completing",
        "ccsm close <name>",
        &["close", "ccsm-tutorial"],
    )?;

    // Step 8: complete
    sandboxed_step(
        &project_dir,
        &data_dir,
        8,
        "Mark the session complete",
        "ccsm complete <name>",
        &["complete", "ccsm-tutorial"],
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
        "  {}  {}",
        style::emoji("🔄", "[*]"),
        style::primary("ccsm init   — start here for new projects")
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

    // Cleanup
    std::fs::remove_dir_all(&project_dir).ok();
    std::fs::remove_dir_all(&data_dir).ok();

    eprintln!(
        "  {}",
        style::dim("Sandbox cleaned up. ./ccsm-tutorial/ has been removed.")
    );
    eprintln!();

    Ok(())
}

/// Run a tutorial step inside the sandbox: print explanation, prompt,
/// run `ccsm` inside the sandbox project with isolated data dir.
fn sandboxed_step(
    project_dir: &PathBuf,
    data_dir: &PathBuf,
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

    let stdin = io::stdin();
    let mut line = String::new();
    if stdin.is_terminal() {
        stdin.lock().read_line(&mut line).ok();
    }

    eprintln!();

    let output = std::process::Command::new("ccsm")
        .args(args)
        .current_dir(project_dir)
        .env("CCSM_DATA_DIR", data_dir)
        .output()?;

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
        let result = run_tutorial();
        assert!(result.is_ok());
    }
}
