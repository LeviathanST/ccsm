use std::path::Path;
use crate::registry::SessionStatus;
use std::time::{Duration, Instant};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::serve::{ServeClient, CreateSession};

/// Start an orchestrator session — creates a ccsm session and resumes opencode TUI.
/// The plugin injects swarm context via inject-scope.
pub fn run_orchestrator(
    goal: &str,
    workspace: &Path,
    _home: &Path,
) -> Result<()> {
    let name = format!("swarm-{}", chrono_timestamp());

    // Create ccsm session with the goal
    eprintln!("  creating orchestrator session: {}", name);
    let status = std::process::Command::new("ccsm")
        .args(["new", &name, "-g", goal])
        .current_dir(workspace)
        .status()
        .context("failed to run ccsm new")?;
    if !status.success() {
        anyhow::bail!("ccsm new failed");
    }

    // Mark it in_progress
    let status = std::process::Command::new("ccsm")
        .args(["start", &name])
        .current_dir(workspace)
        .status()
        .context("failed to run ccsm start")?;
    if !status.success() {
        anyhow::bail!("ccsm start failed");
    }

    // Resume opencode TUI via ccsm resume
    eprintln!("  starting orchestrator: {}", name);
    let status = std::process::Command::new("ccsm")
        .args(["resume", &name])
        .current_dir(workspace)
        .status()
        .context("failed to run ccsm resume")?;

    // Mark completed when opencode exits
    let _ = std::process::Command::new("ccsm")
        .args(["complete", &name])
        .current_dir(workspace)
        .status();

    if !status.success() {
        anyhow::bail!("opencode exited with {}", status);
    }
    Ok(())
}

