use std::sync::Arc;
use tokio::sync::Mutex;

use rmcp::{ErrorData as McpError, schemars, ServiceExt, tool, tool_handler, tool_router};
use rmcp::model::{ServerInfo, ServerCapabilities};

fn workspace_path() -> std::path::PathBuf {
    if let Ok(ws) = std::env::var("CCSM_WORKSPACE") {
        return std::path::PathBuf::from(ws);
    }
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"))
}
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;

mod state;
mod serve;
mod db;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SpawnParam {
    session_names: Vec<String>,
    prompt: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct StatusParam {
    orchestrator_name: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct KillParam {
    orchestrator_name: Option<String>,
    worker_name: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct WaitParam {
    run_id: Option<String>,
    timeout_secs: u64,
}

#[derive(Clone)]
struct SwarmServer {
    state: Arc<Mutex<state::SwarmState>>,
    tool_router: ToolRouter<SwarmServer>,
}

impl SwarmServer {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(state::SwarmState::default())),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl SwarmServer {
    #[tool(name = "swarm-spawn", description = "Spawn worker ccsm sessions via opencode2 serve API. Pre-flight validates all sessions, then spawns each in parallel (non-blocking).")]
    async fn swarm_spawn(
        &self,
        Parameters(SpawnParam { session_names, prompt }): Parameters<SpawnParam>,
    ) -> Result<String, McpError> {
        if session_names.is_empty() {
            return Err(McpError::invalid_params("at least one session name required", None));
        }

        // ── Phase 0: Pre-flight ──────────────────────────────────
        let client = serve::ServeClient::connect()
            .map_err(|e| Self::mcp_err(format!("serve: {e}")))?;

        let ws = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let ws_str = ws.to_string_lossy().to_string();

        // Load ccsm registry to validate sessions
        let id = std::env::var("CCSM_DATA_DIR")
            .ok()
            .or_else(|| {
                let home = std::env::var("HOME").ok()?;
                let dir = std::path::PathBuf::from(&home).join(".ccsm");
                // find workspace id by looking for sessions.json
                std::fs::read_dir(&dir).ok()?.filter_map(|e| {
                    let path = e.ok()?.path();
                    if path.join("sessions.json").exists() {
                        path.file_name()?.to_str().map(String::from)
                    } else {
                        None
                    }
                }).next()
            })
            .unwrap_or_else(|| "default".to_string());

        let sessions_path = std::path::PathBuf::from(
            &std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
        ).join(".ccsm").join(&id).join("sessions.json");

        let sessions_json: serde_json::Value = std::fs::read_to_string(&sessions_path)
            .map_err(|e| Self::mcp_err(format!("read sessions.json: {e}")))
            .and_then(|s| serde_json::from_str(&s).map_err(|e| Self::mcp_err(format!("parse sessions.json: {e}"))))?;

        let sessions = sessions_json["sessions"].as_array()
            .ok_or_else(|| Self::mcp_err("no sessions array in registry"))?;

        // Validate each session: exists, pending, consumer is opencode
        let mut preflight_errors = Vec::new();
        let mut valid_sessions = Vec::new();
        for name in &session_names {
            let entry = sessions.iter().find(|s| s["name"].as_str() == Some(name));
            match entry {
                None => preflight_errors.push(format!("session '{}' not found", name)),
                Some(s) => {
                    let status = s["status"].as_str().unwrap_or("");
                    if status != "pending" {
                        preflight_errors.push(format!("session '{}' is {} (must be pending)", name, status));
                        continue;
                    }
                    let consumer = s["consumer"].as_str().unwrap_or("");
                    if !consumer.is_empty() && consumer != "opencode" {
                        preflight_errors.push(format!(
                            "session '{}' was created for {}. swarm only supports opencode2.",
                            name, consumer
                        ));
                        continue;
                    }
                    let branch = s["branch"].as_str().unwrap_or("").to_string();
                    let goal = s["goal"].as_str().unwrap_or("").to_string();
                    valid_sessions.push((name.clone(), branch, goal));
                }
            }
        }

        if !preflight_errors.is_empty() {
            return Err(Self::mcp_err(format!("pre-flight failed: {}", preflight_errors.join("; "))));
        }

        // ── Phase 1: Spawn workers ───────────────────────────────
        let run_id = format!("run-{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());

        let db = db::SwarmDb::open()
            .map_err(|e| Self::mcp_err(format!("db: {e}")))?;

        // Get orchestrator info from env or default
        let orch_name = std::env::var("CCSM_SESSION").unwrap_or_else(|_| "unknown".into());
        let orch_sid = std::env::var("CCSM_SESSION_ID").unwrap_or_else(|_| "unknown".into());
        let created = Self::now_iso();

        let mut results = Vec::new();
        let prompt_text = prompt.unwrap_or_else(|| "Work on this session.".to_string());

        for (name, branch, _goal) in &valid_sessions {
            // ccsm start <name>
            let start_ok = std::process::Command::new("ccsm")
                .args(["start", name])
                .current_dir(&ws)
                .output()
                .is_ok_and(|o| o.status.success());

            if !start_ok {
                results.push(serde_json::json!({"name": name, "status": "failed", "error": "ccsm start failed"}));
                continue;
            }

            // Check worktree directory
            let worktree_dir = ws.join(".claude").join("worktrees").join(name);
            let _session_dir = if worktree_dir.is_dir() {
                worktree_dir.to_string_lossy().to_string()
            } else {
                ws_str.clone()
            };

            // Create opencode2 session
            let sid = match client.create_session(name) {
                Ok(s) => s.id,
                Err(e) => {
                    results.push(serde_json::json!({"name": name, "status": "failed", "error": format!("create session: {e}")}));
                    continue;
                }
            };

            // Send prompt
            if let Err(e) = client.send_prompt(&sid, &prompt_text) {
                results.push(serde_json::json!({"name": name, "sid": sid, "status": "failed", "error": format!("send prompt: {e}")}));
                continue;
            }

            // Insert into swarm.db
            if let Err(e) = db.insert_worker(&run_id, &orch_name, name, Some(&sid), &created) {
                results.push(serde_json::json!({"name": name, "sid": sid, "status": "warning", "error": format!("db insert: {e}")}));
            }

            // ccsm note <name>
            let _ = std::process::Command::new("ccsm")
                .args(["note", name, &format!("Spawned by orchestrator {} (session {})", orch_name, sid)])
                .current_dir(&ws)
                .output();

            results.push(serde_json::json!({"name": name, "session_id": sid, "status": "running"}));
        }

        let resp = serde_json::json!({"run_id": run_id, "workers": results});
        Ok(serde_json::to_string(&resp).unwrap_or_default())
    }

    fn now_iso() -> String {
        use std::fmt::Write;
        let total = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let secs = total % 86400;
        let days = total / 86400;
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        format!("day{days}T{h:02}:{m:02}:{s:02}Z")
    }

    #[tool(name = "swarm-status", description = "Check status of swarm workers")]
    async fn swarm_status(
        &self,
        Parameters(StatusParam { orchestrator_name }): Parameters<StatusParam>,
    ) -> Result<String, McpError> {
        let db = db::SwarmDb::open()
            .map_err(|e| Self::mcp_err(format!("db: {e}")))?;

        let orch_s = orchestrator_name.clone()
            .or_else(|| std::env::var("CCSM_SESSION").ok())
            .unwrap_or_else(|| "unknown".to_string());
        let orch = orch_s.as_str();

        let workers = db.get_workers(Some(orch), None)
            .map_err(|e| Self::mcp_err(format!("query: {e}")))?;

        if workers.is_empty() {
            return Ok("[]".to_string());
        }

        let client = match serve::ServeClient::connect() {
            Ok(c) => Some(c),
            Err(_) => None,
        };

        let mut results = Vec::new();
        for w in &workers {
            let (tokens_in, tokens_out, cost) = if let (Some(c), Some(sid)) = (&client, &w.worker_sid) {
                if sid.is_empty() {
                    (0u64, 0u64, 0f64)
                } else {
                    match c.get_session(sid) {
                        Ok(detail) => (detail.tokens.input, detail.tokens.output, detail.cost),
                        Err(_) => (0, 0, 0f64),
                    }
                }
            } else {
                (0u64, 0u64, 0f64)
            };

            results.push(serde_json::json!({
                "name": w.worker_name,
                "session_id": w.worker_sid,
                "status": w.status,
                "tokens": { "input": tokens_in, "output": tokens_out },
                "cost": cost
            }));
        }

        Ok(serde_json::to_string(&results).unwrap_or_default())
    }

    fn mcp_err(msg: impl std::fmt::Display) -> McpError {
        McpError::internal_error(msg.to_string(), None)
    }

    #[tool(name = "swarm-kill", description = "Kill active swarm workers via opencode2 serve API")]
    async fn swarm_kill(
        &self,
        Parameters(KillParam { orchestrator_name, worker_name }): Parameters<KillParam>,
    ) -> Result<String, McpError> {
        let db = db::SwarmDb::open().map_err(|e| Self::mcp_err(e))?;

        let workers = db.get_workers(orchestrator_name.as_deref(), worker_name.as_deref())
            .map_err(|e| Self::mcp_err(e))?;

        if workers.is_empty() {
            return Err(Self::mcp_err("no swarm workers found to kill"));
        }

        let client = serve::ServeClient::connect()
            .map_err(|e| Self::mcp_err(e))?;

        let mut killed = Vec::new();
        let mut errors = Vec::new();

        for w in &workers {
            if let Some(ref sid) = w.worker_sid {
                if sid.is_empty() {
                    killed.push(serde_json::json!({"worker": w.worker_name, "status": "no_session"}));
                    continue;
                }
                match client.delete_session(sid) {
                    Ok(()) => {
                        if let Err(e) = db.update_status(&w.run_id, &w.worker_name, "killed") {
                            errors.push(serde_json::json!({"worker": w.worker_name, "error": format!("db update: {e}")}));
                        }
                        killed.push(serde_json::json!({"worker": w.worker_name, "status": "killed"}));
                    }
                    Err(e) => {
                        let msg = format!("{e}");
                        errors.push(serde_json::json!({"worker": w.worker_name, "error": msg}));
                    }
                }
            } else {
                if let Err(e) = db.update_status(&w.run_id, &w.worker_name, "killed") {
                    errors.push(serde_json::json!({"worker": w.worker_name, "error": format!("db update: {e}")}));
                }
                killed.push(serde_json::json!({"worker": w.worker_name, "status": "killed (no session)"}));
            }
        }

        let result = serde_json::json!({"killed": killed, "errors": errors});
        Ok(serde_json::to_string(&result).unwrap_or_default())
    }
}

#[tool_handler(router = self.tool_router)]
impl rmcp::handler::server::ServerHandler for SwarmServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: rmcp::model::Implementation {
                name: "ccsm-swarm".into(),
                version: "0.4.0".into(),
                ..Default::default()
            },
            instructions: None,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = SwarmServer::new();
    let service = server.serve((tokio::io::stdin(), tokio::io::stdout())).await?;
    service.waiting().await?;
    Ok(())
}
