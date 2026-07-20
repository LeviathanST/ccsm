use anyhow::Result;
use std::io::ErrorKind;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub session: String,
    pub window: String,
    pub pane_index: String,
    pub pane_id: String,
    pub process: String,
}

pub fn list_panes(session: Option<&str>) -> Result<Vec<PaneInfo>> {
    let mut args = vec![
        "list-panes",
        "-a",
        "-F",
        "#{session_name}:#{window_index}:#{pane_index}:#{pane_id}:#{pane_current_command}",
    ];
    if let Some(s) = session {
        args = vec![
            "list-panes",
            "-s",
            "-t",
            s,
            "-F",
            "#{session_name}:#{window_index}:#{pane_index}:#{pane_id}:#{pane_current_command}",
        ];
    }
    let out = tmux(&args)?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(5, ':').collect();
            if parts.len() == 5 {
                Some(PaneInfo {
                    session: parts[0].to_string(),
                    window: parts[1].to_string(),
                    pane_index: parts[2].to_string(),
                    pane_id: parts[3].to_string(),
                    process: parts[4].to_string(),
                })
            } else {
                None
            }
        })
        .collect())
}

pub fn capture_pane(target: &str, tail_lines: Option<usize>) -> Result<String> {
    match tail_lines {
        Some(n) if n > 0 => {
            let line_arg = format!("-{}", n);
            tmux(&["capture-pane", "-p", "-t", target, "-S", &line_arg])
        }
        _ => tmux(&["capture-pane", "-p", "-t", target]),
    }
}

pub fn send_keys(target: &str, text: &str, enter: bool) -> Result<()> {
    if text.len() > 65536 {
        anyhow::bail!("text too long ({} bytes, max 65536)", text.len());
    }
    tmux(&["send-keys", "-t", target, "-l", text])?;
    if enter {
        tmux(&["send-keys", "-t", target, "Enter"])?;
    }
    Ok(())
}

#[allow(dead_code)]
pub fn kill_session(name: &str) -> Result<()> {
    tmux(&["kill-session", "-t", name]).map(|_| ())
}

pub fn check_tmux() -> Result<()> {
    tmux(&["start-server"]).map(|_| ())
}

fn tmux(args: &[&str]) -> Result<String> {
    let out = Command::new("tmux").args(args).output().map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
            anyhow::anyhow!("tmux binary not found. Install tmux and ensure it's in your PATH.")
        } else {
            anyhow::anyhow!("failed to execute tmux: {}", e)
        }
    })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("tmux error: {}", stderr.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