/// Spawn worker sessions via opencode serve — each goal becomes a parallel session.
pub fn run_spawn(
    goals: &[String],
    timeout_secs: u64,
    workspace: &Path,
    home: &Path,
) -> Result<()> {
    let now = crate::registry::now_iso();
    let identity = crate::registry::resolve_identity().ok();
    let workspace_id = identity.as_ref().map(|i| i.id.as_str()).unwrap_or("default");

    if let Some(ref run_dir) = current_run_dir(home, workspace_id) {
        anyhow::bail!(
            "swarm already running (run dir: {})
Use `ccsm swarm kill` to stop it first.",
            run_dir.display()
        );
    }

    let run_id = now.replace([':', '-'], "_");
    let run_dir = swarm_dir(home, workspace_id).join(&run_id);
    std::fs::create_dir_all(&run_dir)?;

    let port = find_available_port()?;
    eprintln!("  starting opencode serve on port {}...", port);

    let mut cmd = std::process::Command::new("opencode");
    cmd.arg("serve").arg("--port").arg(port.to_string());
    cmd.current_dir(workspace);
    cmd.stdout(std::process::Stdio::from(std::fs::File::create(run_dir.join("server.stdout.log"))?));
    cmd.stderr(std::process::Stdio::from(std::fs::File::create(run_dir.join("server.log"))?));

    let mut server = cmd.spawn().context("failed to start opencode serve")?;
    let server_pid = server.id();

    let client = ServeClient::new(port)?;
    let start = Instant::now();
    let ready = loop {
        if start.elapsed() > Duration::from_secs(30) { break false; }
        if client.health().unwrap_or(false) { break true; }
        std::thread::sleep(Duration::from_millis(500));
    };
    if !ready {
        let _ = server.kill();
        anyhow::bail!("opencode serve did not become ready within 30s");
    }
    eprintln!("  opencode serve ready (pid {})
", server_pid);

    let mut run = SwarmRun {
        goals: goals.to_vec(),
        timeout_secs,
        created: now,
        server_pid: Some(server_pid),
        server_port: Some(port),
        workers: Vec::new(),
    };

    // Create all worker sessions
    for (i, goal) in goals.iter().enumerate() {
        let name = format!("worker-{}", i + 1);
        eprintln!("  creating {} [{}]", name, truncate(goal, 60));
        let session = client.create_session(CreateSession { title: name.clone() })
            .context("failed to create worker session")?;
        run.workers.push(Worker {
            name,
            session_id: Some(session.id),
            status: "pending".to_string(),
            input_tokens: 0,
            output_tokens: 0,
        });
    }
    save_run(&run_dir, &run)?;

    // Send messages to workers (sequential — each blocks until done)
    for i in 0..run.workers.len() {
        let goal = run.goals[i].clone();
        let sid = run.workers[i].session_id.clone().unwrap();
        let name = run.workers[i].name.clone();
        eprintln!("  {} → sending goal...", name);
        run.workers[i].status = "running".to_string();
        save_run(&run_dir, &run)?;

        match client.send_message_as(&sid, &goal, Some("build")) {
            Ok(info) => {
                run.workers[i].status = "done".to_string();
                run.workers[i].input_tokens = info.tokens.input;
                run.workers[i].output_tokens = info.tokens.output;
                eprintln!("  {} ✓ ({} → {} tokens)", name, info.tokens.input, info.tokens.output);
            }
            Err(e) => {
                run.workers[i].status = "failed".to_string();
                eprintln!("  {} ✗ {}", name, e);
            }
        }
        save_run(&run_dir, &run)?;
    }

    // Shutdown
    eprintln!("
  shutting down server...");
    let _ = server.kill();
    let _ = server.wait();

    let total_in: u64 = run.workers.iter().map(|w| w.input_tokens).sum();
    let total_out: u64 = run.workers.iter().map(|w| w.output_tokens).sum();
    let done = run.workers.iter().filter(|w| w.status == "done").count();
    eprintln!("  swarm complete: {}/{} workers done ({} → {} tokens)",
        done, run.workers.len(), total_in, total_out);

    Ok(())
}

// ── Data types ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SwarmRun {
    pub goals: Vec<String>,
    pub timeout_secs: u64,
    pub created: String,
    pub server_pid: Option<u32>,
    pub server_port: Option<u16>,
    pub workers: Vec<Worker>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Worker {
    pub name: String,
    pub session_id: Option<String>,
    pub status: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

// ── Helpers ────────────────────────────────────────────────────────

fn swarm_dir(home: &Path, workspace_id: &str) -> std::path::PathBuf {
    home.join(workspace_id).join("swarm")
}

fn current_run_dir(home: &Path, workspace_id: &str) -> Option<std::path::PathBuf> {
    let dir = swarm_dir(home, workspace_id);
    if !dir.is_dir() { return None; }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let meta_path = e.path().join("meta.json");
            if !meta_path.exists() { return false; }
            if let Ok(meta) = std::fs::read_to_string(&meta_path) {
                if let Ok(r) = serde_json::from_str::<SwarmRun>(&meta) {
                    if let Some(pid) = r.server_pid {
                        return unsafe { libc::kill(pid as i32, 0) } == 0;
                    }
                }
            }
            false
        })
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()));
    entries.first().map(|e| e.path())
}

fn chrono_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", now)
}

fn save_run(run_dir: &Path, run: &SwarmRun) -> Result<()> {
    std::fs::write(run_dir.join("meta.json"), serde_json::to_string_pretty(run)?)?;
    Ok(())
}

fn find_available_port() -> Result<u16> {
    use std::net::TcpListener;
    let l = TcpListener::bind("127.0.0.1:0")?;
    let port = l.local_addr()?.port();
    drop(l);
    Ok(port)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}…", &s[..max]) }
}

// ── Status / List / Kill ───────────────────────────────────────────

