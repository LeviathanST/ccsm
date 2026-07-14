use std::process::Command;
use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub session: String,
    pub window: String,
    pub pane_index: String,
    pub pane_id: String,
    pub process: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub name: String,
    pub windows: usize,
}

#[allow(dead_code)]
pub fn list_sessions() -> Result<Vec<SessionInfo>> {
    let out = tmux(&["list-sessions", "-F", "#{session_name}:#{session_windows}"])?;
    Ok(out.lines().filter_map(|line| {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 {
            Some(SessionInfo {
                name: parts[0].to_string(),
                windows: parts[1].parse().unwrap_or(0),
            })
        } else {
            None
        }
    }).collect())
}

pub fn list_panes(session: Option<&str>) -> Result<Vec<PaneInfo>> {
    let mut args = vec!["list-panes", "-a", "-F",
        "#{session_name}:#{window_index}:#{pane_index}:#{pane_id}:#{pane_current_command}"];
    if let Some(s) = session {
        args = vec!["list-panes", "-s", "-t", s, "-F",
            "#{session_name}:#{window_index}:#{pane_index}:#{pane_id}:#{pane_current_command}"];
    }
    let out = tmux(&args)?;
    Ok(out.lines().filter_map(|line| {
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
    }).collect())
}

pub fn capture_pane(target: &str, tail_lines: Option<usize>) -> Result<String> {
    match tail_lines {
        Some(n) => {
            let line_arg = format!("-{}", n);
            tmux(&["capture-pane", "-p", "-t", target, "-S", &line_arg])
        }
        None => tmux(&["capture-pane", "-p", "-t", target]),
    }
}

pub fn send_keys(target: &str, text: &str, enter: bool) -> Result<()> {
    tmux(&["send-keys", "-t", target, "-l", text])?;
    if enter {
        tmux(&["send-keys", "-t", target, "Enter"])?;
    }
    Ok(())
}

#[allow(dead_code)]
pub fn new_session(name: &str, width: &str, height: &str) -> Result<()> {
    tmux(&["new-session", "-d", "-s", name, "-x", width, "-y", height])?;
    Ok(())
}

#[allow(dead_code)]
pub fn split_window(session: &str, horizontal: bool) -> Result<()> {
    let dir = if horizontal { "-h" } else { "-v" };
    tmux(&["split-window", dir, "-t", session])?;
    Ok(())
}

#[allow(dead_code)]
pub fn select_layout(session: &str, layout: &str) -> Result<()> {
    tmux(&["select-layout", "-t", session, layout])?;
    Ok(())
}

#[allow(dead_code)]
pub fn kill_session(name: &str) -> Result<()> {
    let _ = tmux(&["kill-session", "-t", name]);
    Ok(())
}

fn tmux(args: &[&str]) -> Result<String> {
    let out = Command::new("tmux")
        .args(args)
        .output()
        .context("failed to execute tmux")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("tmux error: {}", stderr.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