pub fn run_status(home: &Path) -> Result<()> {
    let identity = crate::registry::resolve_identity().ok();
    let workspace_id = identity.as_ref().map(|i| i.id.as_str()).unwrap_or("default");

    let run_dir = current_run_dir(home, workspace_id)
        .context("no active swarm run found")?;

    let run: SwarmRun = serde_json::from_str(&std::fs::read_to_string(run_dir.join("meta.json"))?)?;

    println!("Created: {}", run.created);
    println!("Timeout: {}s", run.timeout_secs);

    if let Some(port) = run.server_port {
        let client = ServeClient::new(port)?;
        let status = if client.health().unwrap_or(false) { "running" } else { "dead" };
        println!("Server: {} (port {})", status, port);
    }

    println!("
Workers:");
    for w in &run.workers {
        println!("  {} — {} ({} → {} tokens)", w.name, w.status, w.input_tokens, w.output_tokens);
    }
    Ok(())
}

pub fn run_list(home: &Path) -> Result<()> {
    let identity = crate::registry::resolve_identity().ok();
    let workspace_id = identity.as_ref().map(|i| i.id.as_str()).unwrap_or("default");

    let dir = swarm_dir(home, workspace_id);
    if !dir.is_dir() { println!("No swarm runs found."); return Ok(()); }

    let mut runs: Vec<(String, SwarmRun)> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let meta_path = entry.path().join("meta.json");
        if meta_path.exists() {
            if let Ok(meta) = std::fs::read_to_string(&meta_path) {
                if let Ok(run) = serde_json::from_str::<SwarmRun>(&meta) {
                    runs.push((entry.file_name().to_string_lossy().to_string(), run));
                }
            }
        }
    }
    runs.sort_by(|a, b| b.1.created.cmp(&a.1.created));

    if runs.is_empty() {
        println!("No swarm runs found.");
    } else {
        for (id, run) in &runs {
            let status = match run.server_pid {
                Some(pid) if unsafe { libc::kill(pid as i32, 0) } == 0 => "running",
                _ => "done",
            };
            let done = run.workers.iter().filter(|w| w.status == "done").count();
            println!("{} — {} workers ({}/{}) [{}]", id, run.goals.len(), done, run.workers.len(), status);
        }
    }
    Ok(())
}

pub fn run_kill(home: &Path) -> Result<()> {
    let identity = crate::registry::resolve_identity().ok();
    let workspace_id = identity.as_ref().map(|i| i.id.as_str()).unwrap_or("default");

    let run_dir = current_run_dir(home, workspace_id)
        .context("no active swarm run found")?;

    let mut run: SwarmRun = serde_json::from_str(&std::fs::read_to_string(run_dir.join("meta.json"))?)?;

    if let Some(pid) = run.server_pid {
        if unsafe { libc::kill(pid as i32, 0) } == 0 {
            eprintln!("killing server (pid {})...", pid);
            unsafe { libc::kill(pid as i32, libc::SIGTERM); }
            std::thread::sleep(Duration::from_secs(1));
            unsafe { libc::kill(pid as i32, libc::SIGKILL); }
        }
    }
    for w in &mut run.workers { w.status = "killed".to_string(); }
    run.server_pid = None;
    save_run(&run_dir, &run)?;
    eprintln!("swarm killed.");
    Ok(())
}

pub fn run_orchestrate(
    sessions: &[String],
    goal: &str,
    _workspace: &Path,
    _branch: Option<&str>,
) -> Result<()> {
    let mut reg = crate::registry::WorkspaceRegistry::load()?;

    for name in sessions {
        let entry = reg.sessions.iter().rev()
            .find(|s| s.name == *name)
            .ok_or_else(|| anyhow::anyhow!("session '{name}' not found"))?;
        if entry.status != SessionStatus::Pending {
            anyhow::bail!("session '{name}' is {}, expected pending", entry.status);
        }
    }

    let orch_name = format!("orchestrate-{}", sessions.join("-"));
    let scope = format!("Orchestration — workers:
{}",
        sessions.iter().map(|n| format!("  - {n}: pending")).collect::<Vec<_>>().join("
"));

    let now = crate::registry::now_iso();
    let mut tags = vec!["orchestrate".to_string()];
    for name in sessions {
        tags.push(format!("worker:{name}"));
    }

    reg.sessions.push(crate::registry::WorkspaceSession {
        session_id: String::new(),
        name: orch_name.clone(),
        goal: goal.to_string(),
        scope: scope.clone(),
        status: SessionStatus::InProgress,
        pids: Vec::new(),
        tags,
        started: now.clone(),
        completed: String::new(),
        consumer: String::new(),
        group: None,
        depends_on: Vec::new(),
        branch: String::new(),
        use_worktree: false,
        is_orchestrator: true,
        retired_session_ids: Vec::new(),
    });
    reg.updated = now;
    reg.save()?;

    eprintln!("  orchestrator '{orch_name}' created (pending)");
    eprintln!("  Workers: {}", sessions.join(", "));
    eprintln!("  Run 'ccsm resume {orch_name}' to start orchestrating.");
    Ok(())
}
